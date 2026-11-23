# Adding a tool

Tools live in one registry so a later tool does not need a new menu or a new permission system.

## Where the pieces are

| Piece | File | What it owns |
| --- | --- | --- |
| Spec | `crates/core/src/tools.rs` | Name, description, JSON schema, and whether each call needs approval |
| Toggle | `ToolToggles` in `crates/core/src/types.rs` and the Tools menu | Whether this chat may offer the tool |
| Handler | `run_tool` in `src-tauri/src/tool_runtime.rs` | The actual work: network, disk, or a process |
| Request | `build_body` in `crates/core/src/providers.rs` | OpenAI, Anthropic, and Gemini tool shapes |

The chat loop already streams tool calls, runs the handler, and shows each step in the thread. A new tool joins that loop when its spec is enabled.

## Steps

1. Add a `ToolSpec` in `builtin_tools`. The name is the function name sent to the model. Use letters, digits, and underscores.
2. If the user must accept every call, add the name to `approval_for`. Shell is the pattern. The prompt is the same one the composer already shows.
3. Add a boolean to `ToolToggles`, with `#[serde(default)]`, and a row in `TOOL_TOGGLES` in `src/types.ts`.
4. Handle the name in `run_tool`. Return plain text. The loop stores it on the assistant message and sends it back to the model.
5. If the tool needs a secret, store it through the keychain with `require_account`, not in settings JSON.

Leave the tool off by default when it can read private files or run a command.

## Reserved names

These names are already in `approval_for` and are not built yet. Use them when their milestone arrives so the approval behavior stays the same.

| Name | Milestone | Behavior |
| --- | --- | --- |
| `update_settings` | App awareness, soon after v0.1.1 | One approval for each settings change. Read-only tools for settings, models, workflows, and chats do not use this name and do not ask. |
| `desktop_click`, `desktop_type`, `desktop_key` | Desktop control, near the end | One approval for each pointer or keyboard action. Screenshot stays the separate look-around tool from v0.1.1. |

## MCP

An MCP server configured in Settings is not a new built-in spec. When the MCP toggle is on, the app starts the server, lists its tools, and prefixes each name with `mcp_`. Those names go through the same request and the same step list. A server tool does not get an approval prompt unless its prefixed name is added to `approval_for`.
