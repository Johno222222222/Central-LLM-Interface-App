import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { estimateTokens } from "./lib/chatText";
import {
  defaultChatSettings,
  type AppSettings,
  type Attachment,
  type CatalogModel,
  type Chat,
  type ChatDelta,
  type ChatDone,
  type ChatMessage,
  type ChatSettings,
  type ChatSummary,
  type ProviderId,
  type ProviderStatus,
  type RuntimeStatus,
  type SendStarted,
  type ToolStep,
} from "./types";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

type Listener = {
  onDelta: (event: ChatDelta) => void;
  onDone: (event: ChatDone) => void;
  onToolStep?: (event: { chatId: string; messageId: string; step: ToolStep }) => void;
  onApproval?: (event: { callId: string }) => void;
};

const listeners = new Set<Listener>();

export async function subscribe(listener: Listener): Promise<() => void> {
  if (!isTauri()) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }
  const unDelta: UnlistenFn = await listen<ChatDelta>("chat-delta", (event) => {
    listener.onDelta(event.payload);
  });
  const unDone: UnlistenFn = await listen<ChatDone>("chat-done", (event) => {
    listener.onDone(event.payload);
  });
  const unStep: UnlistenFn = await listen<{ chatId: string; messageId: string; step: ToolStep }>(
    "tool-step",
    (event) => listener.onToolStep?.(event.payload),
  );
  const unApproval: UnlistenFn = await listen<{ callId: string }>("tool-approval", (event) => {
    listener.onApproval?.(event.payload);
  });
  return () => {
    unDelta();
    unDone();
    unStep();
    unApproval();
  };
}

export const api = {
  getSettings,
  saveSettings,
  providerStatus,
  saveProviderKey,
  clearProviderKey,
  testProvider,
  runtimeStatus,
  restartRuntime,
  setModelsDir,
  listModels,
  refreshHosted,
  unloadAll,
  listChats,
  getChat,
  createChat,
  deleteChat,
  updateChatModel,
  updateChatSettings,
  stageAttachment,
  sendMessage,
  stopGeneration,
  resolveToolApproval,
  detectLuFolder,
  webSearchStatus,
  saveWebSearchKey,
  clearWebSearchKey,
  testWebSearch,
};

async function getSettings(): Promise<AppSettings> {
  if (!isTauri()) return structuredClone(preview.settings);
  return invoke("get_settings");
}

async function saveSettings(settings: AppSettings): Promise<AppSettings> {
  if (!isTauri()) {
    preview.settings = structuredClone(settings);
    return structuredClone(preview.settings);
  }
  return invoke("save_settings", { settings });
}

async function providerStatus(): Promise<ProviderStatus[]> {
  if (!isTauri()) return structuredClone(preview.providers);
  return invoke("provider_status");
}

async function saveProviderKey(provider: ProviderId, secret: string): Promise<void> {
  if (!isTauri()) {
    if (!secret.trim()) throw new Error("Enter a key before saving.");
    const row = preview.providers.find((item) => item.provider === provider);
    if (row) row.saved = true;
    preview.keys.set(provider, secret);
    return;
  }
  await invoke("save_provider_key", { provider, secret });
}

async function clearProviderKey(provider: ProviderId): Promise<void> {
  if (!isTauri()) {
    const row = preview.providers.find((item) => item.provider === provider);
    if (row) row.saved = false;
    preview.keys.delete(provider);
    return;
  }
  await invoke("clear_provider_key", { provider });
}

async function testProvider(provider: ProviderId, draft?: string): Promise<string> {
  if (!isTauri()) {
    const key = draft?.trim() || preview.keys.get(provider) || "";
    if (!key) throw new Error(`Add a ${provider} key in Settings first.`);
    if (key.includes("bad") || key.length < 8) {
      throw new Error("That key was rejected. Check it and try again.");
    }
    return `Connected to ${provider}.`;
  }
  return invoke("test_provider", { provider, draft: draft ?? null });
}

