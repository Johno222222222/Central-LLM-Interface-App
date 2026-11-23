# Central LLM Interface App

A personal Windows desktop app for local and hosted chat. v0.1 is the app shell, Chat, provider keys, and a Windows installer. Create, the model browser, and Benchmark come later. The full plan is in [docs/SPEC.md](docs/SPEC.md).

## Install on Windows

1. Open the latest successful run of the **CI** workflow on GitHub.
2. Download the **windows-installer** artifact.
3. Run the setup file (`Central LLM Interface App_0.1.0_x64-setup.exe`). It installs for the current user, adds a Start menu shortcut, and includes an uninstaller.
4. Start **Central LLM Interface App** from the Start menu.

You do not need Rust, Node, or a separate llama.cpp install. The setup includes a CUDA llama-server build (CUDA 13.5) so an RTX 5070 can run GGUF models. That binary comes from [llama.cpp](https://github.com/ggml-org/llama.cpp) (MIT). The pinned build is `b12651`.

WebView2 is installed by the setup if Windows does not already have it.

## First chat

1. Put `.gguf` chat models in the models folder. Settings shows the path and can open it. That folder belongs to this app. Leave the ComfyUI models folder alone.
2. Open Chat, pick the model, and send a message. The app starts llama-server for you.
3. For OpenAI, Anthropic, Gemini, DeepSeek, or OpenRouter, open Settings, paste a key, and choose **Test connection**. Keys are saved in Windows Credential Manager. They are not written to a file, and the app does not show a saved key again.

If you already run llama-server yourself, paste its URL under **Local runtime** and choose **Save and restart**. Leave the URL empty to use the bundled server.

**Unload all models** in the model picker frees the model the local server has loaded.

## What v0.1 includes

- Top menu: Chat, Create, Benchmark, Models, Settings. Create, Benchmark, and Models are placeholders. There is no Compare section and no Voice tab.
- Chat: conversation list, streaming replies, markdown and code, context meter, sampling (temperature, top P, max tokens, think budget, custom parameters), Think and reasoning effort, image attach, and a model picker grouped by provider and family.
- Code and Voice are on the composer and stay off until a later version.
- Chats are stored on this computer and are still there after a restart.
- Appearance is dark by default. Light and system are in Settings.
- Tools, from the composer: web search (Brave, your own key), web fetch, files in folders you approve, shell with a yes on each command, screenshot, and MCP servers from Settings. Shell and Files start off.
- Extra GGUF folders in Settings, scanned in place. Import from Locally Uncensored fills in that app's model folder when it can find it.

## What v0.1.2 adds

- Local context follows the card and the model. A 9B Q4 on 12 GB is about 32K. Each model has a slider in Settings and next to the token pill. The pill shows that window, and changing it restarts the bundled server.
- Screenshot, shell, and MCP do not flash a console window. A whole drive approved for Files covers every folder on it.
- A matching mmproj GGUF beside a vision model is picked up automatically. A picture sent to a text-only model gets a clear message.
- An empty account (HTTP 402) says which provider is out of credit.
- Hover a message for Copy. User messages can be edited and resent. Assistant messages can be retried, branched, or marked up or down. Export feedback is in Settings.

The next milestone is still app-awareness tools, then v0.2 Create with ComfyUI.

## Run from source

On Windows, install Node.js 22, the Rust stable toolchain, and the Visual Studio C++ build tools. Then:

```powershell
npm install
npm run tauri dev
```

`tauri dev` uses a llama-server only if one is already bundled or you set a custom URL in Settings. To bundle the CUDA server the same way CI does:

```powershell
./scripts/fetch-llama-server.ps1
npm run tauri build
```

The setup file is written under `src-tauri/target/release/bundle/nsis/`.

Checks:

```powershell
npm run lint
npm run typecheck
npm test
cargo test --workspace
```

CI on GitHub runs those checks and builds the Windows installer. It does not build macOS or Linux.

## Where things live

| | |
| --- | --- |
| Chats and settings | The app data folder, in `studio.db` |
| API keys | Windows Credential Manager, service name `Central LLM Interface App` |
| GGUF models | The models folder shown in Settings |
| llama-server log | `llama-server.log` in the app data folder |

## Not in this version

Create and ComfyUI, the Hugging Face model browser, Benchmark, Voice, Code mode, chat plugins, and Mac or Linux builds. Those are later milestones in the spec. App awareness (the model reading and, with a yes each time, changing this app's settings) comes soon after the tools menu. Looking around the desktop and then controlling the mouse and keyboard, still with a yes for each action, is one of the last steps.
