use std::path::PathBuf;

/// Port for the bundled server. ComfyUI usually takes 8188, llama.cpp's own
/// default has moved around, so this app picks a port of its own.
pub const DEFAULT_PORT: u16 = 17860;

/// Leave about 2 GB of VRAM free on a 12 GB card so the desktop stays usable.
pub const VRAM_MARGIN_MIB: u32 = 2_048;

#[derive(Debug, Clone)]
pub struct LlamaLaunch {
    pub binary: PathBuf,
    pub models_dir: PathBuf,
    /// When set, the server loads these GGUF paths in place, including extra folders.
    pub preset: Option<PathBuf>,
    pub port: u16,
    pub context: u32,
    pub gpu_layers: u32,
    pub memory_margin_mib: u32,
}

pub fn llama_command_args(launch: &LlamaLaunch) -> Vec<String> {
    let mut args = vec![
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        launch.port.to_string(),
        "--models-max".into(),
        "1".into(),
        "--models-memory-margin".into(),
        launch.memory_margin_mib.to_string(),
        "-ngl".into(),
        launch.gpu_layers.to_string(),
        "-c".into(),
        launch.context.to_string(),
    ];
    if let Some(preset) = &launch.preset {
        args.push("--models-preset".into());
        args.push(preset.display().to_string());
    }
    args.push("--models-dir".into());
    args.push(launch.models_dir.display().to_string());
    args
}

pub fn bundled_origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_points_at_the_app_models_folder() {
        let launch = LlamaLaunch {
            binary: PathBuf::from(
                r"C:\Program Files\Central LLM Interface App\llama\llama-server.exe",
            ),
            models_dir: PathBuf::from(
                r"C:\Users\Johno\AppData\Roaming\app.central-llm-interface\models",
            ),
            preset: Some(PathBuf::from(
                r"C:\Users\Johno\AppData\Roaming\app.central-llm-interface\models-preset.ini",
            )),
            port: DEFAULT_PORT,
            context: 8192,
            gpu_layers: 99,
            memory_margin_mib: VRAM_MARGIN_MIB,
        };
        assert!(launch
            .binary
            .to_string_lossy()
            .ends_with("llama-server.exe"));
        let args = llama_command_args(&launch);
        let joined = args.join(" ");
        assert!(joined.contains("--models-preset"));
        assert!(joined.contains("--models-dir"));
        assert!(joined.contains(r"app.central-llm-interface\models"));
        assert!(!joined.to_lowercase().contains("comfyui"));
        assert!(joined.contains("--models-max 1"));
        assert!(joined.contains("--port 17860"));
        assert_eq!(bundled_origin(DEFAULT_PORT), "http://127.0.0.1:17860");
    }
}