async function runtimeStatus(): Promise<RuntimeStatus> {
  if (!isTauri()) return structuredClone(preview.runtime);
  return invoke("runtime_status");
}

async function restartRuntime(): Promise<RuntimeStatus> {
  if (!isTauri()) return runtimeStatus();
  return invoke("restart_runtime");
}

async function setModelsDir(path: string): Promise<AppSettings> {
  if (!isTauri()) {
    preview.settings.modelsDir = path;
    return structuredClone(preview.settings);
  }
  return invoke("set_models_dir", { path });
}

async function listModels(): Promise<CatalogModel[]> {
  if (!isTauri()) return structuredClone(preview.models);
  return invoke("list_models");
}

async function refreshHosted(provider: ProviderId): Promise<CatalogModel[]> {
  if (!isTauri()) return listModels();
  return invoke("refresh_hosted_models", { provider });
}

async function unloadAll(): Promise<string> {
  if (!isTauri()) return "No models are loaded.";
  return invoke("unload_all_models");
}

async function listChats(): Promise<ChatSummary[]> {
  if (!isTauri()) return preview.chats.map(summarize);
  return invoke("list_chats");
}

async function getChat(id: string): Promise<Chat> {
  if (!isTauri()) {
    const chat = preview.chats.find((item) => item.id === id);
    if (!chat) throw new Error("That chat is not on this computer.");
    return structuredClone(chat);
  }
  return invoke("get_chat", { id });
}

async function createChat(provider: ProviderId, modelId: string): Promise<Chat> {
  if (!isTauri()) {
    const chat: Chat = {
      id: crypto.randomUUID(),
      title: "New chat",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      modelId,
      provider,
      messages: [],
      settings: defaultChatSettings(),
      contextUsed: 0,
      contextLimit: provider === "local" ? preview.settings.contextLength : 128000,
    };
    preview.chats.unshift(chat);
    return structuredClone(chat);
  }
  return invoke("create_chat", { provider, modelId });
}

async function deleteChat(id: string): Promise<void> {
  if (!isTauri()) {
    preview.chats = preview.chats.filter((chat) => chat.id !== id);
    return;
  }
  await invoke("delete_chat", { id });
}

async function updateChatModel(id: string, provider: ProviderId, modelId: string): Promise<Chat> {
  if (!isTauri()) {
    const chat = mustChat(id);
    chat.provider = provider;
    chat.modelId = modelId;
    chat.contextLimit = provider === "local" ? preview.settings.contextLength : 128000;
    chat.updatedAt = new Date().toISOString();
    return structuredClone(chat);
  }
  return invoke("update_chat_model", { id, provider, modelId });
}

async function updateChatSettings(id: string, settings: ChatSettings): Promise<void> {
  if (!isTauri()) {
    mustChat(id).settings = structuredClone(settings);
    return;
  }
  await invoke("update_chat_settings", { id, settings });
}

async function stageAttachment(sourcePath: string): Promise<Attachment> {
  if (!isTauri()) {
    return { name: sourcePath.split(/[/\\]/).pop() || "image", path: sourcePath, mime: "image/png" };
  }
  return invoke("stage_attachment", { sourcePath });
}

async function sendMessage(
  chatId: string,
  content: string,
  attachmentPaths: string[],
): Promise<SendStarted> {
  if (!isTauri()) return previewSend(chatId, content, attachmentPaths);
  return invoke("send_message", { chatId, content, attachmentPaths });
}

async function resolveToolApproval(callId: string, approved: boolean): Promise<void> {
  if (!isTauri()) return;
  await invoke("resolve_tool_approval", { callId, approved });
}

async function detectLuFolder(): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke("detect_lu_folder");
}

async function webSearchStatus(): Promise<boolean> {
  if (!isTauri()) return preview.webSearch;
  return invoke("web_search_status");
}

