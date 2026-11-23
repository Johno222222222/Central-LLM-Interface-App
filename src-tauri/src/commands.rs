use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use std::collections::HashMap;

use central_llm_core::{
    catalog_from_ids, clamp_context_step, default_context_limit, default_origin,
    estimate_thread_tokens, friendly_http_error, hosted_catalog, loaded_ids, looks_like_vision,
    new_message, parse_model_ids, parse_router_models, redact, stream_completion, test_provider,
    test_url, title_from_message, tool_guidance, AppSettings, Attachment, CatalogModel, Chat,
    ChatMessage, ChatSettings, ChatSummary, CompletionRequest, ImagePart, OutgoingMessage,
    ProviderId, ProviderStatus, StreamPiece, ToolStep, BRAVE_SEARCH_ACCOUNT, MAX_TOOL_ROUNDS,
};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::keychain::KeychainStore;
use crate::runtime::{
    self, apply_health, health_snapshot, probe_origin, sync_runtime, validate_origin,
};
use crate::tool_runtime::{collect_tools, run_tool};
use crate::AppState;
use central_llm_core::SecretStore;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub mode: String,
    pub url: String,
    pub running: bool,
    pub binary_found: bool,
    pub binary_path: Option<String>,
    pub detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendStarted {
    pub user: ChatMessage,
    pub assistant: ChatMessage,
    pub title: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatDelta {
    chat_id: String,
    message_id: String,
    text: String,
    thinking: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatDone {
    chat_id: String,
    message_id: String,
    text: String,
    thinking: String,
    context_used: u32,
    context_limit: u32,
    error: Option<String>,
    cancelled: bool,
    title: String,
}

fn guard<T>(mutex: &std::sync::Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    mutex
        .lock()
        .map_err(|_| "The app state was interrupted.".to_string())
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<AppSettings, String> {
    guard(&state.store)?
        .get_settings()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    validate_origin(&settings.llama_server_url)?;
    let store = guard(&state.store)?;
    store
        .save_settings(&settings)
        .map_err(|error| error.to_string())?;
    Ok(settings)
}

#[tauri::command]
pub fn provider_status() -> Result<Vec<ProviderStatus>, String> {
    ProviderId::all_hosted()
        .iter()
        .map(|provider| {
            let saved = KeychainStore.has_key(provider.slug()).unwrap_or(false);
            Ok(ProviderStatus {
                provider: *provider,
                saved,
            })
        })
        .collect()
}

#[tauri::command]
pub fn save_provider_key(provider: String, secret: String) -> Result<(), String> {
    KeychainStore
        .set_key(&provider, &secret)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_provider_key(provider: String) -> Result<(), String> {
    KeychainStore
        .delete_key(&provider)
        .map_err(|error| error.to_string())
}

#[tauri::command(rename = "test_provider")]
pub async fn test_provider_connection(
    state: State<'_, AppState>,
    provider: String,
    draft: Option<String>,
) -> Result<String, String> {
    let provider = ProviderId::from_slug(&provider).ok_or("Unknown provider.")?;
    let key = if let Some(draft) = draft.filter(|value| !value.trim().is_empty()) {
        draft
    } else if provider.needs_key() {
        KeychainStore
            .get_key(provider.slug())
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Add a {} key in Settings first.", provider.label()))?
    } else {
        String::new()
    };
    let origin = if provider == ProviderId::Local {
        local_origin(&state)?
    } else {
        default_origin(provider).to_string()
    };
    test_provider(&state.client, provider, &origin, &key)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn runtime_status(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    let snapshot = {
        let process = guard(&state.llama)?;
        health_snapshot(&process)
    };
    if snapshot.probe {
        let healthy = probe_origin(&state.client, &snapshot.origin, false).await;
        let mut process = guard(&state.llama)?;
        apply_health(&mut process, &snapshot.origin, healthy);
    }
    status_from(&state)
}

#[tauri::command]
pub async fn restart_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RuntimeStatus, String> {
    let settings = guard(&state.store)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    {
        let mut process = guard(&state.llama)?;
        runtime::force_restart(&mut process);
        let overrides = guard(&state.store)?
            .model_context_overrides()
            .unwrap_or_default();
        sync_runtime(&app, &mut process, &settings, &overrides)?;
    }
    let snapshot = {
        let process = guard(&state.llama)?;
        health_snapshot(&process)
    };
    if snapshot.probe {
        let healthy = probe_origin(&state.client, &snapshot.origin, true).await;
        let mut process = guard(&state.llama)?;
        apply_health(&mut process, &snapshot.origin, healthy);
    }
    status_from(&state)
}

#[tauri::command]
pub async fn set_models_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<AppSettings, String> {
    if path.trim().is_empty() {
        return Err("Choose a folder for chat models.".into());
    }
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    let mut settings = guard(&state.store)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    settings.models_dir = path;
    guard(&state.store)?
        .save_settings(&settings)
        .map_err(|error| error.to_string())?;
    {
        let mut process = guard(&state.llama)?;
        runtime::force_restart(&mut process);
        let overrides = guard(&state.store)?
            .model_context_overrides()
            .unwrap_or_default();
        sync_runtime(&app, &mut process, &settings, &overrides)?;
    }
    Ok(settings)
}

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> Result<Vec<CatalogModel>, String> {
    let settings = guard(&state.store)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    let overrides = guard(&state.store)?
        .model_context_overrides()
        .unwrap_or_default();
    let mut local = runtime::prepared_models(&settings, &overrides);
    let origin = local_origin(&state).unwrap_or_default();
    if !origin.is_empty() {
        if let Ok(response) = state.client.get(format!("{origin}/models")).send().await {
            if let Ok(body) = response.json::<Value>().await {
                let router = parse_router_models(&body);
                for model in &mut local {
                    if let Some(found) = router
                        .iter()
                        .find(|item| item.id == model.id || item.id.ends_with(&model.name))
                    {
                        model.loaded =
                            matches!(found.status.as_str(), "loaded" | "sleeping" | "loading");
                        if let Some(limit) = found.context_limit {
                            model.context_limit = limit;
                        }
                    }
                }
            }
        }
    }
    let mut hosted = hosted_catalog();
    for model in &mut hosted {
        model.key_saved = KeychainStore
            .has_key(model.provider.slug())
            .unwrap_or(false);
    }
    local.extend(hosted);
    Ok(local)
}

#[tauri::command]
pub async fn refresh_hosted_models(
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<CatalogModel>, String> {
    let provider = ProviderId::from_slug(&provider)
        .filter(|provider| provider.needs_key())
        .ok_or("Choose a hosted provider.")?;
    let key = KeychainStore
        .get_key(provider.slug())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Add a {} key in Settings first.", provider.label()))?;
    let origin = default_origin(provider);
    let mut request = state.client.get(test_url(provider, origin));
    for (name, value) in central_llm_core::build_headers(provider, &key) {
        request = request.header(name, value);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("Could not reach {}. {error}", provider.label()))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(friendly_http_error(provider, status.as_u16(), &redact(&key, &body)));
    }
    let ids = parse_model_ids(provider, &body).map_err(|error| error.to_string())?;
    let mut models = catalog_from_ids(provider, &ids);
    for model in &mut models {
        model.key_saved = true;
    }
    Ok(models)
}

#[tauri::command]
pub async fn unload_all_models(state: State<'_, AppState>) -> Result<String, String> {
    let origin = local_origin(&state)?;
    let response = state
        .client
        .get(format!("{origin}/models"))
        .send()
        .await
        .map_err(|_| "The local llama-server is not running. Check Settings, or point at a server that is already up.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "The local server returned HTTP {} when listing models.",
            response.status()
        ));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "The local server did not return a model list.".to_string())?;
    let ids = loaded_ids(&parse_router_models(&body));
    if ids.is_empty() {
        return Ok("No models are loaded.".into());
    }
    let mut unloaded = 0;
    let mut errors = Vec::new();
    for id in ids {
        match state
            .client
            .post(format!("{origin}/models/unload"))
            .json(&serde_json::json!({ "model": id }))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => unloaded += 1,
            Ok(response) => errors.push(format!("{id} ({})", response.status())),
            Err(error) => errors.push(format!("{id} ({error})")),
        }
    }
    if unloaded == 0 {
        return Err(format!("Could not unload models. {}", errors.join(", ")));
    }
    if errors.is_empty() {
        Ok(format!("Unloaded {unloaded} model(s)."))
    } else {
        Ok(format!(
            "Unloaded {unloaded} model(s). Some stayed loaded: {}",
            errors.join(", ")
        ))
    }
}

#[tauri::command]
pub fn list_chats(state: State<AppState>) -> Result<Vec<ChatSummary>, String> {
    guard(&state.store)?
        .list_chats()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_chat(state: State<AppState>, id: String) -> Result<Chat, String> {
    guard(&state.store)?
        .get_chat(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_chat(
    state: State<AppState>,
    provider: ProviderId,
    model_id: String,
) -> Result<Chat, String> {
    let store = guard(&state.store)?;
    let settings = store.get_settings().map_err(|error| error.to_string())?;
    let overrides = store.model_context_overrides().unwrap_or_default();
    let limit = context_limit(provider, &model_id, &settings, &overrides);
    store
        .create_chat(provider, &model_id, limit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_chat(state: State<AppState>, id: String) -> Result<(), String> {
    guard(&state.store)?
        .delete_chat(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_chat_model(
    state: State<AppState>,
    id: String,
    provider: ProviderId,
    model_id: String,
) -> Result<Chat, String> {
    let store = guard(&state.store)?;
    let settings = store.get_settings().map_err(|error| error.to_string())?;
    let overrides = store.model_context_overrides().unwrap_or_default();
    let limit = context_limit(provider, &model_id, &settings, &overrides);
    store
        .update_chat_model(&id, provider, &model_id, limit)
        .map_err(|error| error.to_string())?;
    store.get_chat(&id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_chat_settings(
    state: State<AppState>,
    id: String,
    settings: ChatSettings,
) -> Result<(), String> {
    guard(&state.store)?
        .update_chat_settings(&id, &settings)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn stage_attachment(state: State<AppState>, source_path: String) -> Result<Attachment, String> {
    let source = PathBuf::from(&source_path);
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Choose an image file.")?
        .to_string();
    let mime = mime_for(&source).ok_or("Attach a png, jpg, gif, or webp image.")?;
    let meta = std::fs::metadata(&source).map_err(|_| "Could not read that image.".to_string())?;
    if meta.len() > 20_000_000 {
        return Err("That image is larger than 20 MB.".into());
    }
    let root = runtime::attachment_root(&state.data_dir);
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let dest = root.join(format!("{stamp}-{name}"));
    std::fs::copy(&source, &dest).map_err(|error| format!("Could not copy that image. {error}"))?;
    Ok(Attachment {
        name,
        path: dest.display().to_string(),
        mime: mime.to_string(),
    })
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    chat_id: String,
    content: String,
    attachment_paths: Vec<String>,
) -> Result<SendStarted, String> {
    let content = content.trim().to_string();
    if content.is_empty() && attachment_paths.is_empty() {
        return Err("Write a message first.".into());
    }
    let chat = guard(&state.store)?
        .get_chat(&chat_id)
        .map_err(|error| error.to_string())?;
    if chat.model_id.trim().is_empty() {
        return Err("Pick a model first.".into());
    }
    let settings = guard(&state.store)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    let origin = if chat.provider == ProviderId::Local {
        local_origin(&state)?
    } else {
        default_origin(chat.provider).to_string()
    };
    let key = if chat.provider.needs_key() {
        KeychainStore
            .get_key(chat.provider.slug())
            .map_err(|error| error.to_string())?
            .unwrap_or_default()
    } else {
        String::new()
    };

    let mut user_images = Vec::new();
    let mut user_files = Vec::new();
    for path in &attachment_paths {
        let (file, image) = load_image(&state.data_dir, path)?;
        user_files.push(file);
        user_images.push(image);
    }

    let mut outgoing = Vec::new();
    for message in &chat.messages {
        let mut images = Vec::new();
        for file in &message.attachments {
            if let Ok((_, image)) = load_image(&state.data_dir, &file.path) {
                images.push(image);
            }
        }
        let mut item = OutgoingMessage::text(message.role.clone(), message.content.clone());
        item.images = images;
        outgoing.push(item);
    }
    let has_new_images = !user_images.is_empty();
    let mut user_turn = OutgoingMessage::text("user", content.clone());
    user_turn.images = user_images;
    outgoing.push(user_turn);

    let user = new_message("user", &content, user_files, None, None);
    let assistant = new_message(
        "assistant",
        "",
        Vec::new(),
        Some(chat.provider),
        Some(chat.model_id.clone()),
    );
    let title = if chat.messages.is_empty() || chat.title == "New chat" {
        title_from_message(&content)
    } else {
        chat.title.clone()
    };
    let overrides = guard(&state.store)?
        .model_context_overrides()
        .unwrap_or_default();
    let has_images = has_new_images || chat_has_images(&chat);
    vision_gate(
        &settings,
        &overrides,
        chat.provider,
        &chat.model_id,
        has_images,
    )?;
    if chat.provider == ProviderId::Local && has_images {
        let mut process = guard(&state.llama)?;
        sync_runtime(&app, &mut process, &settings, &overrides)?;
    }
    let limit = context_limit(chat.provider, &chat.model_id, &settings, &overrides);
    let prior_parts: Vec<String> = chat
        .messages
        .iter()
        .map(|message| message.content.clone())
        .chain(std::iter::once(content.clone()))
        .collect();
    let prior_tokens = estimate_thread_tokens(&prior_parts);
    {
        let store = guard(&state.store)?;
        store
            .insert_message(&chat.id, &user)
            .map_err(|error| error.to_string())?;
        store
            .insert_message(&chat.id, &assistant)
            .map_err(|error| error.to_string())?;
        store
            .touch_chat(&chat.id, Some(&title), chat.context_used, limit)
            .map_err(|error| error.to_string())?;
    }

    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut cancels = guard(&state.cancels)?;
        if let Some(previous) = cancels.insert(chat.id.clone(), flag.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }

    let request = CompletionRequest {
        provider: chat.provider,
        model: chat.model_id.clone(),
        messages: outgoing,
        settings: chat.settings.clone(),
        origin,
        tools: Vec::new(),
    };
    let client = state.client.clone();
    let chat_id = chat.id.clone();
    let assistant_id = assistant.id.clone();
    let app_task = app.clone();
    tauri::async_runtime::spawn(async move {
        run_stream(
            app_task,
            client,
            request,
            key,
            flag,
            chat_id,
            assistant_id,
            title,
            limit,
            prior_tokens,
        )
        .await;
    });

    Ok(SendStarted {
        user,
        assistant,
        title: title_from_message_if_needed(&chat, &content),
    })
}

fn title_from_message_if_needed(chat: &Chat, content: &str) -> String {
    if chat.messages.is_empty() || chat.title == "New chat" {
        title_from_message(content)
    } else {
        chat.title.clone()
    }
}

async fn run_stream(
    app: AppHandle,
    client: reqwest::Client,
    mut request: CompletionRequest,
    key: String,
    flag: Arc<AtomicBool>,
    chat_id: String,
    assistant_id: String,
    title: String,
    limit: u32,
    prior_tokens: u32,
) {
    let settings = app
        .state::<AppState>()
        .store
        .lock()
        .ok()
        .and_then(|store| store.get_settings().ok());
    let Some(settings) = settings else {
        return;
    };
    request.tools = collect_tools(&request.settings.tools, &settings);
    if !request.tools.is_empty() {
        request.messages.insert(
            0,
            OutgoingMessage::text(
                "system",
                tool_guidance(
                    &settings.approved_folders,
                    request.tools.iter().any(|tool| tool.name == "get_app_state"),
                ),
            ),
        );
    }
    let mut steps: Vec<ToolStep> = Vec::new();
    let mut final_text = String::new();
    let mut final_thinking = String::new();
    let mut error = None;
    let mut cancelled = false;
    let mut used = prior_tokens;

    for _ in 0..MAX_TOOL_ROUNDS {
        if flag.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let mut text = String::new();
        let mut thinking = String::new();
        let mut last_save = Instant::now();
        let save_app = app.clone();
        let save_id = assistant_id.clone();
        let chat_for_delta = chat_id.clone();
        let message_for_delta = assistant_id.clone();
        let result = stream_completion(&client, &request, &key, &flag, |piece| {
            match piece {
                StreamPiece::Text(chunk) => {
                    text.push_str(chunk);
                    let _ = app.emit(
                        "chat-delta",
                        ChatDelta {
                            chat_id: chat_for_delta.clone(),
                            message_id: message_for_delta.clone(),
                            text: chunk.clone(),
                            thinking: String::new(),
                        },
                    );
                }
                StreamPiece::Thinking(chunk) => {
                    thinking.push_str(chunk);
                    let _ = app.emit(
                        "chat-delta",
                        ChatDelta {
                            chat_id: chat_for_delta.clone(),
                            message_id: message_for_delta.clone(),
                            text: String::new(),
                            thinking: chunk.clone(),
                        },
                    );
                }
                StreamPiece::Usage { .. }
                | StreamPiece::Error(_)
                | StreamPiece::ToolDelta { .. } => {}
            }
            if last_save.elapsed() > Duration::from_millis(700) {
                persist_partial(&save_app, &save_id, &text, &thinking);
                last_save = Instant::now();
            }
        })
        .await;

        let outcome = match result {
            Ok(outcome) => outcome,
            Err(failure) => {
                error = Some(failure.to_string());
                break;
            }
        };
        final_text = outcome.text;
        final_thinking = outcome.thinking;
        cancelled = outcome.cancelled;
        if let Some(prompt) = outcome.prompt_tokens {
            used = prompt;
        }
        if outcome.tool_calls.is_empty() || cancelled {
            break;
        }
        let mut assistant_turn = OutgoingMessage::text("assistant", final_text.clone());
        assistant_turn.tool_calls = outcome.tool_calls.clone();
        request.messages.push(assistant_turn);
        for call in outcome.tool_calls {
            let mut step = ToolStep {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
                result: None,
                status: "running".into(),
            };
            if central_llm_core::needs_command_approval(&call.name) {
                if call.name == "update_settings" {
                    match crate::app_tools::describe_setting_change(&app, &chat_id, &call.arguments)
                    {
                        Ok(summary) => step.arguments = summary,
                        Err(reason) => {
                            step.status = "error".into();
                            step.result = Some(reason.clone());
                            steps.push(step.clone());
                            emit_step(&app, &chat_id, &assistant_id, &step);
                            let mut rejected = OutgoingMessage::text("tool", reason);
                            rejected.tool_call_id = Some(call.id);
                            rejected.tool_name = Some(call.name);
                            request.messages.push(rejected);
                            continue;
                        }
                    }
                }
                step.status = "approval".into();
                emit_step(&app, &chat_id, &assistant_id, &step);
                let approved = wait_approval(&app, &call.id).await;
                if !approved {
                    step.status = "denied".into();
                    step.result = Some("You denied this.".into());
                    steps.push(step.clone());
                    emit_step(&app, &chat_id, &assistant_id, &step);
                    let mut denied = OutgoingMessage::text("tool", "The user denied this action.");
                    denied.tool_call_id = Some(call.id);
                    denied.tool_name = Some(call.name);
                    request.messages.push(denied);
                    continue;
                }
            }
            let result_text = run_tool(&app, &call.name, &call.arguments, &settings, &chat_id).await;
            step.status = "done".into();
            step.result = Some(result_text.clone());
            steps.push(step.clone());
            emit_step(&app, &chat_id, &assistant_id, &step);
            let mut tool_message = OutgoingMessage::text("tool", result_text);
            tool_message.tool_call_id = Some(call.id);
            tool_message.tool_name = Some(call.name.clone());
            request.messages.push(tool_message);
        }
        if let Ok(store) = app.state::<AppState>().store.lock() {
            let _ = store.save_tool_steps(
                &assistant_id,
                &final_text,
                if final_thinking.is_empty() {
                    None
                } else {
                    Some(final_thinking.as_str())
                },
                &steps,
            );
        }
    }

    if let Ok(store) = app.state::<AppState>().store.lock() {
        let _ = store.save_tool_steps(
            &assistant_id,
            &final_text,
            if final_thinking.is_empty() {
                None
            } else {
                Some(final_thinking.as_str())
            },
            &steps,
        );
        let _ = store.touch_chat(&chat_id, Some(&title), used, limit);
    }
    let _ = app.emit(
        "chat-done",
        ChatDone {
            chat_id,
            message_id: assistant_id,
            text: final_text,
            thinking: final_thinking,
            context_used: used,
            context_limit: limit,
            error,
            cancelled,
            title,
        },
    );
}

fn emit_step(app: &AppHandle, chat_id: &str, message_id: &str, step: &ToolStep) {
    let _ = app.emit(
        "tool-step",
        serde_json::json!({
            "chatId": chat_id,
            "messageId": message_id,
            "step": step,
        }),
    );
}

async fn wait_approval(app: &AppHandle, call_id: &str) -> bool {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    if let Ok(mut approvals) = app.state::<AppState>().approvals.lock() {
        approvals.insert(call_id.to_string(), sender);
    }
    let _ = app.emit("tool-approval", serde_json::json!({ "callId": call_id }));
    matches!(
        tokio::time::timeout(Duration::from_secs(180), receiver).await,
        Ok(Ok(true))
    )
}

fn persist_partial(app: &AppHandle, message_id: &str, text: &str, thinking: &str) {
    let state = app.state::<AppState>();
    let locked = state.store.lock();
    if let Ok(store) = locked {
        let _ = store.update_message(
            message_id,
            text,
            if thinking.is_empty() {
                None
            } else {
                Some(thinking)
            },
        );
    }
}

#[tauri::command]
pub fn set_ui_state(state: State<AppState>, screen: String, focus: String) -> Result<(), String> {
    let mut ui = state.ui.lock().map_err(|_| "The app is busy.".to_string())?;
    ui.screen = screen;
    ui.focus = focus;
    Ok(())
}

#[tauri::command]
pub fn resolve_tool_approval(
    state: State<AppState>,
    call_id: String,
    approved: bool,
) -> Result<(), String> {
    if let Some(sender) = guard(&state.approvals)?.remove(&call_id) {
        let _ = sender.send(approved);
    }
    Ok(())
}

#[tauri::command]
pub fn detect_lu_folder() -> Result<Option<String>, String> {
    let roaming = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs_fallback());
    let Some(roaming) = roaming else {
        return Ok(None);
    };
    Ok(central_llm_core::detect_lu_model_folder(&roaming))
}

fn dirs_fallback() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| {
        std::path::PathBuf::from(home)
            .join("AppData")
            .join("Roaming")
    })
}

#[tauri::command]
pub fn web_search_status() -> Result<bool, String> {
    Ok(KeychainStore
        .get_account(BRAVE_SEARCH_ACCOUNT)
        .map(|value| value.is_some())
        .unwrap_or(false))
}

#[tauri::command]
pub fn save_web_search_key(secret: String) -> Result<(), String> {
    KeychainStore
        .set_account(BRAVE_SEARCH_ACCOUNT, &secret)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_web_search_key() -> Result<(), String> {
    KeychainStore
        .delete_account(BRAVE_SEARCH_ACCOUNT)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn test_web_search(app: AppHandle) -> Result<String, String> {
    let settings = guard(&app.state::<AppState>().store)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    let answer =
        crate::tool_runtime::run_tool(&app, "web_search", r#"{"query":"llama.cpp"}"#, &settings, "")
            .await;
    if answer.starts_with("Add a Brave")
        || answer.starts_with("Web search returned HTTP")
        || answer.starts_with("Web search could not")
    {
        Err(answer)
    } else {
        Ok("Brave Search answered.".into())
    }
}

#[tauri::command]
pub fn stop_generation(state: State<AppState>, chat_id: String) -> Result<(), String> {
    if let Some(flag) = guard(&state.cancels)?.get(&chat_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

fn status_from(state: &AppState) -> Result<RuntimeStatus, String> {
    let process = guard(&state.llama)?;
    Ok(RuntimeStatus {
        mode: process.mode.clone(),
        url: process.origin.clone(),
        running: process.running,
        binary_found: process.binary_found,
        binary_path: process.binary_path.clone(),
        detail: process.detail.clone(),
    })
}

fn local_origin(state: &AppState) -> Result<String, String> {
    let settings = guard(&state.store)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    let custom = settings.llama_server_url.trim().trim_end_matches('/');
    if custom.is_empty() {
        Ok(guard(&state.llama)?.origin.clone())
    } else {
        Ok(custom
            .strip_suffix("/v1")
            .unwrap_or(custom)
            .trim_end_matches('/')
            .to_string())
    }
}

#[tauri::command]
pub async fn edit_message(
    app: AppHandle,
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    content: String,
) -> Result<SendStarted, String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("Write a message first.".into());
    }
    {
        let store = guard(&state.store)?;
        store
            .update_message(&message_id, &content, None)
            .map_err(|error| error.to_string())?;
        store
            .delete_messages_from(&chat_id, &message_id, false)
            .map_err(|error| error.to_string())?;
    }
    spawn_reply(app, &state, &chat_id)
}

#[tauri::command]
pub async fn regenerate_message(
    app: AppHandle,
    state: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    model_id: Option<String>,
    provider: Option<String>,
) -> Result<SendStarted, String> {
    if let Some(model_id) = model_id.filter(|id| !id.trim().is_empty()) {
        let chat = guard(&state.store)?
            .get_chat(&chat_id)
            .map_err(|error| error.to_string())?;
        let provider = provider
            .as_deref()
            .and_then(ProviderId::from_slug)
            .unwrap_or(chat.provider);
        let settings = guard(&state.store)?
            .get_settings()
            .map_err(|error| error.to_string())?;
        let overrides = guard(&state.store)?
            .model_context_overrides()
            .unwrap_or_default();
        let limit = context_limit(provider, &model_id, &settings, &overrides);
        guard(&state.store)?
            .update_chat_model(&chat_id, provider, &model_id, limit)
            .map_err(|error| error.to_string())?;
    }
    guard(&state.store)?
        .delete_messages_from(&chat_id, &message_id, true)
        .map_err(|error| error.to_string())?;
    spawn_reply(app, &state, &chat_id)
}

#[tauri::command]
pub fn branch_chat(
    state: State<AppState>,
    chat_id: String,
    message_id: String,
) -> Result<Chat, String> {
    let store = guard(&state.store)?;
    let source = store.get_chat(&chat_id).map_err(|error| error.to_string())?;
    let end = source
        .messages
        .iter()
        .position(|message| message.id == message_id)
        .ok_or("That message is no longer in the chat.")?;
    let branched = store
        .create_chat(source.provider, &source.model_id, source.context_limit)
        .map_err(|error| error.to_string())?;
    store
        .update_chat_settings(&branched.id, &source.settings)
        .map_err(|error| error.to_string())?;
    for message in source.messages.iter().take(end + 1) {
        let mut copy = message.clone();
        copy.id = new_message(
            &message.role,
            &message.content,
            message.attachments.clone(),
            message.provider,
            message.model_id.clone(),
        )
        .id;
        store
            .insert_message(&branched.id, &copy)
            .map_err(|error| error.to_string())?;
    }
    let title = format!("{} (branch)", source.title);
    store
        .touch_chat(&branched.id, Some(&title), source.context_used, source.context_limit)
        .map_err(|error| error.to_string())?;
    store
        .get_chat(&branched.id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_feedback(
    state: State<AppState>,
    chat_id: String,
    message_id: String,
    rating: String,
    note: String,
) -> Result<(), String> {
    let store = guard(&state.store)?;
    let chat = store.get_chat(&chat_id).map_err(|error| error.to_string())?;
    store
        .save_feedback(&chat_id, &message_id, &rating, note.trim(), &chat.model_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_feedback(state: State<AppState>) -> Result<String, String> {
    let rows = guard(&state.store)?
        .list_feedback()
        .map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&rows).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_model_context(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
    context: u32,
) -> Result<CatalogModel, String> {
    let settings = guard(&state.store)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    let overrides = guard(&state.store)?
        .model_context_overrides()
        .unwrap_or_default();
    let current = runtime::prepared_models(&settings, &overrides);
    let model = current
        .iter()
        .find(|model| model.id == model_id)
        .cloned()
        .ok_or("That model is not in a scanned folder.")?;
    let chosen = clamp_context_step(context, model.context_max.max(2_048));
    guard(&state.store)?
        .set_model_context(&model_id, chosen)
        .map_err(|error| error.to_string())?;
    let overrides = guard(&state.store)?
        .model_context_overrides()
        .unwrap_or_default();
    {
        let mut process = guard(&state.llama)?;
        runtime::force_restart(&mut process);
        sync_runtime(&app, &mut process, &settings, &overrides)?;
    }
    runtime::prepared_models(&settings, &overrides)
        .into_iter()
        .find(|model| model.id == model_id)
        .ok_or_else(|| "That model is not in a scanned folder.".into())
}

fn spawn_reply(app: AppHandle, state: &AppState, chat_id: &str) -> Result<SendStarted, String> {
    let chat = guard(&state.store)?
        .get_chat(chat_id)
        .map_err(|error| error.to_string())?;
    if chat.model_id.trim().is_empty() {
        return Err("Pick a model first.".into());
    }
    let Some(user) = chat
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .cloned()
    else {
        return Err("There is no message to resend.".into());
    };
    let settings = guard(&state.store)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    let overrides = guard(&state.store)?
        .model_context_overrides()
        .unwrap_or_default();
    vision_gate(
        &settings,
        &overrides,
        chat.provider,
        &chat.model_id,
        chat_has_images(&chat),
    )?;
    let origin = if chat.provider == ProviderId::Local {
        local_origin(state)?
    } else {
        default_origin(chat.provider).to_string()
    };
    let key = if chat.provider.needs_key() {
        KeychainStore
            .get_key(chat.provider.slug())
            .map_err(|error| error.to_string())?
            .unwrap_or_default()
    } else {
        String::new()
    };
    let mut outgoing = Vec::new();
    for message in &chat.messages {
        let mut images = Vec::new();
        for file in &message.attachments {
            if let Ok((_, image)) = load_image(&state.data_dir, &file.path) {
                images.push(image);
            }
        }
        let mut item = OutgoingMessage::text(message.role.clone(), message.content.clone());
        item.images = images;
        outgoing.push(item);
    }
    let assistant = new_message(
        "assistant",
        "",
        Vec::new(),
        Some(chat.provider),
        Some(chat.model_id.clone()),
    );
    let limit = context_limit(chat.provider, &chat.model_id, &settings, &overrides);
    let prior_parts: Vec<String> = chat.messages.iter().map(|message| message.content.clone()).collect();
    let prior_tokens = estimate_thread_tokens(&prior_parts);
    guard(&state.store)?
        .insert_message(&chat.id, &assistant)
        .map_err(|error| error.to_string())?;
    guard(&state.store)?
        .touch_chat(&chat.id, None, chat.context_used, limit)
        .map_err(|error| error.to_string())?;
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut cancels = guard(&state.cancels)?;
        if let Some(previous) = cancels.insert(chat.id.clone(), flag.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }
    let request = CompletionRequest {
        provider: chat.provider,
        model: chat.model_id.clone(),
        messages: outgoing,
        settings: chat.settings.clone(),
        origin,
        tools: Vec::new(),
    };
    let client = state.client.clone();
    let chat_id = chat.id.clone();
    let assistant_id = assistant.id.clone();
    let title = chat.title.clone();
    tauri::async_runtime::spawn(async move {
        run_stream(
            app,
            client,
            request,
            key,
            flag,
            chat_id,
            assistant_id,
            title,
            limit,
            prior_tokens,
        )
        .await;
    });
    Ok(SendStarted {
        user,
        assistant,
        title: chat.title,
    })
}

fn context_limit(
    provider: ProviderId,
    model_id: &str,
    settings: &AppSettings,
    overrides: &HashMap<String, u32>,
) -> u32 {
    if provider == ProviderId::Local {
        runtime::prepared_models(settings, overrides)
            .into_iter()
            .find(|model| model.id == model_id)
            .map(|model| model.context_limit)
            .unwrap_or(settings.context_length)
            .max(512)
    } else {
        default_context_limit(provider, model_id)
    }
}

fn chat_has_images(chat: &Chat) -> bool {
    chat.messages.iter().any(|message| !message.attachments.is_empty())
}

fn vision_gate(
    settings: &AppSettings,
    overrides: &HashMap<String, u32>,
    provider: ProviderId,
    model_id: &str,
    has_images: bool,
) -> Result<(), String> {
    if provider != ProviderId::Local || !has_images {
        return Ok(());
    }
    let models = runtime::prepared_models(settings, overrides);
    let Some(model) = models.iter().find(|model| model.id == model_id) else {
        return Ok(());
    };
    if model.mmproj_path.is_some() {
        return Ok(());
    }
    if looks_like_vision(&model.name) {
        Err("This vision model needs a matching mmproj GGUF next to the model, or in a scanned folder.".into())
    } else {
        Err("This model can't see images. Pick a vision model, or put a matching mmproj GGUF next to it.".into())
    }
}

fn load_image(data_dir: &Path, path: &str) -> Result<(Attachment, ImagePart), String> {
    let path = PathBuf::from(path);
    let canon = path
        .canonicalize()
        .map_err(|_| "That image is no longer available.".to_string())?;
    let root = runtime::attachment_root(data_dir)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !canon.starts_with(&root) {
        return Err("Images have to be attached from the composer.".into());
    }
    let mime = mime_for(&canon).ok_or("Attach a png, jpg, gif, or webp image.")?;
    let bytes = std::fs::read(&canon).map_err(|_| "Could not read that image.".to_string())?;
    if bytes.len() > 20_000_000 {
        return Err("That image is larger than 20 MB.".into());
    }
    let name = canon
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image")
        .to_string();
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok((
        Attachment {
            name,
            path: canon.display().to_string(),
            mime: mime.to_string(),
        },
        ImagePart {
            mime: mime.to_string(),
            data_base64,
        },
    ))
}

fn mime_for(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        _ => None,
    }
}
