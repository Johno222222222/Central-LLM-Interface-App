//! Built-in tool registry.
//!
//! Add a tool by appending a [`ToolSpec`] in [`builtin_tools`] and handling its
//! name in the desktop shell (`src-tauri/src/tool_runtime.rs`). The shell is
//! the only place that touches the network, disk, or processes. See `docs/TOOLS.md`.

use serde_json::{json, Value};

use crate::ToolToggles;

pub const BRAVE_SEARCH_ACCOUNT: &str = "web-search";
pub const MAX_TOOL_ROUNDS: u32 = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub title: String,
    pub description: String,
    pub parameters: Value,
    /// Shell and files start disabled and need an extra user decision.
    pub risky: bool,
}

pub fn builtin_tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "web_search".into(),
            title: "Web search".into(),
            description: "Search the web. Returns a few titles and URLs.".into(),
            parameters: object_schema(
                json!({
                    "query": {"type": "string", "description": "What to search for"}
                }),
                &["query"],
            ),
            risky: false,
        },
        ToolSpec {
            name: "web_fetch".into(),
            title: "Web fetch".into(),
            description: "Fetch one public http(s) page as text.".into(),
            parameters: object_schema(
                json!({
                    "url": {"type": "string", "description": "Absolute http or https URL"}
                }),
                &["url"],
            ),
            risky: false,
        },
        ToolSpec {
            name: "files".into(),
            title: "Files".into(),
            description: "List or read a file inside an approved folder. Cannot write.".into(),
            parameters: object_schema(
                json!({
                    "action": {"type": "string", "enum": ["list", "read"]},
                    "path": {"type": "string", "description": "Absolute path inside an approved folder"}
                }),
                &["action", "path"],
            ),
            risky: true,
        },
        ToolSpec {
            name: "shell".into(),
            title: "Shell".into(),
            description: "Run one command after the user approves it.".into(),
            parameters: object_schema(
                json!({
                    "command": {"type": "string", "description": "The command line to run"}
                }),
                &["command"],
            ),
            risky: true,
        },
        ToolSpec {
            name: "screenshot".into(),
            title: "Screenshot".into(),
            description: "Capture the primary screen.".into(),
            parameters: object_schema(json!({}), &[]),
            risky: false,
        },
        ToolSpec {
            name: "get_app_state".into(),
            title: "App state".into(),
            description: "Current tab, model, context, enabled tools, and version.".into(),
            parameters: object_schema(json!({}), &[]),
            risky: false,
        },
        ToolSpec {
            name: "list_settings".into(),
            title: "Settings".into(),
            description: "Settings values. Provider keys are only set or not set.".into(),
            parameters: object_schema(json!({}), &[]),
            risky: false,
        },
        ToolSpec {
            name: "list_models".into(),
            title: "Models".into(),
            description: "Local GGUF files and which cloud keys are saved.".into(),
            parameters: object_schema(json!({}), &[]),
            risky: false,
        },
        ToolSpec {
            name: "list_threads".into(),
            title: "Chats".into(),
            description: "Chat titles and dates.".into(),
            parameters: object_schema(json!({}), &[]),
            risky: false,
        },
        ToolSpec {
            name: "get_server_status".into(),
            title: "Server".into(),
            description: "llama-server status and VRAM in use.".into(),
            parameters: object_schema(json!({}), &[]),
            risky: false,
        },
        ToolSpec {
            name: "update_settings".into(),
            title: "Change setting".into(),
            description: "Change one allowed setting. The user must approve.".into(),
            parameters: object_schema(
                json!({
                    "setting": {"type": "string", "description": "appearance, contextLength, temperature, tool.webSearch, extraModelDirs, modelContext, model, provider"},
                    "value": {"type": "string", "description": "New value"},
                    "modelId": {"type": "string", "description": "Model id when setting is modelContext"}
                }),
                &["setting", "value"],
            ),
            risky: false,
        },
        ToolSpec {
            name: "open_tab".into(),
            title: "Open tab".into(),
            description: "Open Chat, Create, Models, or Settings.".into(),
            parameters: object_schema(
                json!({
                    "tab": {"type": "string", "enum": ["chat", "create", "models", "settings"]},
                    "focus": {"type": "string", "description": "Optional setting to scroll to"}
                }),
                &["tab"],
            ),
            risky: false,
        },
    ]
}

