# Central LLM Interface App

Product and technical spec. This is the source of truth for the app. Decisions at the end were answered on Nov 23, 2026, in the America/Detroit time zone. Items 2 and 3 were left to the build and can be changed later.

The app is a personal Windows desktop AI studio for Johno. It keeps the chat and models experience he already likes from Locally Uncensored, and adds Create screens that can drive any ComfyUI workflow, including a real image slot for image-to-3D. It is built in small steps for an RTX 5070 with 12 GB of VRAM.

## Why this app

Locally Uncensored cannot feed a new picture into a custom image-to-3D workflow from its Image tab. On import, this app should parse the ComfyUI graph, build real input controls, and file the workflow under Create by its output type. A mesh or `.glb` workflow lands under 3D, with an image slot if it loads an image.

Johno likes about 90 percent of the Locally Uncensored interface, especially the chat screen, and already uses ComfyUI Desktop for image, video, music, and 3D workflows. He does not want to wait on Locally Uncensored updates to get full workflow support. This is a personal Windows desktop app, built in a public GitHub repo by Cursor cloud agents, in small steps.

| | |
| --- | --- |
| Name | Central LLM Interface App |
| Repo | Public GitHub repo Central-LLM-Interface-App |
| Hardware | RTX 5070, 12 GB VRAM, Windows |
| ComfyUI | Existing Desktop install at `C:\Users\borna\ComfyUI\Installs\ComfyUI\ComfyUI` |
| Reference app | UE5 Systems Trainer, public repo Johno222222222/UE5-Systems-Trainer |
| Direction | Player and user perspective, not micro hand-holding. Johno is new to C++ and UE5 and learns fast. This app is not a C++ project, but the same tone applies. |

## Goals and non-goals

Goals:

- One app for local and hosted chat, plus image, video, music, and 3D.
- Clone the Locally Uncensored chat screen closely, with Code and Voice as toggles inside chat, not separate screens.
- Import any ComfyUI workflow and auto-generate real inputs from the graph.
- Classify workflows by output, so a 3D graph shows up under 3D, not Image.
- Local GGUF chat from Hugging Face, plus bring-your-own keys and OpenRouter.
- Store API keys in the OS keychain, not in a plaintext file.
- Connect to the ComfyUI install he already has. Do not replace it.
- Ship in small milestones he can pause between.

Non-goals:

- Cloning every Locally Uncensored feature on day one, including Compare, Benchmark, and the full plugin and tools set.
- A separate Voice tab or a separate coding screen.
- Replacing or forking ComfyUI Desktop, or offering a git-pull update button for it.
- Paid cloud generators such as Higgsfield in the first versions.
- A public model marketplace, accounts, or telemetry.
- Inventing new image or 3D models. The app wraps ComfyUI and Hugging Face.
- Chat plugins in v0.1.
- Shipping macOS or Linux until one of the very last steps. Windows only until then.

## Stack

Tauri 2 with a React and TypeScript UI. A native shell plus a web UI stays familiar, and Tauri keeps the installer and RAM use smaller than Electron, which matters when llama.cpp and ComfyUI already share a 12 GB card and system memory. The Rust side owns the llama.cpp connection, filesystem access, the OS keychain, and the ComfyUI HTTP and websocket client.

| | |
| --- | --- |
| Shell | Tauri 2 |
| UI | React and TypeScript |
| Local chat | Bundled llama.cpp llama-server (CUDA), auto-started, speaking its OpenAI-compatible API. A custom server URL in Settings is optional. |
| Hosted chat | OpenAI, Anthropic, Gemini, DeepSeek, and OpenRouter as a catch-all |
| Creative backend | The existing ComfyUI Desktop install, by host and port |
| 3D preview | three.js, for `.glb` and `.gltf` |
| Model search | Hugging Face, filtered to GGUF and fit |
| Secrets | OS keychain, never plaintext on disk |
| Persistence | SQLite for chats, settings refs, tags, and model records. Original workflow JSON kept on disk |
| Chat GGUFs | An app-managed folder, not the ComfyUI models folder |
| Installer | A simple Windows installer with a Start menu shortcut and an uninstaller |