async function saveWebSearchKey(secret: string): Promise<void> {
  if (!isTauri()) {
    if (!secret.trim()) throw new Error("Enter a key before saving.");
    preview.webSearch = true;
    return;
  }
  await invoke("save_web_search_key", { secret });
}

async function clearWebSearchKey(): Promise<void> {
  if (!isTauri()) {
    preview.webSearch = false;
    return;
  }
  await invoke("clear_web_search_key");
}

async function testWebSearch(): Promise<string> {
  if (!isTauri()) {
    if (!preview.webSearch) throw new Error("Add a Brave Search key in Settings first.");
    return "Brave Search answered.";
  }
  return invoke("test_web_search");
}

async function stopGeneration(chatId: string): Promise<void> {
  if (!isTauri()) {
    const timer = preview.timers.get(chatId);
    if (timer) window.clearInterval(timer);
    preview.timers.delete(chatId);
    return;
  }
  await invoke("stop_generation", { chatId });
}

function summarize(chat: Chat): ChatSummary {
  return {
    id: chat.id,
    title: chat.title,
    createdAt: chat.createdAt,
    updatedAt: chat.updatedAt,
    modelId: chat.modelId,
    provider: chat.provider,
    contextUsed: chat.contextUsed,
    contextLimit: chat.contextLimit,
  };
}

function mustChat(id: string): Chat {
  const chat = preview.chats.find((item) => item.id === id);
  if (!chat) throw new Error("That chat is not on this computer.");
  return chat;
}

function previewSend(chatId: string, content: string, attachmentPaths: string[]): SendStarted {
  const chat = mustChat(chatId);
  if (!chat.modelId) throw new Error("Pick a model first.");
  const trimmed = content.trim();
  if (!trimmed && attachmentPaths.length === 0) throw new Error("Write a message first.");
  const user = message("user", trimmed, attachmentPaths.map((path) => ({
    name: path.split(/[/\\]/).pop() || "image",
    path,
    mime: "image/png",
  })));
  const assistant = message("assistant", "", []);
  assistant.provider = chat.provider;
  assistant.modelId = chat.modelId;
  chat.messages.push(user, assistant);
  if (chat.title === "New chat") {
    chat.title = trimmed.slice(0, 48) || "New chat";
  }
  const reply = [
    "This preview is answering without a model, so you can see the chat layout.",
    "",
    "A real reply streams from llama-server or a hosted provider. Sampling for that call is top-level on OpenAI-compatible APIs:",
    "",
    "```json",
    '{ "temperature": 0.7, "top_p": 0.95, "max_tokens": 1024 }',
    "```",
  ].join("\n");
  let cursor = 0;
  const timer = window.setInterval(() => {
    const slice = reply.slice(cursor, cursor + 18);
    cursor += 18;
    assistant.content += slice;
    const delta: ChatDelta = { chatId, messageId: assistant.id, text: slice, thinking: "" };
    for (const listener of listeners) listener.onDelta(delta);
    if (cursor >= reply.length) {
      window.clearInterval(timer);
      preview.timers.delete(chatId);
      chat.contextUsed = chat.messages.reduce(
        (sum, item) => sum + estimateTokens(item.content),
        0,
      );
      const done: ChatDone = {
        chatId,
        messageId: assistant.id,
        text: assistant.content,
        thinking: "",
        contextUsed: chat.contextUsed,
        contextLimit: chat.contextLimit,
        cancelled: false,
        title: chat.title,
      };
      for (const listener of listeners) listener.onDone(done);
    }
  }, 24);
  preview.timers.set(chatId, timer);
  return { user: structuredClone(user), assistant: structuredClone(assistant), title: chat.title };
}

function message(role: string, content: string, attachments: Attachment[]): ChatMessage {
  return {
    id: crypto.randomUUID(),
    role,
    content,
    attachments,
    createdAt: new Date().toISOString(),
    toolSteps: [],
  };
}

