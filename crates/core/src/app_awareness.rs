//! What the chat model is allowed to read and change about this app.
//!
//! Reads are plain text with no secrets. A change is a [`SettingProposal`]
//! the shell shows to the user before it writes anything.

use crate::ToolToggles;

#[derive(Debug, Clone, PartialEq)]
pub struct SettingSnapshot {
    pub appearance: String,
    pub context_length: u32,
    pub gpu_layers: u32,
    pub extra_model_dirs: Vec<String>,
    pub llama_server_url: String,
    pub models_dir: String,
    pub output_dir: String,
    pub approved_folders: Vec<String>,
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u32,
    pub think: bool,
    pub effort: String,
    pub model_id: String,
    pub provider: String,
    pub tools: ToolToggles,
    /// Current context for the model named in this change, when there is one.
    pub model_context: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Text(String),
    Number(u32),
    Decimal(f64),
    Flag(bool),
    Paths(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingProposal {
    pub setting: String,
    pub label: String,
    pub old_value: String,
    pub new_value: String,
    pub value: SettingValue,
    pub restarts_server: bool,
    pub model_id: String,
}

pub fn propose_setting(
    setting: &str,
    raw_value: &str,
    model_id: &str,
    view: &SettingSnapshot,
) -> Result<SettingProposal, String> {
    if setting_is_forbidden(setting) {
        return Err(
            "That setting stays in Settings. Chat cannot change API keys, approved folders, or Shell and Files."
                .into(),
        );
    }
    let key = canonical(setting);
    let value = raw_value.trim();
    let proposal = match key.as_str() {
        "appearance" | "theme" => text_choice(
            "appearance",
            "Theme",
            &view.appearance,
            value,
            &["dark", "light", "system"],
            false,
        )?,
        "contextlength" | "ctx" | "context" => number_setting(
            "contextLength",
            "Fallback context",
            view.context_length,
            value,
            512,
            131_072,
            true,
        )?,
        "gpulayers" => number_setting(
            "gpuLayers",
            "GPU layers",
            view.gpu_layers,
            value,
            0,
            999,
            true,
        )?,
        "extramodeldirectories" | "extramodelders" | "extramodelfolders" => {
            let paths = parse_paths(value);
            path_setting(
                "extraModelDirs",
                "Extra model folders",
                &view.extra_model_dirs,
                paths,
                true,
            )?
        }
        "llamaserverurl" | "serverurl" => {
            if !(value.is_empty() || value.starts_with("http://") || value.starts_with("https://"))
            {
                return Err("The server URL needs to start with http:// or https://.".into());
            }
            text_setting(
                "llamaServerUrl",
                "Local server URL",
                &view.llama_server_url,
                value,
                true,
            )?
        }
        "modelsdir" | "modelsfolder" => {
            require_path(value)?;
            text_setting("modelsDir", "Models folder", &view.models_dir, value, true)?
        }
        "outputdir" | "outputfolder" => {
            require_path(value)?;
            text_setting("outputDir", "Output folder", &view.output_dir, value, false)?
        }
        "temperature" => decimal_setting("temperature", "Temperature", view.temperature, value, 0.0, 2.0)?,
        "topp" => decimal_setting("topP", "Top P", view.top_p, value, 0.0, 1.0)?,
        "maxtokens" => number_setting(
            "maxTokens",
            "Max tokens",
            view.max_tokens,
            value,
            1,
            32_768,
            false,
        )?,
        "think" => flag_setting("think", "Think", view.think, value, false)?,
        "effort" => text_choice(
            "effort",
            "Effort",
            &view.effort,
            value,
            &["default", "low", "medium", "high"],
            false,
        )?,
        "model" | "defaultmodel" => {
            if value.is_empty() {
                return Err("Name the model.".into());
            }
            text_setting("model", "Selected model", &view.model_id, value, false)?
        }
        "provider" => text_choice(
            "provider",
            "Provider",
            &view.provider,
            value,
            &["local", "openai", "anthropic", "gemini", "deepseek", "openrouter"],
            false,
        )?,
        "modelcontext" | "modelctx" => {
            if model_id.trim().is_empty() {
                return Err("Name the model whose context should change.".into());
            }
            let old = view.model_context.unwrap_or(0);
            let mut proposal = number_setting("modelContext", "Model context", old, value, 2048, 131_072, true)?;
            proposal.model_id = model_id.trim().to_string();
            if old == 0 {
                proposal.old_value = "auto".into();
            }
            proposal
        }
        "toolwebsearch" | "toolswebsearch" => tool_flag("tool.webSearch", "Web search", view.tools.web_search, value)?,
        "toolwebfetch" | "toolswebfetch" => tool_flag("tool.webFetch", "Web fetch", view.tools.web_fetch, value)?,
        "toolscreenshot" | "toolsscreenshot" => {
            tool_flag("tool.screenshot", "Screenshot", view.tools.screenshot, value)?
        }
        "toolmcp" | "toolsmcp" => tool_flag("tool.mcp", "MCP", view.tools.mcp, value)?,
        "toolapp" | "toolsapp" => tool_flag("tool.app", "App", view.tools.app, value)?,
        "toolshell" | "toolsshell" | "shell" => {
            if parse_bool(value).unwrap_or(true) {
                return Err("Shell stays off until you turn it on in the Tools menu.".into());
            }
            tool_flag("tool.shell", "Shell", view.tools.shell, "false")?
        }
        "toolfiles" | "toolsfiles" | "files" => {
            if parse_bool(value).unwrap_or(true) {
                return Err("Files stays off until you turn it on in the Tools menu.".into());
            }
            tool_flag("tool.files", "Files", view.tools.files, "false")?
        }
        _ => {
            return Err(
                "That setting is not in the list chat can change. Theme, context, sampling, tools, and model folders are."
                    .into(),
            );
        }
    };
    Ok(proposal)
}

pub fn setting_is_forbidden(setting: &str) -> bool {
    let key = canonical(setting);
    if key.is_empty() {
        return false;
    }
    key.contains("approvedfolder")
        || key.contains("shellapproval")
        || key.contains("approvalmode")
        || key.contains("mcpserver")
        || key.contains("password")
        || key.contains("credential")
        || key.contains("secret")
        || key.contains("apikey")
        || key.contains("accesstoken")
        || key == "token"
        || key.ends_with("key")
}

/// `set` or `not set`. The secret itself is never copied into the result.
pub fn provider_key_line(provider: &str, secret: Option<&str>) -> String {
    let saved = secret.is_some_and(|value| !value.trim().is_empty());
    format!(
        "{provider}: {}",
        if saved { "set" } else { "not set" }
    )
}

pub fn settings_report(view: &SettingSnapshot, key_lines: &[String]) -> String {
    let mut lines = vec![
        format!("appearance: {}", view.appearance),
        format!("contextLength: {}", view.context_length),
        format!("gpuLayers: {}", view.gpu_layers),
        format!("modelsDir: {}", view.models_dir),
        format!("outputDir: {}", view.output_dir),
        format!(
            "llamaServerUrl: {}",
            if view.llama_server_url.is_empty() {
                "(bundled)"
            } else {
                view.llama_server_url.as_str()
            }
        ),
        format!("extraModelDirs: {}", join_paths(&view.extra_model_dirs)),
        format!(
            "approvedFolders: {} (change these yourself in Settings)",
            join_paths(&view.approved_folders)
        ),
        format!("temperature: {}", view.temperature),
        format!("topP: {}", view.top_p),
        format!("maxTokens: {}", view.max_tokens),
        format!("think: {}", view.think),
        format!("effort: {}", view.effort),
        format!("model: {}", view.model_id),
        format!("provider: {}", view.provider),
        format!(
            "tools: app={} webSearch={} webFetch={} screenshot={} mcp={} files={} shell={}",
            view.tools.app,
            view.tools.web_search,
            view.tools.web_fetch,
            view.tools.screenshot,
            view.tools.mcp,
            view.tools.files,
            view.tools.shell
        ),
        "API keys:".into(),
    ];
    if key_lines.is_empty() {
        lines.push("(none reported)".into());
    } else {
        lines.extend(key_lines.iter().cloned());
    }
    lines.join("\n")
}

pub fn quant_label(filename: &str) -> String {
    let upper = filename.to_ascii_uppercase();
    const TOKENS: &[&str] = &[
        "IQ4_XS", "IQ4_NL", "IQ3_M", "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q4_K_M",
        "Q4_K_S", "Q4_0", "Q3_K_M", "Q3_K_S", "Q2_K", "BF16", "F16", "F32",
    ];
    TOKENS
        .iter()
        .find(|token| upper.contains(*token))
        .copied()
        .unwrap_or("unknown")
        .to_string()
}

pub fn open_tab_name(tab: &str) -> Option<&'static str> {
    match canonical(tab).as_str() {
        "chat" => Some("chat"),
        "create" => Some("create"),
        "models" => Some("models"),
        "settings" => Some("settings"),
        _ => None,
    }
}

fn canonical(setting: &str) -> String {
    setting
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn text_choice(
    setting: &str,
    label: &str,
    old: &str,
    value: &str,
    allowed: &[&str],
    restarts_server: bool,
) -> Result<SettingProposal, String> {
    let value = value.to_ascii_lowercase();
    if !allowed.contains(&value.as_str()) {
        return Err(format!("{label} must be one of {}.", allowed.join(", ")));
    }
    text_setting(setting, label, old, &value, restarts_server)
}

fn text_setting(
    setting: &str,
    label: &str,
    old: &str,
    value: &str,
    restarts_server: bool,
) -> Result<SettingProposal, String> {
    Ok(SettingProposal {
        setting: setting.into(),
        label: label.into(),
        old_value: if old.is_empty() { "(empty)".into() } else { old.into() },
        new_value: if value.is_empty() { "(empty)".into() } else { value.into() },
        value: SettingValue::Text(value.into()),
        restarts_server,
        model_id: String::new(),
    })
}

fn number_setting(
    setting: &str,
    label: &str,
    old: u32,
    value: &str,
    min: u32,
    max: u32,
    restarts_server: bool,
) -> Result<SettingProposal, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("{label} needs a whole number."))?;
    if parsed < min || parsed > max {
        return Err(format!("{label} must be between {min} and {max}."));
    }
    Ok(SettingProposal {
        setting: setting.into(),
        label: label.into(),
        old_value: old.to_string(),
        new_value: parsed.to_string(),
        value: SettingValue::Number(parsed),
        restarts_server,
        model_id: String::new(),
    })
}