pub fn enabled_builtin(toggles: &ToolToggles) -> Vec<ToolSpec> {
    builtin_tools()
        .into_iter()
        .filter(|tool| tool_enabled(toggles, &tool.name))
        .collect()
}

pub fn tool_enabled(toggles: &ToolToggles, name: &str) -> bool {
    match name {
        "web_search" => toggles.web_search,
        "web_fetch" => toggles.web_fetch,
        "files" => toggles.files,
        "shell" => toggles.shell,
        "screenshot" => toggles.screenshot,
        "mcp" => toggles.mcp,
        "get_app_state" | "list_settings" | "list_models" | "list_threads"
        | "get_server_status" | "update_settings" | "open_tab" => toggles.app,
        other => toggles.mcp && other.starts_with("mcp_"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    None,
    /// The user must accept this exact call before it runs.
    /// Reserved for shell now, and for later app-setting changes and desktop control.
    EachCall,
}

pub fn approval_for(name: &str) -> Approval {
    match name {
        "shell" | "update_settings" | "desktop_click" | "desktop_type" | "desktop_key" => {
            Approval::EachCall
        }
        _ => Approval::None,
    }
}

pub fn needs_command_approval(name: &str) -> bool {
    approval_for(name) == Approval::EachCall
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

pub fn openai_tools(tools: &[ToolSpec]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect(),
    )
}

pub fn anthropic_tools(tools: &[ToolSpec]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters,
                })
            })
            .collect(),
    )
}

pub fn gemini_tools(tools: &[ToolSpec]) -> Value {
    json!([{
        "functionDeclarations": tools.iter().map(|tool| json!({
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        })).collect::<Vec<_>>()
    }])
}

/// Brave Search is the web-search provider: a free monthly quota with the user's own key.
pub fn brave_search_url(query: &str) -> String {
    format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count=5",
        url_encode(query)
    )
}

pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut chars = html.chars();
    let mut skip = String::new();
    while let Some(ch) = chars.next() {
        if ch != '<' {
            if skip.is_empty() {
                out.push(ch);
            }
            continue;
        }
        let mut tag = String::new();
        for next in chars.by_ref() {
            if next == '>' {
                break;
            }
            tag.push(next);
        }
        let closing = tag.trim_start().starts_with('/');
        let name = tag
            .trim()
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if name == "script" || name == "style" {
            if closing && skip == name {
                skip.clear();
            } else if !closing {
                skip = name;
            }
        } else if skip.is_empty() && !out.ends_with(' ') && !out.is_empty() {
            out.push(' ');
        }
    }
    collapse_ws(&out)
}

pub fn public_http_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    let Some((scheme, rest)) = raw.split_once("://") else {
        return Err("Only http and https URLs can be fetched.".into());
    };
    if !matches!(scheme, "http" | "https") {
        return Err("Only http and https URLs can be fetched.".into());
    }
    if rest.contains('@') {
        return Err("That URL includes a username, which is refused.".into());
    }
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(host) = hostport.strip_prefix('[') {
        host.split(']').next().unwrap_or("")
    } else {
        hostport.split(':').next().unwrap_or("")
    };
    if host.is_empty() || is_blocked_host(host) {
        return Err("That address is private or local, so it was not fetched.".into());
    }
    Ok(raw.to_string())
}

fn is_blocked_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    if matches!(
        host.as_str(),
        "localhost" | "0.0.0.0" | "::1" | "metadata.google.internal"
    ) || host.ends_with(".local")
        || host.ends_with(".localhost")
    {
        return true;
    }
    if let Some(ip) = parse_ipv4(&host) {
        let [a, b, _, _] = ip;
        return a == 10
            || a == 127
            || (a == 172 && (16..=31).contains(&b))
            || (a == 192 && b == 168)
            || (a == 169 && b == 254)
            || a == 0;
    }
    false
}

