use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use central_llm_core::{
    brave_search_url, html_to_text, mcp_initialize, mcp_initialized_notification, mcp_tool_name,
    mcp_tools_call, mcp_tools_list, parse_mcp_tool_list, path_is_inside, public_http_url,
    AppSettings, McpBuffer, McpServerConfig, ToolSpec, BRAVE_SEARCH_ACCOUNT,
};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::keychain::KeychainStore;
use crate::AppState;

pub fn collect_tools(
    toggles: &central_llm_core::ToolToggles,
    settings: &AppSettings,
) -> Vec<ToolSpec> {
    let mut tools = central_llm_core::enabled_builtin(toggles);
    if toggles.mcp {
        for server in settings.mcp_servers.iter().filter(|server| server.enabled) {
            if let Ok(found) = mcp_list(server) {
                for mut tool in found {
                    let remote_name = tool.name.clone();
                    tool.name = mcp_tool_name(&server.name, &remote_name);
                    tool.title = format!("{} · {}", server.name, remote_name);
                    tool.description = format!("{} (from {})", tool.description, server.name);
                    tools.push(tool);
                }
            }
        }
    }
    tools
}

pub async fn run_tool(
    app: &AppHandle,
    name: &str,
    arguments: &str,
    settings: &AppSettings,
) -> String {
    let args: Value = serde_json::from_str(arguments).unwrap_or(json!({}));
    if let Some(rest) = name.strip_prefix("mcp_") {
        return call_named_mcp(settings, rest, &args);
    }
    match name {
        "web_search" => {
            web_search(
                app,
                args.get("query")
                    .and_then(|item| item.as_str())
                    .unwrap_or(""),
            )
            .await
        }
        "web_fetch" => {
            web_fetch(
                app,
                args.get("url").and_then(|item| item.as_str()).unwrap_or(""),
            )
            .await
        }
        "files" => files(
            settings,
            args.get("action")
                .and_then(|item| item.as_str())
                .unwrap_or(""),
            args.get("path")
                .and_then(|item| item.as_str())
                .unwrap_or(""),
        ),
        "shell" => shell(
            args.get("command")
                .and_then(|item| item.as_str())
                .unwrap_or(""),
        ),
        "screenshot" => screenshot(&app.state::<AppState>().data_dir),
        _ => format!("Unknown tool {name}."),
    }
}

async fn web_search(app: &AppHandle, query: &str) -> String {
    if query.trim().is_empty() {
        return "Say what to search for.".into();
    }
    let key = KeychainStore
        .get_account(BRAVE_SEARCH_ACCOUNT)
        .ok()
        .flatten()
        .unwrap_or_default();
    if key.is_empty() {
        return "Add a Brave Search key in Settings before web search can run.".into();
    }
    let state = app.state::<AppState>();
    let response = state
        .client
        .get(brave_search_url(query))
        .header("X-Subscription-Token", key)
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => {
            let body = response.text().await.unwrap_or_default();
            summarize_brave(&body)
        }
        Ok(response) => format!("Web search returned HTTP {}.", response.status()),
        Err(error) => format!("Web search could not connect. {error}"),
    }
}

fn summarize_brave(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return "Web search returned something that was not JSON.".into();
    };
    let Some(results) = value
        .pointer("/web/results")
        .and_then(|item| item.as_array())
    else {
        return "Web search returned no results.".into();
    };
    if results.is_empty() {
        return "Web search returned no results.".into();
    }
    results
        .iter()
        .take(5)
        .map(|item| {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Result");
            let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let snippet = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("{title}\n{url}\n{snippet}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

async fn web_fetch(app: &AppHandle, url: &str) -> String {
    let url = match public_http_url(url) {
        Ok(url) => url,
        Err(error) => return error,
    };
    let state = app.state::<AppState>();
    match state.client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            let body = response.text().await.unwrap_or_default();
            let text = html_to_text(&body);
            truncate(&text, 12_000)
        }
        Ok(response) => format!("Fetch returned HTTP {}.", response.status()),
        Err(error) => format!("Could not fetch that page. {error}"),
    }
}

fn files(settings: &AppSettings, action: &str, path: &str) -> String {
    if settings.approved_folders.is_empty() {
        return "Files is on, but no folder is approved in Settings.".into();
    }
    let candidate = Path::new(path);
    let allowed = settings
        .approved_folders
        .iter()
        .any(|root| path_is_inside(Path::new(root), candidate));
    if !allowed {
        return "That path is outside the folders approved in Settings.".into();
    }
    match action {
        "list" => match std::fs::read_dir(candidate) {
            Ok(entries) => {
                let mut names = entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect::<Vec<_>>();
                names.sort();
                let truncated = names.len() > 60;
                names.truncate(60);
                if names.is_empty() {
                    "That folder is empty.".into()
                } else {
                    let mut text = names.join("\n");
                    if truncated {
                        text.push_str("\n…listing truncated");
                    }
                    truncate(&text, 4_000)
                }
            }
            Err(error) => format!("Could not list that folder. {error}"),
        },
        "read" => match std::fs::read(candidate) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                truncate(&text, 8_000)
            }
            Err(error) => format!("Could not read that file. {error}"),
        },
        _ => "Use action list or read.".into(),
    }
}