llama.cpp is the default local runtime because GGUF files from Hugging Face are what Johno already uses, sampling and vision `mmproj` files are straightforward, and it does not add a second model manager. Ollama can be an optional backend later. If sampling is sent to Ollama, custom parameters go inside `options`. For OpenAI-compatible endpoints they go top-level.

## Architecture

The UI never talks to providers or ComfyUI on its own. It calls Tauri commands. The core routes chat to llama.cpp or a hosted API, stores threads locally, reads and writes keys only through the keychain, searches Hugging Face for GGUF files, and drives ComfyUI with its documented HTTP and websocket API. Output files stay in the ComfyUI output folder, and the app previews them in a gallery or the 3D viewer.

```text
App UI (React and TypeScript)
        | commands
        v
Tauri core (Rust)
        |-- Local runtime (llama.cpp, keychain, SQLite)
        |-- Hosted services (cloud APIs and Hugging Face)
        +-- ComfyUI Desktop (HTTP and websocket)
```

## Screens

The top menu is Chat, Create, Benchmark, Models, and Settings. Benchmark stays visible but is not a working suite until a later milestone. There is no Compare section, and no separate Voice tab. Appearance should feel close to Locally Uncensored, especially in chat. Do not redesign the composer for sport.

### Chat

Clone the Locally Uncensored chat screen closely: a conversation column, a model picker grouped by family, and a composer bar at the bottom. Code (coding mode) and Voice sit on that composer as toggles, instead of a different screen inside chat or a top-menu option.

Chat models come from three sources, shown in one picker: local GGUF files run by llama.cpp; bring-your-own keys for OpenAI, Anthropic, Gemini, and DeepSeek; and OpenRouter as a catch-all for other hosted models. A conversation can switch models between turns.

| Composer feature | When |
| --- | --- |
| Agent mode toggle | Later, with tools |
| Sandbox toggle | Later, with tools |
| Context meter, e.g. 118.8K/256K | v0.1 |
| Attach files or images | v0.1 for images; docs later |
| Mic | Later, with Voice |
| Think toggle | Later |
| Reasoning effort: Default, Low, Medium, High | Later |
| Docs side panel for PDF, DOCX, and TXT | Later |
| Plugins: reverse engineering, Neurons steering, Caveman, Persona, group chat | Later |
| Tools toggles: Web, System, Files, Shell, Screenshot, Image, Video, Workflows, MCP | Later |
| Sampling popover: temperature, top P, max tokens, plus room for think budget and custom params | v0.1, without think budget as a required behavior |
| Model picker grouped by family, with Unload all models | v0.1 |
| Phone button | Later |
| Small-Model toggle | Later |
| Code toggle, inside chat | Later |
| Voice toggle, inside chat | Later |

Features marked later may appear in the composer as disabled controls so the layout stays close to Locally Uncensored, but they must not be fake-working. v0.1 needs a real streaming reply, persisted threads, the context meter, the sampling popover, and a picker that can reach local and hosted models.

### Create

Never git-pull a Desktop-managed ComfyUI. Locally Uncensored's updater failed that way on this install. The app connects by host and port, and may show the install path for context. Missing custom nodes can be offered as a git URL install of that node pack. There is no core Update ComfyUI button.

Create has four headers: Image, Video, Music, and 3D. Locally Uncensored-style sub-tools such as Edit, Cutout, Enhance, Animate, Character, and Extend become sub-options under the header they belong to. Each sub-option runs either a built-in recipe (a known-good workflow the app ships later) or any imported ComfyUI workflow classified into that header.

Create itself stays in v0.2. That milestone ships import and auto-generated inputs first. Built-in recipes, including the Generate modes below, follow once import is solid.

| Header | Sub-options |
| --- | --- |
| Image | Generate, Edit, Cutout, Enhance, and Character |
| Video | Generate, with choices for Text to video, Image to video, and Multiview to video; plus Animate and Extend |
| Music | Generate |
| 3D | Generate, with choices for Text to 3D, Image to 3D, and Multiview to 3D |

Every header also has a Workflows list for imported graphs of that output type.

| Output from the graph | Create header |
| --- | --- |
| Mesh, `.glb`, `.gltf`, or similar 3D save nodes | 3D |
| Video save or video combine nodes | Video |
| Audio or music save nodes | Music |
| Image save nodes, or anything unclear | Image |

On import, parse both ComfyUI UI format and API format. Walk the nodes and widgets, then build a real control panel:

