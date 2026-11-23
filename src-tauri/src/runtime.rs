use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use central_llm_core::{
    bundled_origin, llama_command_args, scan_all_model_dirs, write_models_preset, AppSettings,
    LlamaLaunch, DEFAULT_PORT, VRAM_MARGIN_MIB,
};
use tauri::{AppHandle, Manager};

pub struct LlamaProcess {
    child: Option<Child>,
    pub origin: String,
    pub mode: String,
    pub binary_path: Option<String>,
    pub binary_found: bool,
    pub running: bool,
    pub detail: String,
    fingerprint: String,
}

impl LlamaProcess {
    pub fn new() -> Self {
        Self {
            child: None,
            origin: bundled_origin(DEFAULT_PORT),
            mode: "bundled".into(),
            binary_path: None,
            binary_found: false,
            running: false,
            detail: "The local server has not been started.".into(),
            fingerprint: String::new(),
        }
    }
}

impl Drop for LlamaProcess {
    fn drop(&mut self) {
        stop_child(&mut self.child);
    }
}

pub fn force_restart(process: &mut LlamaProcess) {
    process.fingerprint.clear();
    stop_child(&mut process.child);
    process.running = false;
}

pub fn sync_runtime(
    app: &AppHandle,
    process: &mut LlamaProcess,
    settings: &AppSettings,
) -> Result<(), String> {
    let custom = normalize_origin(&settings.llama_server_url);
    let fingerprint = format!(
        "{custom}|{}|{}|{}|{}",
        settings.models_dir,
        settings.extra_model_dirs.join(";"),
        settings.context_length,
        settings.gpu_layers
    );
    if !custom.is_empty() {
        stop_child(&mut process.child);
        process.mode = "custom".into();
        process.origin = custom;
        process.fingerprint = fingerprint;
        process.running = false;
        process.detail = format!("Using your server at {}.", process.origin);
        return Ok(());
    }

    let binary = find_binary(app);
    process.binary_found = binary.is_some();
    process.binary_path = binary.as_ref().map(|path| path.display().to_string());
    process.mode = "bundled".into();
    process.origin = bundled_origin(DEFAULT_PORT);
    let Some(binary) = binary else {
        stop_child(&mut process.child);
        process.running = false;
        process.fingerprint = fingerprint;
        process.detail = "The bundled llama-server was not found. The Windows installer includes it. You can also paste a server URL below.".into();
        return Ok(());
    };

    let alive = process
        .child
        .as_mut()
        .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
    if alive && process.fingerprint == fingerprint {
        process.running = true;
        process.detail = format!("Bundled llama-server is running at {}.", process.origin);
        return Ok(());
    }

    stop_child(&mut process.child);
    std::fs::create_dir_all(&settings.models_dir).map_err(|error| error.to_string())?;
    let preset = app
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("models-preset.ini"));
    if let Some(preset_path) = &preset {
        let models =
            scan_all_model_dirs(Path::new(&settings.models_dir), &settings.extra_model_dirs);
        write_models_preset(
            preset_path,
            &models,
            settings.context_length.max(512),
            settings.gpu_layers,
        )
        .map_err(|error| error.to_string())?;
    }
    let launch = LlamaLaunch {
        binary: binary.clone(),
        models_dir: PathBuf::from(&settings.models_dir),
        preset,
        port: DEFAULT_PORT,
        context: settings.context_length.max(512),
        gpu_layers: settings.gpu_layers,
        memory_margin_mib: VRAM_MARGIN_MIB,
    };
    let mut command = Command::new(&binary);
    command.args(llama_command_args(&launch));
    if let Some(dir) = binary.parent() {
        command.current_dir(dir);
    }
    command.stdin(Stdio::null());
    if let Some(log_path) = app
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("llama-server.log"))
    {
        if let Ok(file) = std::fs::File::create(&log_path) {
            if let Ok(cloned) = file.try_clone() {
                command
                    .stdout(Stdio::from(file))
                    .stderr(Stdio::from(cloned));
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    match command.spawn() {
        Ok(child) => {
            process.child = Some(child);
            process.running = true;
            process.fingerprint = fingerprint;
            process.detail = format!("Starting bundled llama-server at {}.", process.origin);
            Ok(())
        }
        Err(error) => {
            process.running = false;
            process.detail = format!("Could not start llama-server. {error}");
            Err(process.detail.clone())
        }
    }
}

pub struct HealthSnapshot {
    pub origin: String,
    pub probe: bool,
}

pub fn health_snapshot(process: &LlamaProcess) -> HealthSnapshot {
    HealthSnapshot {
        probe: !(process.mode == "bundled" && process.child.is_none() && !process.binary_found),
        origin: process.origin.clone(),
    }
}

pub fn apply_health(process: &mut LlamaProcess, origin: &str, healthy: bool) {
    if process.origin != origin {
        return;
    }
    process.running = healthy;
    if healthy {
        process.detail = if process.mode == "custom" {
            format!("Connected to your server at {}.", process.origin)
        } else {
            format!("Bundled llama-server is running at {}.", process.origin)
        };
    } else if process.mode == "custom" {
        process.detail = format!(
            "Could not reach {}. Check that the server is running.",
            process.origin
        );
    } else if process.binary_found {
        process.detail = format!(
            "llama-server did not answer at {}. The log is llama-server.log in the app data folder.",
            process.origin
        );
    }
}

pub async fn probe_origin(client: &reqwest::Client, origin: &str, wait: bool) -> bool {
    if wait {
        probe_until_ready(client, origin).await
    } else {
        probe_once(client, origin).await
    }
}

async fn probe_once(client: &reqwest::Client, origin: &str) -> bool {
    let url = format!("{}/health", origin.trim_end_matches('/'));
    client
        .get(url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

async fn probe_until_ready(client: &reqwest::Client, origin: &str) -> bool {
    for _ in 0..20 {
        if probe_once(client, origin).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    false
}

pub fn find_binary(app: &AppHandle) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    let mut candidates = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        candidates.push(dir.join("llama").join(name));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("llama").join(name));
            candidates.push(dir.join(name));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn normalize_origin(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    trimmed
        .strip_suffix("/v1")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_string()
}

fn stop_child(slot: &mut Option<Child>) {
    let Some(mut child) = slot.take() else {
        return;
    };
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn validate_origin(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.starts_with("http://") || value.starts_with("https://") {
        Ok(())
    } else {
        Err("The server URL needs to start with http:// or https://.".into())
    }
}

pub fn attachment_root(data_dir: &Path) -> PathBuf {
    data_dir.join("attachments")
}