fn parse_ipv4(host: &str) -> Option<[u8; 4]> {
    let mut parts = [0u8; 4];
    let mut split = host.split('.');
    for part in &mut parts {
        let text = split.next()?;
        if text.is_empty() || text.len() > 3 {
            return None;
        }
        *part = text.parse().ok()?;
    }
    if split.next().is_some() {
        return None;
    }
    Some(parts)
}

/// True when `candidate` is `root` or a path inside it.
/// Drive roots such as `C:\` contain everything on that drive. Comparison ignores
/// case, slash direction, a `\\?\` prefix, and a trailing separator. The paths do
/// not have to exist on disk.
pub fn path_is_inside(root: &std::path::Path, candidate: &std::path::Path) -> bool {
    paths_contain(&root.to_string_lossy(), &candidate.to_string_lossy())
}

pub fn paths_contain(root: &str, candidate: &str) -> bool {
    let root = normalize_path_key(root);
    let candidate = normalize_path_key(candidate);
    if root.is_empty() || candidate.is_empty() {
        return false;
    }
    if is_drive_root(&root) {
        let drive = &root[..2];
        return candidate.len() >= 2 && candidate[..2].eq_ignore_ascii_case(drive);
    }
    candidate == root || candidate.starts_with(&(root + "\\"))
}

fn normalize_path_key(raw: &str) -> String {
    let mut text = raw.trim().replace('/', "\\");
    if let Some(rest) = text.strip_prefix("\\\\?\\") {
        text = rest.to_string();
    }
    while text.len() > 3 && text.ends_with('\\') {
        text.pop();
    }
    if text.len() == 2 && text.as_bytes().get(1) == Some(&b':') {
        text.push('\\');
    }
    text.to_ascii_lowercase()
}

fn is_drive_root(normalized: &str) -> bool {
    normalized.len() == 3
        && normalized.as_bytes()[1] == b':'
        && normalized.as_bytes()[2] == b'\\'
        && normalized.as_bytes()[0].is_ascii_alphabetic()
}

pub fn tool_guidance(approved_folders: &[String], include_app: bool) -> String {
    let folders = if approved_folders.is_empty() {
        "none approved yet".to_string()
    } else {
        approved_folders.join(", ")
    };
    let mut text = format!(
        "Tools are available. The files tool can only read these approved folders: {folders}. Do not invent paths outside them. Keep listings short."
    );
    if include_app {
        text.push_str(" You can inspect this app with get_app_state, list_settings, list_models, list_threads, and get_server_status.");
    }
    text
}

pub fn mcp_tool_name(server: &str, tool: &str) -> String {
    let mut name = format!("mcp_{}_{}", sanitize_token(server), sanitize_token(tool));
    if name.len() > 64 {
        name.truncate(64);
    }
    name
}

fn sanitize_token(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

pub fn encode_mcp_message(payload: &Value) -> String {
    let body = payload.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

pub fn mcp_initialize(id: i64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "Central LLM Interface App", "version": "0.1.1"}
        }
    })
}

pub fn mcp_initialized_notification() -> Value {
    json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
}

pub fn mcp_tools_list(id: i64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/list"})
}

pub fn mcp_tools_call(id: i64, name: &str, arguments: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
}

#[derive(Debug, Default)]
pub struct McpBuffer {
    pending: Vec<u8>,
}

impl McpBuffer {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut messages = Vec::new();
        loop {
            if let Some(message) = self.next_framed() {
                messages.push(message);
                continue;
            }
            if let Some(message) = self.next_line() {
                messages.push(message);
                continue;
            }
            break;
        }
        messages
    }

    fn next_framed(&mut self) -> Option<String> {
        let header_end = self
            .pending
            .windows(4)
            .position(|window| window == b"\r\n\r\n")?;
        let header = String::from_utf8_lossy(&self.pending[..header_end]).to_string();
        let length = header.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })?;
        let start = header_end + 4;
        if self.pending.len() < start + length {
            return None;
        }
        let body = String::from_utf8_lossy(&self.pending[start..start + length]).into_owned();
        self.pending.drain(..start + length);
        Some(body)
    }

    fn next_line(&mut self) -> Option<String> {
        if self
            .pending
            .windows(14)
            .any(|window| window == b"Content-Length")
        {
            return None;
        }
        let end = self.pending.iter().position(|byte| *byte == b'\n')?;
        let line = String::from_utf8_lossy(&self.pending[..end])
            .trim()
            .to_string();
        self.pending.drain(..=end);
        if line.starts_with('{') {
            Some(line)
        } else {
            None
        }
    }
}