| Graph node or widget | Control in the app |
| --- | --- |
| LoadImage | Image slot, with upload and a preview |
| LoadVideo | Video slot |
| LoadAudio | Audio slot |
| Text prompt or a multiline text widget | Text area |
| Seed | Seed field, with a randomize action |
| Sampler name or scheduler | Dropdown from the node's options |
| Numeric widgets such as steps, CFG, or batch size | Number field or slider, with the node's min, max, and default |

A run uses ComfyUI's public API only: upload inputs with `/upload/image` (and the equivalent for video or audio when needed), queue with `/prompt`, follow progress on the websocket, then fetch outputs with `/history` and `/view`. Default batch size to 1, and warn before a setting that is likely to exhaust 12 GB. Image, video, and audio land in a gallery. 3D outputs open in a built-in three.js viewer, and the user can reveal the file in the ComfyUI output folder.

```text
App -> ComfyUI: Upload input
App -> ComfyUI: POST /prompt
ComfyUI -> App: Progress on the websocket
App -> ComfyUI: GET /history and /view
ComfyUI -> App: Image, video, or .glb
```

If the graph uses custom nodes that the connected ComfyUI does not have, show the missing class names and, when a git URL is known or pasted, offer to install that pack. Tags can still group a workflow with the model files it needs, but the header placement comes from output type, not from tags alone.

### Models

This screen has two sides, close to Locally Uncensored's layout: an Installed list, and a Get new browser over Hugging Face. Installed is grouped by Chat, Image, Video, and LoRAs. Get new is aimed at chat GGUF files first, with a quant picker, an optional vision `mmproj` add-on, size buckets, and category filters for Agent and tools, Sees images, Coding, Reasoning, and Small and fast.

Fit badges are computed for this 12 GB card. Use a simple budget of about 10 GB so Windows and the desktop stay comfortable, based on file size plus a buffer for context. Runs on your PC means it should load with useful context. Tight fit means it may load, but context will be short or the model may spill into system RAM. Do not recommend IQ2 quants as the default pick on this card. Prefer Q4_K_M or higher when the model still fits. A smaller model at Q6 is usually the better 12 GB choice than a 24B squeezed to Q2.

### Settings

- Providers and keys: OpenAI, Anthropic, Gemini, DeepSeek, and OpenRouter, each with a test-connection action. Keys go to the OS keychain.
- Local runtime: bundled llama-server is auto-started, with an optional custom URL, the app's models folder, and unload behavior.
- ComfyUI connection: host, port, and optional install path. Default the path to the Desktop install above. Usual port is 8188, but it must be editable. Show connected or not, with a clear error if Desktop is not running. This connection UI is part of v0.2.
- Folders: chat GGUFs live in the app's own folder, not in the ComfyUI folder; plus ComfyUI models and output folders, and the app's own data directory.
- Appearance: system, light, or dark. Dark is the default.

### Benchmark, later

Keep the top-level item in the menu so the layout stays stable, and show a short coming-soon state until this milestone. Do not design the benchmark suite in v0.1.

## Data model

This is the shape to persist, not a wire format that must be copied character for character. Keys are not stored in these records. Only a provider id points at the keychain.