fn shell(command: &str) -> String {
    if command.trim().is_empty() {
        return "There was no command to run.".into();
    }
    let mut process = if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd
    };
    hide_console(&mut process);
    let mut child = match process
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return format!("Could not run that command. {error}"),
    };
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let mut err = String::new();
        if let Some(pipe) = stdout.as_mut() {
            let _ = pipe.read_to_string(&mut out);
        }
        if let Some(pipe) = stderr.as_mut() {
            let _ = pipe.read_to_string(&mut err);
        }
        (out, err)
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > Duration::from_secs(30) => {
                let _ = child.kill();
                let _ = child.wait();
                return "The command did not finish within 30 seconds.".into();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => return format!("Could not wait for that command. {error}"),
        }
    };
    let (out, err) = reader.join().unwrap_or_default();
    let mut text = out;
    if !err.trim().is_empty() {
        text.push('\n');
        text.push_str(&err);
    }
    text.push_str(&format!("\nexit {}", status.code().unwrap_or(-1)));
    truncate(&text, 12_000)
}

fn screenshot(data_dir: &Path) -> String {
    #[cfg(windows)]
    {
        let dest = data_dir.join("attachments");
        let _ = std::fs::create_dir_all(&dest);
        let file = dest.join(format!("screenshot-{}.bmp", std::process::id()));
        match capture_screen(&file) {
            Ok(()) => format!("Saved a screenshot at {}", file.display()),
            Err(error) => format!("Screenshot failed. {error}"),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = data_dir;
        "Screenshot runs on Windows.".into()
    }
}

#[cfg(windows)]
fn capture_screen(path: &Path) -> Result<(), String> {
    use std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void;
        fn DeleteDC(hdc: *mut c_void) -> i32;
        fn CreateCompatibleBitmap(hdc: *mut c_void, width: i32, height: i32) -> *mut c_void;
        fn SelectObject(hdc: *mut c_void, object: *mut c_void) -> *mut c_void;
        fn DeleteObject(object: *mut c_void) -> i32;
        fn BitBlt(
            dest: *mut c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            src: *mut c_void,
            src_x: i32,
            src_y: i32,
            rop: u32,
        ) -> i32;
        fn GetDIBits(
            hdc: *mut c_void,
            bitmap: *mut c_void,
            start: u32,
            lines: u32,
            bits: *mut c_void,
            info: *mut BitmapInfo,
            usage: u32,
        ) -> i32;
    }
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_pels: i32,
        y_pels: i32,
        clr_used: u32,
        clr_important: u32,
    }
    #[repr(C)]
    struct BitmapInfo {
        header: BitmapInfoHeader,
    }
    unsafe {
        let width = GetSystemMetrics(0);
        let height = GetSystemMetrics(1);
        if width <= 0 || height <= 0 {
            return Err("No screen was found.".into());
        }
        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return Err("Could not read the screen.".into());
        }
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if memory.is_null() || bitmap.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen);
            return Err("Could not prepare the capture.".into());
        }
        let previous = SelectObject(memory, bitmap);
        let copied = BitBlt(memory, 0, 0, width, height, screen, 0, 0, 0x00CC_0020);
        let stride = (width as usize) * 4;
        let mut pixels = vec![0u8; stride * height as usize];
        let mut info = BitmapInfo {
            header: BitmapInfoHeader {
                size: 40,
                width,
                height,
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: pixels.len() as u32,
                x_pels: 0,
                y_pels: 0,
                clr_used: 0,
                clr_important: 0,
            },
        };
        let rows = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr() as *mut c_void,
            &mut info,
            0,
        );
        SelectObject(memory, previous);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(std::ptr::null_mut(), screen);
        if copied == 0 || rows == 0 {
            return Err("Could not copy the screen.".into());
        }
        write_bmp(path, width, height, &pixels)
    }
}

