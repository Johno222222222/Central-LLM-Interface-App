use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const COMFY_DEFAULT_PATH: &str = r"C:\Users\borna\ComfyUI\Installs\ComfyUI\ComfyUI";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Local,
    Openai,
    Anthropic,
    Gemini,
    Deepseek,
    Openrouter,
}

impl ProviderId {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::Deepseek => "deepseek",
            Self::Openrouter => "openrouter",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Openai => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Gemini",
            Self::Deepseek => "DeepSeek",
            Self::Openrouter => "OpenRouter",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "local" => Some(Self::Local),
            "openai" => Some(Self::Openai),
            "anthropic" => Some(Self::Anthropic),
            "gemini" => Some(Self::Gemini),
            "deepseek" => Some(Self::Deepseek),
            "openrouter" => Some(Self::Openrouter),
            _ => None,
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Self::Local)
    }

    pub fn all_hosted() -> &'static [ProviderId] {
        &[
            Self::Openai,
            Self::Anthropic,
            Self::Gemini,
            Self::Deepseek,
            Self::Openrouter,
        ]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Default,
    Low,
    Medium,
    High,
}

impl Effort {
    pub fn cycle(self) -> Self {
        match self {
            Self::Default => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Default,
        }
    }

    /// Level sent to a provider when Think is on. Default maps to medium so the
    /// toggle changes the request instead of leaving the provider's own default.
    pub fn api_value(self) -> &'static str {
        match self {
            Self::Default | Self::Medium => "medium",
            Self::Low => "low",
            Self::High => "high",
        }
    }

    pub fn anthropic_budget(self) -> u32 {
        match self {
            Self::Low => 1_024,
            Self::Default | Self::Medium => 4_096,
            Self::High => 16_000,
        }
    }
}

impl Default for Effort {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    System,
    Light,
    Dark,
}

impl Default for Appearance {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSettings {
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think_budget: Option<u32>,
    #[serde(default)]
    pub custom_params: Map<String, Value>,
    #[serde(default)]
    pub agent: bool,
    #[serde(default)]
    pub sandbox: bool,
    #[serde(default)]
    pub code: bool,
    #[serde(default)]
    pub voice: bool,
    #[serde(default)]
    pub think: bool,
    #[serde(default)]
    pub effort: Effort,
    /// Per-chat tool toggles. Risky tools stay off until the user turns them on.
    #[serde(default)]
    pub tools: ToolToggles,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolToggles {
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub web_fetch: bool,
    #[serde(default)]
    pub files: bool,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub screenshot: bool,
    #[serde(default)]
    pub mcp: bool,
    /// Reads about this app. On unless the user turns the App group off.
    #[serde(default = "default_true")]
    pub app: bool,
}

impl Default for ToolToggles {
    fn default() -> Self {
        Self {
            web_search: false,
            web_fetch: false,
            files: false,
            shell: false,
            screenshot: false,
            mcp: false,
            app: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolStep {
    pub id: String,
    pub name: String,
    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// running, approval, done, denied, or error
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.95,
            max_tokens: 1_024,
            think_budget: None,
            custom_params: Map::new(),
            agent: false,
            sandbox: false,
            code: false,
            voice: false,
            think: false,
            effort: Effort::Default,
            tools: ToolToggles::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Empty means the app should auto-start the bundled llama-server.
    pub llama_server_url: String,
    pub comfy_host: String,
    pub comfy_port: u32,
    pub comfy_path: String,
    pub models_dir: String,
    pub output_dir: String,
    pub appearance: Appearance,
    pub context_length: u32,
    pub gpu_layers: u32,
    /// Extra GGUF folders scanned in place. The app folder stays `models_dir`.
    #[serde(default)]
    pub extra_model_dirs: Vec<String>,
    /// Folders the Files tool is allowed to read. Empty means the tool cannot read anything.
    #[serde(default)]
    pub approved_folders: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

impl AppSettings {
    pub fn fresh(models_dir: impl Into<String>, output_dir: impl Into<String>) -> Self {
        Self {
            llama_server_url: String::new(),
            comfy_host: "127.0.0.1".into(),
            comfy_port: 8188,
            comfy_path: COMFY_DEFAULT_PATH.into(),
            models_dir: models_dir.into(),
            output_dir: output_dir.into(),
            appearance: Appearance::Dark,
            context_length: 8_192,
            gpu_layers: 99,
            extra_model_dirs: Vec::new(),
            approved_folders: Vec::new(),
            mcp_servers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    pub path: String,
    pub mime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default)]
    pub tool_steps: Vec<ToolStep>,
    /// done, stopped, cut_off, or error. Empty on older messages.
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub stop_reason: String,
    #[serde(default)]
    pub finished_at: String,
    #[serde(default)]
    pub thinking_started_at: String,
    #[serde(default)]
    pub thinking_finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub model_id: String,
    pub provider: ProviderId,
    pub context_used: u32,
    pub context_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub model_id: String,
    pub provider: ProviderId,
    pub messages: Vec<ChatMessage>,
    pub settings: ChatSettings,
    pub context_used: u32,
    pub context_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub provider: ProviderId,
    pub family: String,
    pub context_limit: u32,
    pub installed: bool,
    pub path: Option<String>,
    pub loaded: bool,
    pub key_saved: bool,
    /// Where a local file was found, such as "App folder" or "Locally Uncensored".
    #[serde(default)]
    pub source_label: String,
    /// Largest context the slider offers, usually the model's training window.
    #[serde(default)]
    pub context_max: u32,
    /// Context the app would pick from VRAM and model size, before a manual override.
    #[serde(default)]
    pub context_auto: u32,
    /// Matching vision projector, when one was found beside the model.
    #[serde(default)]
    pub mmproj_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackEntry {
    pub id: String,
    pub chat_id: String,
    pub message_id: String,
    pub rating: String,
    pub note: String,
    pub model_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub provider: ProviderId,
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModel {
    pub id: String,
    pub status: String,
    pub context_limit: Option<u32>,
}
