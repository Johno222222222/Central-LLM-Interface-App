import type { CatalogModel, Effort, ProviderId } from "../types";

const EFFORTS: Effort[] = ["default", "low", "medium", "high"];

export function cycleEffort(current: Effort): Effort {
  const index = EFFORTS.indexOf(current);
  return EFFORTS[(index + 1) % EFFORTS.length];
}

export function effortLabel(effort: Effort): string {
  switch (effort) {
    case "low":
      return "Low";
    case "medium":
      return "Medium";
    case "high":
      return "High";
    default:
      return "Default";
  }
}

export function formatTokens(value: number): string {
  if (value < 1000) return String(Math.max(0, Math.round(value)));
  const tenths = Math.round(value / 100);
  const whole = Math.floor(tenths / 10);
  const frac = tenths % 10;
  return frac === 0 ? `${whole}K` : `${whole}.${frac}K`;
}

export function formatContext(used: number, limit: number): string {
  return `${formatTokens(used)}/${formatTokens(limit)}`;
}

export function estimateTokens(text: string): number {
  if (!text) return 0;
  return Math.ceil(text.length / 4);
}

export interface ModelGroup {
  provider: ProviderId;
  family: string;
  models: CatalogModel[];
}

const PROVIDER_ORDER: ProviderId[] = [
  "local",
  "openai",
  "anthropic",
  "gemini",
  "deepseek",
  "openrouter",
];

export function groupModels(models: CatalogModel[]): ModelGroup[] {
  const groups = new Map<string, ModelGroup>();
  const ordered = [...models].sort((a, b) => {
    const provider = PROVIDER_ORDER.indexOf(a.provider) - PROVIDER_ORDER.indexOf(b.provider);
    if (provider !== 0) return provider;
    return a.family.localeCompare(b.family) || a.name.localeCompare(b.name);
  });
  for (const model of ordered) {
    const key = `${model.provider}::${model.family}`;
    const existing = groups.get(key);
    if (existing) {
      existing.models.push(model);
    } else {
      groups.set(key, { provider: model.provider, family: model.family, models: [model] });
    }
  }
  return [...groups.values()];
}

export function providerLabel(provider: ProviderId): string {
  switch (provider) {
    case "local":
      return "Local";
    case "openai":
      return "OpenAI";
    case "anthropic":
      return "Anthropic";
    case "gemini":
      return "Gemini";
    case "deepseek":
      return "DeepSeek";
    case "openrouter":
      return "OpenRouter";
  }
}
