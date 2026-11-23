import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { NAV } from "../types";
import { cycleEffort, formatContext, groupModels } from "./chatText";
import type { CatalogModel } from "../types";

describe("context meter", () => {
  it("matches the spec example", () => {
    expect(formatContext(118_800, 256_000)).toBe("118.8K/256K");
    expect(formatContext(8192, 8192)).toBe("8.2K/8.2K");
  });
});

describe("reasoning effort", () => {
  it("cycles default, low, medium, high", () => {
    expect(cycleEffort("default")).toBe("low");
    expect(cycleEffort("low")).toBe("medium");
    expect(cycleEffort("medium")).toBe("high");
    expect(cycleEffort("high")).toBe("default");
  });
});

describe("model picker groups", () => {
  it("groups by provider and family with local first", () => {
    const models: CatalogModel[] = [
      model("openai", "GPT-4o", "gpt-4o"),
      model("local", "Qwen", "qwen.gguf"),
      model("local", "Llama", "llama.gguf"),
      model("openai", "GPT-4o", "gpt-4o-mini"),
    ];
    const groups = groupModels(models);
    expect(groups.map((group) => `${group.provider}:${group.family}`)).toEqual([
      "local:Llama",
      "local:Qwen",
      "openai:GPT-4o",
    ]);
    expect(groups[2].models.map((item) => item.id)).toEqual(["gpt-4o", "gpt-4o-mini"]);
  });
});

describe("navigation", () => {
  it("has the v0.1 menu and no Compare or Voice tab", () => {
    expect(NAV.map((item) => item.label)).toEqual([
      "Chat",
      "Create",
      "Benchmark",
      "Models",
      "Settings",
    ]);
    expect(NAV.some((item) => item.id === "benchmark" && item.soon)).toBe(true);
    const labels = NAV.map((item) => item.label).join(" ");
    expect(labels).not.toContain("Compare");
    expect(labels).not.toContain("Voice");
  });
});

describe("CI", () => {
  it("builds only on Windows", () => {
    const yaml = readFileSync(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8");
    expect(yaml).toContain("windows-latest");
    expect(yaml).not.toContain("macos-latest");
    expect(yaml).not.toContain("ubuntu-latest");
    expect(yaml).toContain("upload-artifact");
    expect(yaml).toContain("cuda-13.5");
  });
});

describe("installer", () => {
  it("stops the running app before NSIS replaces llama.dll", () => {
    const config = readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8");
    const hooks = readFileSync(new URL("../../src-tauri/nsis/hooks.nsh", import.meta.url), "utf8");
    expect(config).toContain('"installerHooks": "nsis/hooks.nsh"');
    expect(hooks).toContain("NSIS_HOOK_PREINSTALL");
    expect(hooks).toContain("NSIS_HOOK_PREUNINSTALL");
    expect(hooks).toContain("taskkill.exe");
    expect(hooks).toContain("/IM llama-server.exe");
    expect(hooks).toContain("Sleep 2000");
  });
});

function model(provider: CatalogModel["provider"], family: string, id: string): CatalogModel {
  return {
    id,
    name: id,
    provider,
    family,
    contextLimit: 8192,
    installed: true,
    loaded: false,
    keySaved: provider === "local",
  };
}
