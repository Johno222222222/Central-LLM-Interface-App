use central_llm_core::{
    open_tab_name, propose_setting, provider_key_line, quant_label, settings_report, SecretStore,
    SettingSnapshot, SettingValue, ToolToggles,
};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::keychain::KeychainStore;
use crate::runtime::{self, prepared_models};
use crate::AppState;

pub fn describe_setting_change(app: &AppHandle, chat_id: &str, arguments: &str) -> Result<String, String> {
    let proposal = proposal_from(app, chat_id, arguments)?;
    Ok(format!(
        "{}: {} → {}",
        proposal.label, proposal.old_value, proposal.new_value
    ))
}

pub fn apply_setting_change(app: &AppHandle, chat_id: &str, arguments: &str) -> Result<String, String> {
    let proposal = proposal_from(app, chat_id, arguments)?;
    let summary = format!(
        "Changed {}: {} → {}.",
        proposal.label, proposal.old_value, proposal.new_value
    );
    match proposal.setting.as_str() {
        "appearance" | "contextLength" | "gpuLayers" | "extraModelDirs" | "llamaServerUrl"
        | "modelsDir" | "outputDir" => {
            let mut settings = with_store(app, |store| store.get_settings().map_err(|error| error.to_string()))?;
            match proposal.setting.as_str() {
                "appearance" => {
                    settings.appearance = match proposal.value {
                        SettingValue::Text(ref text) if text == "light" => {
                            central_llm_core::Appearance::Light
                        }
                        SettingValue::Text(ref text) if text == "system" => {
                            central_llm_core::Appearance::System
                        }
                        _ => central_llm_core::Appearance::Dark,
                    };
                }
                "contextLength" => {
                    if let SettingValue::Number(value) = proposal.value {
                        settings.context_length = value;
                    }
                }
                "gpuLayers" => {
                    if let SettingValue::Number(value) = proposal.value {
                        settings.gpu_layers = value;
                    }
                }
                "extraModelDirs" => {
                    if let SettingValue::Paths(paths) = proposal.value.clone() {
                        settings.extra_model_dirs = paths;
                    }
                }
                "llamaServerUrl" => {
                    if let SettingValue::Text(ref text) = proposal.value {
                        settings.llama_server_url = text.clone();
                    }
                }
                "modelsDir" => {
                    if let SettingValue::Text(ref text) = proposal.value {
                        std::fs::create_dir_all(text).map_err(|error| error.to_string())?;
                        settings.models_dir = text.clone();
                    }
                }
                "outputDir" => {
                    if let SettingValue::Text(ref text) = proposal.value {
                        std::fs::create_dir_all(text).map_err(|error| error.to_string())?;
                        settings.output_dir = text.clone();
                    }
                }
                _ => {}
            }
            with_store(app, |store| store.save_settings(&settings).map_err(|error| error.to_string()))?;
            if proposal.restarts_server {
                restart(app, &settings)?;
            }
        }
        "temperature" | "topP" | "maxTokens" | "think" | "effort" | "tool.webSearch"
        | "tool.webFetch" | "tool.screenshot" | "tool.mcp" | "tool.app" | "tool.shell"
        | "tool.files" => {
            let mut chat = with_store(app, |store| store.get_chat(chat_id).map_err(|error| error.to_string()))?;
            apply_chat_field(&mut chat.settings, &proposal);
            with_store(app, |store| {
                store
                    .update_chat_settings(chat_id, &chat.settings)
                    .map_err(|error| error.to_string())
            })?;
        }
        "model" | "provider" => {
            let chat = with_store(app, |store| store.get_chat(chat_id).map_err(|error| error.to_string()))?;
            let (provider, model_id) = match proposal.setting.as_str() {
                "provider" => {
                    let provider = central_llm_core::ProviderId::from_slug(&proposal.new_value)
                        .ok_or("Choose a known provider.")?;
                    (provider, chat.model_id.clone())
                }
                _ => (
                    chat.provider,
                    match &proposal.value {
                        SettingValue::Text(text) => text.clone(),
                        _ => chat.model_id.clone(),
                    },
                ),
            };
            let settings = with_store(app, |store| store.get_settings().map_err(|error| error.to_string()))?;
            let overrides = with_store(app, |store| Ok(store.model_context_overrides().unwrap_or_default()))?;
            let limit = if provider == central_llm_core::ProviderId::Local {
                prepared_models(&settings, &overrides)
                    .into_iter()
                    .find(|model| model.id == model_id)
                    .map(|model| model.context_limit)
                    .unwrap_or(settings.context_length)
            } else {
                central_llm_core::default_context_limit(provider, &model_id)
            };
            with_store(app, |store| {
                store
                    .update_chat_model(chat_id, provider, &model_id, limit)
                    .map_err(|error| error.to_string())
            })?;
        }
        "modelContext" => {
            let SettingValue::Number(value) = proposal.value else {
                return Err("Context needs a whole number.".into());
            };
            let settings = with_store(app, |store| store.get_settings().map_err(|error| error.to_string()))?;
            let overrides = with_store(app, |store| Ok(store.model_context_overrides().unwrap_or_default()))?;
            let models = prepared_models(&settings, &overrides);
            let ceiling = models
                .iter()
                .find(|model| model.id == proposal.model_id)
                .map(|model| model.context_max.max(2048))
                .unwrap_or(131_072);
            let chosen = central_llm_core::clamp_context_step(value, ceiling);
            with_store(app, |store| {
                store
                    .set_model_context(&proposal.model_id, chosen)
                    .map_err(|error| error.to_string())
            })?;
            restart(app, &settings)?;
        }
        other => return Err(format!("Could not apply {other}.")),
    }
    let _ = app.emit("app-settings-changed", json!({ "chatId": chat_id }));
    Ok(summary)
}