```ts
type CreateKind = "image" | "video" | "music" | "3d";
type Fit = "runs" | "tight" | "no";

interface Workflow {
  id: string;
  name: string;
  sourcePath?: string;
  graph: unknown; // original ComfyUI JSON, UI or API format
  format: "ui" | "api";
  kind: CreateKind; // from output nodes, not from tags alone
  tags: string[];
  inputs: WorkflowInput[];
  outputs: WorkflowOutput[];
  missingNodes: MissingNode[];
  importedAt: string;
}

interface WorkflowInput {
  nodeId: string;
  kind: "image" | "video" | "audio" | "text" | "seed" | "sampler" | "number" | "other";
  label: string;
  widget?: string;
  default?: unknown;
  min?: number;
  max?: number;
}

interface WorkflowOutput {
  nodeId: string;
  kind: CreateKind | "other";
}

interface MissingNode {
  classType: string;
  gitUrl?: string;
}

interface Tag {
  id: string;
  name: string;
}

interface Model {
  id: string;
  name: string;
  family?: string;
  source:
    | "local-gguf"
    | "huggingface"
    | "openai"
    | "anthropic"
    | "gemini"
    | "deepseek"
    | "openrouter"
    | "comfy"
    | "lora";
  repoId?: string;
  path?: string;
  quant?: string;
  sizeBytes?: number;
  vramEstimateMb?: number;
  fit?: Fit;
  modalities: Array<"text" | "vision" | "audio">;
  categories: string[];
  mmprojPath?: string;
  installed: boolean;
}

interface Chat {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  modelId: string;
  messages: ChatMessage[];
  settings: ChatSettings;
  documentIds: string[];
  contextUsed: number;
  contextLimit: number;
}

interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  createdAt: string;
  attachments?: string[];
  thinking?: string;
}

interface ChatSettings {
  temperature: number;
  topP: number;
  maxTokens: number;
  thinkBudget?: number;
  customParams?: Record<string, unknown>;
  agent: boolean;
  sandbox: boolean;
  code: boolean;
  voice: boolean;
  think: boolean;
  effort: "default" | "low" | "medium" | "high";
}

interface AppSettings {
  llamaServerUrl: string;
  comfyHost: string;
  comfyPort: number;
  comfyPath?: string;
  modelsDir: string; // app-managed chat GGUFs, not the ComfyUI folder
  outputDir: string;
  appearance: "system" | "light" | "dark";
}
```

## Build plan

Each slice below is meant to be one cloud-agent run. Stop when that slice's acceptance line is true. Leave later slices alone. Windows is the only target, and CI should build only Windows. Chat plugins are out of scope for v0.1. macOS and Linux wait until one of the very last steps.

| Milestone | Focus | Runs |
| --- | --- | --- |
| v0.1 | App shell, Chat, and a Windows installer | 5 |
| v0.1.1 | Tools menu and extra GGUF folders | 1 |
| v0.1.2 | Context, hidden tools, vision projectors, message actions | 1 |
| v0.2 | Create with ComfyUI import | 4 |
| v0.3 | Models browser | 2 |
| Later | Benchmark, voice, code, tools, then Mac and Linux | As needed |

### v0.1 Chat

**0.1.1 App shell.** New public GitHub repo Central-LLM-Interface-App, Tauri 2 plus React and TypeScript, a desktop window, and top nav for Chat, Create, Benchmark, Models, and Settings. Benchmark is not a working suite. Create, Models, and Settings can be placeholders. Builds and CI are Windows-only: no macOS or Linux jobs. Done when the app launches on Windows, nav switches screens, CI is Windows-only, and there is no Compare or Voice tab.

**0.1.2 Providers and keychain.** Settings fields for OpenAI, Anthropic, Gemini, DeepSeek, and OpenRouter, each with a test-connection action. Done when a key can be saved and read back from the OS keychain, a bad key shows a clear error, and keys are not written to a plaintext config file.

**0.1.3 Local chat.** Bundle a CUDA llama-server and auto-start it. Chat GGUFs live in the app's own models folder, not in the ComfyUI folder. Settings can still point at a custom llama-server URL. Include the context meter, a sampling popover for temperature, top P, and max tokens, a family-grouped picker for installed local models, and Unload all models. Done when a local message streams without Johno installing llama.cpp himself, the thread survives a restart, and unload works.

**0.1.4 Hosted models in the same chat.** Add OpenAI-compatible adapters for OpenAI, DeepSeek, and OpenRouter, plus Anthropic and Gemini adapters. Sampling params go top-level on OpenAI-compatible calls. The picker groups models by family across local and hosted sources. Do not implement chat plugins. Done when the same thread can switch models, a hosted reply streams, and errors are readable.

**0.1.5 Simple Windows installer.** Ship a standard Windows installer (the Tauri bundle) that installs the app, the bundled llama-server, a Start menu shortcut, and an uninstaller. CI publishes that Windows artifact only. Done when Johno can install from one Windows download without building from source or unpacking a zip.

### v0.1.1 Tools and extra model folders

Two additions after the first installer, in one step.

**Tools.** The composer has a Tools menu, modeled on Locally Uncensored: one dropdown, a toggle and a status dot per tool. The first set is Web search, Web fetch, Files, Shell, Screenshot, and MCP.

