mod app_tools;
mod commands;
mod keychain;
mod runtime;
mod tool_runtime;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use central_llm_core::{AppSettings, Store};
use tauri::Manager;

use crate::runtime::{apply_health, health_snapshot, probe_origin, stop, sync_runtime};
use std::sync::atomic::AtomicBool;

pub struct UiState {
    pub screen: String,
    pub focus: String,
}

pub struct AppState {
    pub store: Mutex<Store>,
    pub client: reqwest::Client,
    pub llama: Mutex<runtime::LlamaProcess>,
    pub data_dir: PathBuf,
    pub cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub approvals: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
    pub ui: Mutex<UiState>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(data.join("models"))?;
            std::fs::create_dir_all(data.join("outputs"))?;
            std::fs::create_dir_all(data.join("attachments"))?;
            let defaults = AppSettings::fresh(
                data.join("models").display().to_string(),
                data.join("outputs").display().to_string(),
            );
            let store = Store::open(&data.join("studio.db"), &defaults)
                .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })?;
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .build()
                .expect("http client");
            app.manage(AppState {
                store: Mutex::new(store),
                client,
                llama: Mutex::new(runtime::LlamaProcess::new()),
                data_dir: data,
                cancels: Mutex::new(HashMap::new()),
                approvals: Mutex::new(HashMap::new()),
                ui: Mutex::new(UiState {
                    screen: "chat".into(),
                    focus: String::new(),
                }),
            });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                let settings = state
                    .store
                    .lock()
                    .ok()
                    .and_then(|store| store.get_settings().ok());
                if let Some(settings) = settings {
                    let overrides = state
                        .store
                        .lock()
                        .ok()
                        .and_then(|store| store.model_context_overrides().ok())
                        .unwrap_or_default();
                    if let Ok(mut process) = state.llama.lock() {
                        let _ = sync_runtime(&handle, &mut process, &settings, &overrides);
                    }
                    let snapshot = state
                        .llama
                        .lock()
                        .ok()
                        .map(|process| health_snapshot(&process));
                    if let Some(snapshot) = snapshot {
                        if snapshot.probe {
                            let healthy = probe_origin(&state.client, &snapshot.origin, true).await;
                            if let Ok(mut process) = state.llama.lock() {
                                apply_health(&mut process, &snapshot.origin, healthy);
                            }
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::provider_status,
            commands::save_provider_key,
            commands::clear_provider_key,
            commands::test_provider_connection,
            commands::runtime_status,
            commands::restart_runtime,
            commands::set_models_dir,
            commands::list_models,
            commands::refresh_hosted_models,
            commands::unload_all_models,
            commands::list_chats,
            commands::get_chat,
            commands::create_chat,
            commands::delete_chat,
            commands::update_chat_model,
            commands::update_chat_settings,
            commands::stage_attachment,
            commands::send_message,
            commands::edit_message,
            commands::regenerate_message,
            commands::branch_chat,
            commands::save_feedback,
            commands::export_feedback,
            commands::set_model_context,
            commands::set_ui_state,
            commands::stop_generation,
            commands::resolve_tool_approval,
            commands::detect_lu_folder,
            commands::web_search_status,
            commands::save_web_search_key,
            commands::clear_web_search_key,
            commands::test_web_search,
        ])
        .build(tauri::generate_context!())
        .expect("Central LLM Interface App failed to start")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut process) = state.llama.lock() {
                        stop(&mut process);
                    }
                }
            }
        });
}