pub fn parse_mcp_tool_list(message: &Value) -> Vec<ToolSpec> {
    let Some(tools) = message
        .pointer("/result/tools")
        .and_then(|item| item.as_array())
    else {
        return Vec::new();
    };
    tools
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?.to_string();
            let description = tool
                .get("description")
                .and_then(|item| item.as_str())
                .unwrap_or("MCP tool")
                .to_string();
            let parameters = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| object_schema(json!({}), &[]));
            Some(ToolSpec {
                title: name.clone(),
                name,
                description,
                parameters,
                risky: false,
            })
        })
        .collect()
}

fn url_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risky_tools_are_off_unless_toggled() {
        let toggles = ToolToggles::default();
        let defaults: Vec<_> = enabled_builtin(&toggles)
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert!(defaults.contains(&"get_app_state".to_string()));
        assert!(!defaults.iter().any(|name| name == "shell" || name == "files"));
        let mut on = toggles;
        on.app = false;
        on.shell = true;
        on.web_search = true;
        let names: Vec<_> = enabled_builtin(&on)
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(names, vec!["web_search".to_string(), "shell".to_string()]);
        assert!(needs_command_approval("shell"));
        assert!(!needs_command_approval("files"));
    }

    #[test]
    fn openai_tool_list_uses_function_shape() {
        let tools = enabled_builtin(&ToolToggles {
            web_fetch: true,
            ..ToolToggles::default()
        });
        let body = openai_tools(&tools);
        assert_eq!(body[0]["type"], "function");
        assert_eq!(body[0]["function"]["name"], "web_fetch");
    }

    #[test]
    fn fetch_refuses_private_urls() {
        assert!(public_http_url("https://example.com/docs").is_ok());
        assert!(public_http_url("http://127.0.0.1/admin").is_err());
        assert!(public_http_url("http://192.168.1.5/").is_err());
        assert!(public_http_url("file:///c/windows").is_err());
    }

    #[test]
    fn html_becomes_text() {
        let text = html_to_text("<p>Hello <b>world</b></p><script>alert(1)</script>");
        assert!(text.contains("Hello world"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn drive_root_contains_paths_under_it() {
        assert!(paths_contain(r"C:\", r"C:\Users\Public\ComfyUI"));
        assert!(paths_contain("c:", r"C:\Users\Public\ComfyUI"));
        assert!(paths_contain(r"C:\", r"c:/users/public/comfyui/"));
        assert!(paths_contain(r"D:\", r"D:\"));
        assert!(!paths_contain(r"D:\", r"C:\Users\Public\ComfyUI"));
        assert!(!paths_contain(r"C:\Users", r"C:\UsersExtra\notes"));
        assert!(paths_contain(r"C:\Users\", r"C:\Users\Public"));
        assert!(paths_contain(
            r"\\?\C:\Users\Public",
            r"C:/Users/Public/ComfyUI"
        ));
    }

    #[test]
    fn tool_guidance_names_approved_folders() {
        let text = tool_guidance(&[r"C:\".into(), r"D:\".into()], true);
        assert!(text.contains(r"C:\"));
        assert!(text.contains(r"D:\"));
        assert!(text.contains("get_app_state"));
        assert!(tool_guidance(&[], false).contains("none approved"));
        assert!(!tool_guidance(&[], false).contains("get_app_state"));
    }

    #[test]
    fn mcp_frame_roundtrip() {
        let payload = mcp_tools_list(2);
        let frame = encode_mcp_message(&payload);
        let mut buffer = McpBuffer::default();
        let messages = buffer.push(frame.as_bytes());
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("tools/list"));
    }
}