#[cfg(windows)]
fn write_bmp(path: &Path, width: i32, height: i32, pixels: &[u8]) -> Result<(), String> {
    let image_size = pixels.len() as u32;
    let offset = 14 + 40;
    let file_size = offset + image_size;
    let mut bytes = Vec::with_capacity(file_size as usize);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&file_size.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&offset.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&image_size.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 16]);
    bytes.extend_from_slice(pixels);
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

fn call_named_mcp(settings: &AppSettings, prefixed: &str, args: &Value) -> String {
    let wanted = format!("mcp_{prefixed}");
    for server in settings.mcp_servers.iter().filter(|server| server.enabled) {
        let Ok(tools) = mcp_list(server) else {
            continue;
        };
        for tool in tools {
            if mcp_tool_name(&server.name, &tool.name) == wanted {
                return mcp_call(&server, &tool.name, args).unwrap_or_else(|error| error);
            }
        }
    }
    format!("No connected MCP tool matches {prefixed}.")
}

fn mcp_list(server: &McpServerConfig) -> Result<Vec<ToolSpec>, String> {
    let messages = mcp_exchange(
        server,
        &[
            mcp_initialize(1),
            mcp_initialized_notification(),
            mcp_tools_list(2),
        ],
    )?;
    Ok(messages
        .iter()
        .find_map(|message| {
            let tools = parse_mcp_tool_list(message);
            if tools.is_empty() {
                None
            } else {
                Some(tools)
            }
        })
        .unwrap_or_default())
}

fn mcp_call(server: &McpServerConfig, name: &str, args: &Value) -> Result<String, String> {
    let messages = mcp_exchange(
        server,
        &[
            mcp_initialize(1),
            mcp_initialized_notification(),
            mcp_tools_call(3, name, args),
        ],
    )?;
    let Some(message) = messages
        .iter()
        .find(|message| message.get("id").and_then(|id| id.as_i64()) == Some(3))
    else {
        return Err("The MCP server did not answer the tool call.".into());
    };
    if let Some(error) = message
        .pointer("/error/message")
        .and_then(|item| item.as_str())
    {
        return Err(error.to_string());
    }
    Ok(message
        .pointer("/result/content")
        .map(|item| item.to_string())
        .unwrap_or_else(|| message.to_string()))
}

fn mcp_exchange(server: &McpServerConfig, outbound: &[Value]) -> Result<Vec<Value>, String> {
    let mut command = Command::new(&server.command);
    command
        .args(&server.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start {}. {error}", server.name))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("{} has no input.", server.name))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{} has no output.", server.name))?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = McpBuffer::default();
        let mut chunk = [0u8; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    for message in buffer.push(&chunk[..size]) {
                        let _ = tx.send(message);
                    }
                }
            }
        }
    });
    for payload in outbound {
        let frame = central_llm_core::encode_mcp_message(payload);
        stdin
            .write_all(frame.as_bytes())
            .map_err(|error| format!("Could not talk to {}. {error}", server.name))?;
    }
    let _ = stdin.flush();
    let mut replies = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    replies.push(value);
                }
                if replies
                    .iter()
                    .any(|message| message.get("id").and_then(|id| id.as_i64()) == Some(2))
                    || replies
                        .iter()
                        .any(|message| message.get("id").and_then(|id| id.as_i64()) == Some(3))
                {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    if replies.is_empty() {
        Err(format!("{} did not answer.", server.name))
    } else {
        Ok(replies)
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        text.to_string()
    } else {
        let clipped: String = text.chars().take(max_chars).collect();
        format!("{clipped}\n…")
    }
}
