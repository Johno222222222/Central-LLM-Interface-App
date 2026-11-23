export type ProviderId =
  | "local"
  | "openai"
  | "anthropic"
  | "gemini"
  | "deepseek"
  | "openrouter";

export type Effort = "default" | "low" | "medium" | "high";
export type Appearance = "system" | "light" | "dark";
export type ScreenId = "chat" | "create" | "benchmark" | "models" | "settings";

export interface ChatSettings {
  temperature: number;
  topP: number;
  maxTokens: number;
  thinkBudget?: number;
  customParams: Record<string, unknown>;
  agent: boolean;
  sandbox: boolean;
  code: boolean;
  voice: boolean;
  think: boolean;
  effort: Effort;
  tools: ToolToggles;
}

export interface ToolToggles {
  webSearch: boolean;
  webFetch: boolean;
  files: boolean;
  shell: boolean;
  screenshot: boolean;
  mcp: boolean;
}

export interface ToolStep {
  id: string;
  name: string;
  arguments: string;
  result?: string;
  status: string;
}

export interface McpServerConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  enabled: boolean;
}

export interface AppSettings {
  llamaServerUrl: string;
  comfyHost: string;
  comfyPort: number;
  comfyPath: string;
  modelsDir: string;
  outputDir: string;
  appearance: Appearance;
  contextLength: number;
  gpuLayers: number;
  extraModelDirs: string[];
  approvedFolders: string[];
  mcpServers: McpServerConfig[];
}

export interface Attachment {
  name: string;
  path: string;
  mime: string;
}

export interface ChatMessage {
  id: string;
  role: string;
  content: string;
  thinking?: string;
  attachments: Attachment[];
  createdAt: string;
  provider?: ProviderId;
  modelId?: string;
  toolSteps: ToolStep[];
}

export interface ChatSummary {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  modelId: string;
  provider: ProviderId;
  contextUsed: number;
  contextLimit: number;
}

export interface Chat extends ChatSummary {
  messages: ChatMessage[];
  settings: ChatSettings;
}

export interface CatalogModel {
  id: string;
  name: string;
  provider: ProviderId;
  family: string;
  contextLimit: number;
  installed: boolean;
  path?: string | null;
  loaded: boolean;
  keySaved: boolean;
  sourceLabel?: string;
}

export interface ProviderStatus {
  provider: ProviderId;
  saved: boolean;
}

export interface RuntimeStatus {
  mode: "bundled" | "custom";
  url: string;
  running: boolean;
  binaryFound: boolean;
  binaryPath?: string | null;
  detail: string;
}

export interface ChatDelta {
  chatId: string;
  messageId: string;
  text: string;
  thinking: string;
}

export interface ChatDone {
  chatId: string;
  messageId: string;
  text: string;
  thinking: string;
  contextUsed: number;
  contextLimit: number;
  error?: string | null;
  cancelled: boolean;
  title: string;
}

export interface SendStarted {
  user: ChatMessage;
  assistant: ChatMessage;
  title: string;
}

export const PROVIDERS: { id: ProviderId; label: string; hint: string }[] = [
  { id: "openai", label: "OpenAI", hint: "api.openai.com" },
  { id: "anthropic", label: "Anthropic", hint: "api.anthropic.com" },
  { id: "gemini", label: "Gemini", hint: "Google AI Studio" },
  { id: "deepseek", label: "DeepSeek", hint: "api.deepseek.com" },
  { id: "openrouter", label: "OpenRouter", hint: "Catch-all for other hosted models" },
];

export const NAV: { id: ScreenId; label: string; soon?: boolean }[] = [
  { id: "chat", label: "Chat" },
  { id: "create", label: "Create" },
  { id: "benchmark", label: "Benchmark", soon: true },
  { id: "models", label: "Models" },
  { id: "settings", label: "Settings" },
];

export function defaultChatSettings(): ChatSettings {
  return {
    temperature: 0.7,
    topP: 0.95,
    maxTokens: 1024,
    customParams: {},
    agent: false,
    sandbox: false,
    code: false,
    voice: false,
    think: false,
    effort: "default",
    tools: {
      webSearch: false,
      webFetch: false,
      files: false,
      shell: false,
      screenshot: false,
      mcp: false,
    },
  };
}

export const TOOL_TOGGLES: { key: keyof ToolToggles; label: string; hint: string }[] = [
  { key: "webSearch", label: "Web search", hint: "Needs a Brave Search key" },
  { key: "webFetch", label: "Web fetch", hint: "Public http and https pages" },
  { key: "files", label: "Files", hint: "Read folders you approve" },
  { key: "shell", label: "Shell", hint: "You approve each command" },
  { key: "screenshot", label: "Screenshot", hint: "Capture the primary screen" },
  { key: "mcp", label: "MCP", hint: "Servers from Settings" },
];