const seededReply = [
  "Local GGUF files in the app models folder are served by the bundled llama-server. A hosted model uses the key you saved in Windows Credential Manager.",
  "",
  "Sampling on an OpenAI-compatible call stays at the top level:",
  "",
  "```json",
  '{ "temperature": 0.7, "top_p": 0.95, "max_tokens": 1024 }',
  "```",
  "",
  "Switch models between turns from the picker. This thread stays on this computer.",
].join("\n");

const preview = {
  settings: {
    llamaServerUrl: "",
    comfyHost: "127.0.0.1",
    comfyPort: 8188,
    comfyPath: "C:\\Users\\borna\\ComfyUI\\Installs\\ComfyUI\\ComfyUI",
    modelsDir: "C:\\Users\\Johno\\AppData\\Roaming\\app.central-llm-interface\\models",
    outputDir: "C:\\Users\\Johno\\AppData\\Roaming\\app.central-llm-interface\\outputs",
    appearance: "dark",
    contextLength: 8192,
    gpuLayers: 99,
    extraModelDirs: [],
    approvedFolders: [],
    mcpServers: [],
  } as AppSettings,
  runtime: {
    mode: "bundled",
    url: "http://127.0.0.1:17860",
    running: false,
    binaryFound: false,
    binaryPath: null,
    detail:
      "The bundled llama-server was not found. The Windows installer includes it. You can also paste a server URL below.",
  } as RuntimeStatus,
  providers: [
    { provider: "openai", saved: false },
    { provider: "anthropic", saved: false },
    { provider: "gemini", saved: false },
    { provider: "deepseek", saved: false },
    { provider: "openrouter", saved: false },
  ] as ProviderStatus[],
  keys: new Map<ProviderId, string>(),
  webSearch: false,
  models: [
    {
      id: "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
      name: "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
      provider: "local",
      family: "Qwen",
      contextLimit: 8192,
      installed: true,
      path: "C:\\Users\\Johno\\AppData\\Roaming\\app.central-llm-interface\\models\\Qwen2.5-7B-Instruct-Q4_K_M.gguf",
      loaded: false,
      keySaved: true,
    },
    {
      id: "Llama-3.1-8B-Instruct-Q4_K_M.gguf",
      name: "Llama-3.1-8B-Instruct-Q4_K_M.gguf",
      provider: "local",
      family: "Llama",
      contextLimit: 8192,
      installed: true,
      loaded: false,
      keySaved: true,
    },
    hosted("openai", "GPT-4.1", "gpt-4.1"),
    hosted("openai", "GPT-4o", "gpt-4o-mini"),
    hosted("anthropic", "Claude", "claude-sonnet-4-5"),
    hosted("gemini", "Flash", "gemini-2.5-flash"),
    hosted("deepseek", "Chat", "deepseek-chat"),
    hosted("openrouter", "Anthropic", "anthropic/claude-sonnet-4.5"),
  ] as CatalogModel[],
  chats: [
    {
      id: "preview-chat",
      title: "Where replies come from",
      createdAt: "2026-11-23T12:00:00.000Z",
      updatedAt: "2026-11-23T12:04:00.000Z",
      modelId: "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
      provider: "local",
      contextUsed: 220,
      contextLimit: 8192,
      settings: defaultChatSettings(),
      messages: [
        {
          id: "preview-user",
          role: "user",
          content: "Where does a local reply come from, and where do API keys live?",
          attachments: [],
          createdAt: "2026-11-23T12:03:00.000Z",
          toolSteps: [],
        },
        {
          id: "preview-assistant",
          role: "assistant",
          content: seededReply,
          attachments: [],
          createdAt: "2026-11-23T12:04:00.000Z",
          provider: "local",
          modelId: "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
          toolSteps: [],
        },
      ],
    },
  ] as Chat[],
  timers: new Map<string, number>(),
};

function hosted(provider: ProviderId, family: string, id: string): CatalogModel {
  return {
    id,
    name: id,
    provider,
    family,
    contextLimit: 128000,
    installed: true,
    loaded: false,
    keySaved: false,
  };
}