pub fn app_state_text(app: &AppHandle, chat_id: &str) -> String {
    let screen = app
        .state::<AppState>()
        .ui
        .lock()
        .map(|ui| ui.screen.clone())
        .unwrap_or_else(|_| "chat".into());
    let focus = app
        .state::<AppState>()
        .ui
        .lock()
        .map(|ui| ui.focus.clone())
        .unwrap_or_default();
    let Ok(chat) = with_store(app, |store| store.get_chat(chat_id).map_err(|error| error.to_string())) else {
        return format!("tab: {screen}\nversion: {}\nNo chat is open.", env!("CARGO_PKG_VERSION"));
    };
    let tools = tool_names(&chat.settings.tools);
    let folders = if chat_settings_folders(app).is_empty() {
        "(none)".into()
    } else {
        chat_settings_folders(app).join("; ")
    };
    format!(
        "tab: {screen}\nfocus: {}\nmodel: {}\nprovider: {}\ncontext: {}\ntools: {tools}\napproved folders: {folders}\nversion: {}",
        if focus.is_empty() { "(none)" } else { focus.as_str() },
        chat.model_id,
        chat.provider.slug(),
        chat.context_limit,
        env!("CARGO_PKG_VERSION")
    )
}

pub fn settings_text(app: &AppHandle, chat_id: &str) -> String {
    let Ok(view) = snapshot(app, chat_id, "") else {
        return "Settings are not available.".into();
    };
    let keys = central_llm_core::ProviderId::all_hosted()
        .iter()
        .map(|provider| {
            let saved = KeychainStore
                .get_key(provider.slug())
                .ok()
                .flatten();
            provider_key_line(provider.slug(), saved.as_deref())
        })
        .collect::<Vec<_>>();
    settings_report(&view, &keys)
}

pub fn models_text(app: &AppHandle) -> String {
    let Ok(settings) = with_store(app, |store| store.get_settings().map_err(|error| error.to_string()))
    else {
        return "Models are not available.".into();
    };
    let overrides = with_store(app, |store| Ok(store.model_context_overrides().unwrap_or_default())).unwrap_or_default();
    let mut lines = Vec::new();
    for model in prepared_models(&settings, &overrides)
        .into_iter()
        .filter(|model| model.provider == central_llm_core::ProviderId::Local)
        .take(40)
    {
        let bytes = model
            .path
            .as_deref()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|meta| meta.len())
            .unwrap_or(0);
        lines.push(format!(
            "{} | {} | {} | {} | ctx {} | mmproj {}",
            model.name,
            format_bytes(bytes),
            quant_label(&model.name),
            if model.source_label.is_empty() {
                "local"
            } else {
                model.source_label.as_str()
            },
            model.context_limit,
            if model.mmproj_path.is_some() { "yes" } else { "no" }
        ));
    }
    if lines.is_empty() {
        lines.push("No local GGUF files.".into());
    }
    lines.push("Cloud keys:".into());
    for provider in central_llm_core::ProviderId::all_hosted() {
        let saved = KeychainStore.has_key(provider.slug()).unwrap_or(false);
        lines.push(format!(
            "{} | key {}",
            provider.slug(),
            if saved { "set" } else { "not set" }
        ));
    }
    lines.join("\n")
}