- Web search uses Brave Search and the user's own key, saved in the OS keychain and set in Settings. The free Brave plan is enough to try it.
- Web fetch reads a public http or https page. Private and local addresses are refused.
- Files lists and reads only inside folders the user approved in Settings. It cannot write. The toggle starts off.
- Shell runs one command and asks for approval of that exact command before it runs. The toggle starts off.
- Screenshot captures the primary screen on Windows.
- MCP starts servers configured in Settings and exposes their tools in the same menu.

Providers that support tools get real tool calls: OpenAI-compatible APIs including OpenRouter and DeepSeek, Anthropic, Gemini, and llama-server when the model supports tools. Each call and its result shows in the thread as a collapsible step.

The tool list is a registry (`crates/core/src/tools.rs`). A new tool is a spec plus one handler in the desktop shell. How to add one is in `docs/TOOLS.md`. The registry already names the approval rule for later tools so they drop in without a new permission system:

- `update_settings` asks for approval of each change.
- `desktop_click`, `desktop_type`, and `desktop_key` ask for approval of each action.

Those names are reserved. They are not implemented in v0.1.1.

**Extra model folders.** Settings can add more GGUF folders. The app scans them next to its own models folder and does not copy the files. An Import from Locally Uncensored action looks for `%APPDATA%\Locally Uncensored\models`, and for a GGUF folder named in `%APPDATA%\locally-uncensored\config.json`. If neither is there, the user browses to the folder. Models from those folders show up in the chat picker and load through the bundled llama-server by absolute path. Moving them into the app folder is a later step, so extra folders stay a separate list from the app's own models folder.

### v0.1.2 Polish

From a real Windows test on an RTX 5070 with 12 GB. This stays inside chat and the local server. App awareness is still the next milestone, then v0.2 Create.