fn decimal_setting(
    setting: &str,
    label: &str,
    old: f64,
    value: &str,
    min: f64,
    max: f64,
) -> Result<SettingProposal, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("{label} needs a number."))?;
    if !(parsed >= min && parsed <= max) {
        return Err(format!("{label} must be between {min} and {max}."));
    }
    Ok(SettingProposal {
        setting: setting.into(),
        label: label.into(),
        old_value: trim_float(old),
        new_value: trim_float(parsed),
        value: SettingValue::Decimal(parsed),
        restarts_server: false,
        model_id: String::new(),
    })
}

fn flag_setting(
    setting: &str,
    label: &str,
    old: bool,
    value: &str,
    restarts_server: bool,
) -> Result<SettingProposal, String> {
    let parsed = parse_bool(value).ok_or_else(|| format!("{label} needs true or false."))?;
    Ok(SettingProposal {
        setting: setting.into(),
        label: label.into(),
        old_value: old.to_string(),
        new_value: parsed.to_string(),
        value: SettingValue::Flag(parsed),
        restarts_server,
        model_id: String::new(),
    })
}

fn tool_flag(setting: &str, label: &str, old: bool, value: &str) -> Result<SettingProposal, String> {
    flag_setting(setting, label, old, value, false)
}

fn path_setting(
    setting: &str,
    label: &str,
    old: &[String],
    paths: Vec<String>,
    restarts_server: bool,
) -> Result<SettingProposal, String> {
    Ok(SettingProposal {
        setting: setting.into(),
        label: label.into(),
        old_value: join_paths(old),
        new_value: join_paths(&paths),
        value: SettingValue::Paths(paths),
        restarts_server,
        model_id: String::new(),
    })
}