pub fn threads_text(app: &AppHandle) -> String {
    let Ok(chats) = with_store(app, |store| store.list_chats().map_err(|error| error.to_string()))
    else {
        return "Chats are not available.".into();
    };
    if chats.is_empty() {
        return "No chats yet.".into();
    }
    chats
        .into_iter()
        .take(40)
        .map(|chat| format!("{} | updated {}", chat.title, chat.updated_at))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn server_status_text(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let Ok(process) = state.llama.lock() else {
        return "The local server status is unavailable.".into();
    };
    let used = runtime::detect_vram_used_mib();
    let total = runtime::detect_vram_bytes() / (1024 * 1024);
    format!(
        "mode: {}\nrunning: {}\nurl: {}\ndetail: {}\nvram used: {}\nvram total: {total} MiB",
        process.mode,
        if process.running { "yes" } else { "no" },
        process.origin,
        process.detail,
        used.map(|value| format!("{value} MiB"))
            .unwrap_or_else(|| "unavailable".into())
    )
}

pub fn open_tab(app: &AppHandle, arguments: &str) -> String {
    let args: Value = serde_json::from_str(arguments).unwrap_or(json!({}));
    let tab = args.get("tab").and_then(|item| item.as_str()).unwrap_or("");
    let Some(tab) = open_tab_name(tab) else {
        return "Open Chat, Create, Models, or Settings.".into();
    };
    let focus = args
        .get("focus")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if let Ok(mut ui) = app.state::<AppState>().ui.lock() {
        ui.screen = tab.into();
        ui.focus = focus.clone();
    }
    let _ = app.emit(
        "app-navigate",
        json!({ "tab": tab, "focus": focus }),
    );
    if focus.is_empty() {
        format!("Opened {tab}.")
    } else {
        format!("Opened {tab} at {focus}.")
    }
}

fn proposal_from(
    app: &AppHandle,
    chat_id: &str,
    arguments: &str,
) -> Result<central_llm_core::SettingProposal, String> {
    let args: Value = serde_json::from_str(arguments).unwrap_or(json!({}));
    let setting = args
        .get("setting")
        .and_then(|item| item.as_str())
        .unwrap_or("");
    let value = json_string(args.get("value"));
    let model_id = args
        .get("modelId")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_string();
    let mut view = snapshot(app, chat_id, &model_id)?;
    if canonical_is_model_context(setting) {
        if let Ok(parsed) = value.parse::<u32>() {
            let settings = with_store(app, |store| store.get_settings().map_err(|error| error.to_string()))?;
            let overrides = with_store(app, |store| Ok(store.model_context_overrides().unwrap_or_default()))?;
            if let Some(model) = prepared_models(&settings, &overrides)
                .into_iter()
                .find(|model| model.id == model_id)
            {
                let chosen = central_llm_core::clamp_context_step(parsed, model.context_max.max(2048));
                return propose_setting(setting, &chosen.to_string(), &model_id, &view);
            }
        }
    }
    let _ = &mut view;
    propose_setting(setting, &value, &model_id, &view)
}

fn canonical_is_model_context(setting: &str) -> bool {
    let key: String = setting
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect();
    key == "modelcontext" || key == "modelctx"
}

fn snapshot(app: &AppHandle, chat_id: &str, model_id: &str) -> Result<SettingSnapshot, String> {
    let (settings, chat, overrides) = with_store(app, |store| {
        Ok((
            store.get_settings().map_err(|error| error.to_string())?,
            store.get_chat(chat_id).map_err(|error| error.to_string())?,
            store.model_context_overrides().unwrap_or_default(),
        ))
    })?;
    let model_context = if model_id.is_empty() {
        None
    } else {
        overrides.get(model_id).copied().or_else(|| {
            prepared_models(&settings, &overrides)
                .into_iter()
                .find(|model| model.id == model_id)
                .map(|model| model.context_limit)
        })
    };
    Ok(SettingSnapshot {
        appearance: appearance_name(settings.appearance).into(),
        context_length: settings.context_length,
        gpu_layers: settings.gpu_layers,
        extra_model_dirs: settings.extra_model_dirs,
        llama_server_url: settings.llama_server_url,
        models_dir: settings.models_dir,
        output_dir: settings.output_dir,
        approved_folders: settings.approved_folders,
        temperature: chat.settings.temperature,
        top_p: chat.settings.top_p,
        max_tokens: chat.settings.max_tokens,
        think: chat.settings.think,
        effort: effort_name(chat.settings.effort).into(),
        model_id: chat.model_id,
        provider: chat.provider.slug().into(),
        tools: chat.settings.tools,
        model_context,
    })
}

fn apply_chat_field(settings: &mut central_llm_core::ChatSettings, proposal: &central_llm_core::SettingProposal) {
    match proposal.setting.as_str() {
        "temperature" => {
            if let SettingValue::Decimal(value) = proposal.value {
                settings.temperature = value;
            }
        }
        "topP" => {
            if let SettingValue::Decimal(value) = proposal.value {
                settings.top_p = value;
            }
        }
        "maxTokens" => {
            if let SettingValue::Number(value) = proposal.value {
                settings.max_tokens = value;
            }
        }
        "think" => {
            if let SettingValue::Flag(value) = proposal.value {
                settings.think = value;
            }
        }
        "effort" => {
            if let SettingValue::Text(ref text) = proposal.value {
                settings.effort = match text.as_str() {
                    "low" => central_llm_core::Effort::Low,
                    "medium" => central_llm_core::Effort::Medium,
                    "high" => central_llm_core::Effort::High,
                    _ => central_llm_core::Effort::Default,
                };
            }
        }
        "tool.webSearch" => {
            if let SettingValue::Flag(value) = proposal.value {
                settings.tools.web_search = value;
            }
        }
        "tool.webFetch" => {
            if let SettingValue::Flag(value) = proposal.value {
                settings.tools.web_fetch = value;
            }
        }
        "tool.screenshot" => {
            if let SettingValue::Flag(value) = proposal.value {
                settings.tools.screenshot = value;
            }
        }
        "tool.mcp" => {
            if let SettingValue::Flag(value) = proposal.value {
                settings.tools.mcp = value;
            }
        }
        "tool.app" => {
            if let SettingValue::Flag(value) = proposal.value {
                settings.tools.app = value;
            }
        }
        "tool.shell" => {
            if let SettingValue::Flag(false) = proposal.value {
                settings.tools.shell = false;
            }
        }
        "tool.files" => {
            if let SettingValue::Flag(false) = proposal.value {
                settings.tools.files = false;
            }
        }
        _ => {}
    }
}

fn restart(app: &AppHandle, settings: &central_llm_core::AppSettings) -> Result<(), String> {
    let overrides = with_store(app, |store| Ok(store.model_context_overrides().unwrap_or_default()))?;
    let state = app.state::<AppState>();
    let mut process = state.llama.lock().map_err(|_| "The local server is busy.")?;
    runtime::force_restart(&mut process);
    runtime::sync_runtime(app, &mut process, settings, &overrides)
}

fn with_store<T>(
    app: &AppHandle,
    body: impl FnOnce(&central_llm_core::Store) -> Result<T, String>,
) -> Result<T, String> {
    let state = app.state::<AppState>();
    let store = state.store.lock().map_err(|_| "The app is busy.".to_string())?;
    body(&store)
}

fn chat_settings_folders(app: &AppHandle) -> Vec<String> {
    with_store(app, |store| {
        Ok(store.get_settings().map(|settings| settings.approved_folders).unwrap_or_default())
    })
    .unwrap_or_default()
}

fn tool_names(tools: &ToolToggles) -> String {
    let mut names = Vec::new();
    if tools.app {
        names.push("app");
    }
    if tools.web_search {
        names.push("web search");
    }
    if tools.web_fetch {
        names.push("web fetch");
    }
    if tools.screenshot {
        names.push("screenshot");
    }
    if tools.mcp {
        names.push("mcp");
    }
    if tools.files {
        names.push("files");
    }
    if tools.shell {
        names.push("shell");
    }
    if names.is_empty() {
        "(none)".into()
    } else {
        names.join(", ")
    }
}

fn appearance_name(appearance: central_llm_core::Appearance) -> &'static str {
    match appearance {
        central_llm_core::Appearance::System => "system",
        central_llm_core::Appearance::Light => "light",
        central_llm_core::Appearance::Dark => "dark",
    }
}

fn effort_name(effort: central_llm_core::Effort) -> &'static str {
    match effort {
        central_llm_core::Effort::Low => "low",
        central_llm_core::Effort::Medium => "medium",
        central_llm_core::Effort::High => "high",
        central_llm_core::Effort::Default => "default",
    }
}

fn json_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.0} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{bytes} B")
    }
}