- The bundled llama-server picks a context from VRAM and model size. A 9B Q4_K_M on 12 GB gets about 32K, not the old 8.2K default. Settings and the chat meter have a per-model slider. The choice is saved, the server restarts when it changes, and the token pill shows that window.
- Screenshot captures the screen in the app process. Shell, screenshot, and MCP never open a console window.
- Approving a drive root such as `C:\` covers every path on that drive. The check ignores case, slash direction, and a trailing separator.
- A matching `mmproj*.gguf` next to a vision model, or in another scanned folder, is passed to llama-server. A picture sent to a model that cannot see it gets a plain explanation.
- HTTP 402 and similar credit errors name the provider and where to top up. DeepSeek points at its own site or OpenRouter.
- Tool schemas stay short. Long file listings are cut down. When tools are on, the model is told the exact approved folders.
- User messages can be copied or edited. An edit resends from that point and drops the later turns. Assistant messages can be copied, retried (with another model if wanted), branched into a new chat, or marked up or down with an optional note. Feedback stays on this computer, and Settings can export it.

### Next, after v0.1.2: app awareness

Chat models can read this app's own state and, with a yes on each change, update settings.

Read-only tools cover settings, installed models, imported workflows, and chats. They use the same registry as v0.1.1 and do not need an approval prompt to look. A change, such as turning a tool on, pointing at another models folder, or editing a provider, goes through `update_settings` and waits for the user to approve that specific change. The model sees the refusal if the user says no. Nothing here writes keys, deletes models, or starts a ComfyUI run.

### Near the end: desktop look-around and control

After the creative tools and the model browser exist, the model can look at the desktop and then act on it.

Looking is the screenshot tool from v0.1.1, used on purpose so the model can see the screen before it asks to do something. Control is mouse and keyboard: `desktop_click`, `desktop_type`, and `desktop_key`. Each of those waits for approval of that one action, using the approval hook the registry already has. There is no mode that clicks or types without a yes. This stays one of the last steps, with Benchmark, voice, and the Mac and Linux builds.

### v0.2 Create

**0.2.1 ComfyUI connection.** Settings for host, port, and optional install path, plus a health check. No git-pull or Update ComfyUI action. Done when the app shows connected if Desktop is running, and a clear error if it is not.

**0.2.2 Import and classify.** Import UI-format and API-format workflow JSON, detect missing custom nodes, and classify by output type. Save the workflow and optional tags. Done when an image-to-model graph that saves a mesh or `.glb`, such as the Pixal3D plus TRELLIS.2 workflow, appears under Create, 3D, not under Image.

**0.2.3 Auto inputs and a run.** Generate controls from LoadImage, LoadVideo, LoadAudio, text, seed, sampler, and numeric widgets. Upload inputs, queue the prompt, show progress, and fetch outputs into a gallery. Default batch size is 1, with a warning on values likely to exhaust 12 GB. Done when Johno can pick a new image in the app, without editing the JSON, hit Create, and see the output in the gallery.

**0.2.4 3D viewer.** A three.js viewer for `.glb` and `.gltf` outputs from Create, 3D, plus a reveal-in-folder action. Done when a mesh from a run can be rotated and zoomed in the app.

### v0.3 Models

**0.3.1 Installed list.** List installed chat GGUFs and, when folders are configured, Image, Video, and LoRA files, with size, quant, and source path. Chat GGUFs get a fit badge. Done when an installed local chat model shows up here and in the chat picker.

**0.3.2 Get new, from Hugging Face.** Search GGUF chat models, with size buckets, the category filters listed above, a quant picker, and an optional vision mmproj add-on. Show Runs on your PC or Tight fit for 12 GB. Do not default to IQ2. Done when Johno can search, see a fit badge, download a small GGUF into the app's models folder (not the ComfyUI folder), and pick it in chat.

### Later

- Benchmark.
- Voice as an in-chat toggle, including the mic.
- Code as an in-chat toggle, with file-aware coding help.
- Agent tools: Web, Files, Shell, Screenshot, and the rest of the Locally Uncensored tools row, with clear permission prompts.
- Docs side panel for PDF, DOCX, and TXT.
- Plugins: Persona, group chat, Caveman, Neurons, and reverse engineering.
- Think, reasoning effort, Phone, and Small-Model.
- Built-in Create recipes, once import is reliable: Image tools, Video and 3D Generate modes (text, image, and multiview), plus Animate, Extend, and the rest of the agreed sub-options.
- Optional Ollama backend.
- App awareness: read settings, installed models, workflows, and chats, and change settings only with approval of each change.
- Desktop look-around and control: screenshots first, then mouse and keyboard with approval of each action.
- macOS and Linux, as one of the very last steps, not right after v0.1.

## Decisions

1. App name: Central LLM Interface App.
2. Stack: Tauri 2 with React and TypeScript, llama.cpp as the default local runtime, and Ollama later. Decided by Jules, changeable. Rationale: Tauri stays lighter than Electron while local models and ComfyUI already use memory, and llama.cpp runs the GGUF files Johno already uses without adding a second model manager.
3. Bundle a CUDA llama-server with the app and auto-start it. Settings can still point at a custom server URL. Decided by Jules, changeable. Rationale: a simple Windows installer should just work, without a separate llama.cpp setup, and a Settings override still covers an existing server.
4. GitHub repo named after the app, Central-LLM-Interface-App, and public.
5. Windows only for now. macOS and Linux are one of the very last steps, not part of v0.1 and not the step right after it.
6. Keep Create for v0.2 as planned. Import and auto-generated inputs come first in that milestone. Built-in recipes follow once import is solid.
7. Downloaded GGUF chat models live in the app's own folder, not in the ComfyUI folder, so ComfyUI's files stay untouched.
8. Chat plugins come later, not in v0.1.
9. Ship a simple Windows installer: a standard setup that adds a Start menu shortcut and an uninstaller. The alternative was a portable zip you unpack yourself, or building the app from source with Rust and Node.

## Notes for cloud agents

- Treat this page as the source of truth. If something about Locally Uncensored is not written here, do not invent it.
- Give Johno player and user-perspective direction in the app: clear next actions, not micro hand-holding, and no walls of jargon.
- One slice per run. Stop at that slice's acceptance line. Do not start the next slice unless he asks.
- Windows only, including CI and build artifacts. Do not add macOS or Linux targets until that last-steps milestone. Budget for an RTX 5070 with 12 GB of VRAM. Default batch size to 1 on Create runs.
- Bundle and auto-start llama-server. Put downloaded chat GGUFs in the app's own folder, never in the ComfyUI folder.
- Do not implement chat plugins in v0.1.
- Never add a git-pull or Update ComfyUI action for Desktop-managed installs.
- Never store API keys in source, logs, or plaintext config.
- Product choices are in the Decisions section. If one would change the current slice, ask before overriding it.