fn parse_paths(value: &str) -> Vec<String> {
    if let Ok(items) = serde_json::from_str::<Vec<String>>(value) {
        return items
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }
    if value.trim().is_empty() {
        Vec::new()
    } else {
        vec![value.trim().to_string()]
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn require_path(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err("That folder path is empty.".into())
    } else {
        Ok(())
    }
}

fn join_paths(paths: &[String]) -> String {
    if paths.is_empty() {
        "(none)".into()
    } else {
        paths.join("; ")
    }
}

fn trim_float(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SettingSnapshot {
        SettingSnapshot {
            appearance: "dark".into(),
            context_length: 8192,
            gpu_layers: 99,
            extra_model_dirs: vec![],
            llama_server_url: String::new(),
            models_dir: r"C:\models".into(),
            output_dir: r"C:\out".into(),
            approved_folders: vec![r"C:\".into()],
            temperature: 0.7,
            top_p: 0.95,
            max_tokens: 1024,
            think: false,
            effort: "default".into(),
            model_id: "Qwen2.5-7B-Q4_K_M.gguf".into(),
            provider: "local".into(),
            tools: ToolToggles::default(),
            model_context: Some(32768),
        }
    }

    #[test]
    fn whitelist_allows_theme_and_blocks_secrets_and_risky_tools() {
        let view = sample();
        let theme = propose_setting("theme", "light", "", &view).unwrap();
        assert_eq!(theme.old_value, "dark");
        assert_eq!(theme.new_value, "light");
        assert_eq!(theme.setting, "appearance");

        assert!(propose_setting("approvedFolders", r"D:\", "", &view).is_err());
        assert!(propose_setting("openaiApiKey", "sk-live-secret", "", &view).is_err());
        assert!(propose_setting("api_key", "sk-live-secret", "", &view).is_err());
        assert!(propose_setting("shellApproval", "off", "", &view).is_err());
        assert!(propose_setting("mcpServers", "[]", "", &view).is_err());
        assert!(propose_setting("tool.shell", "true", "", &view).is_err());
        assert!(propose_setting("tool.files", "on", "", &view).is_err());
        assert!(propose_setting("appearance", "sk-live-secret", "", &view).is_err());

        let shell_off = propose_setting("tool.shell", "false", "", &view).unwrap();
        assert_eq!(shell_off.new_value, "false");
        let search = propose_setting("tool.webSearch", "true", "", &view).unwrap();
        assert_eq!(search.setting, "tool.webSearch");
        let ctx = propose_setting("modelContext", "16384", "Qwen2.5-7B-Q4_K_M.gguf", &view).unwrap();
        assert_eq!(ctx.old_value, "32768");
        assert_eq!(ctx.new_value, "16384");
        assert!(ctx.restarts_server);
    }

    #[test]
    fn settings_report_never_prints_a_secret() {
        let secret = "sk-live-secret-DO_NOT_LEAK";
        let saved = provider_key_line("deepseek", Some(secret));
        assert_eq!(saved, "deepseek: set");
        assert!(!saved.contains(secret));
        let missing = provider_key_line("openai", Some("   "));
        assert_eq!(missing, "openai: not set");
        let report = settings_report(&sample(), &[saved, missing]);
        assert!(!report.contains(secret));
        assert!(report.contains("deepseek: set"));
        assert!(report.contains("openai: not set"));
        assert!(report.contains("appearance: dark"));
        assert!(!report.to_lowercase().contains("apikey"));
        assert_eq!(quant_label("Qwen2.5-7B-Instruct-Q4_K_M.gguf"), "Q4_K_M");
    }
}
