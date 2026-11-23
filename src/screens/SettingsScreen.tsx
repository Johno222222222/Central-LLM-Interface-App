import { useEffect, useState } from "react";
import { api, isTauri, onSettingsChanged } from "../api";
import { applyTheme } from "../theme";
import { PROVIDERS, type AppSettings, type Appearance, type CatalogModel, type ProviderId, type ProviderStatus, type RuntimeStatus } from "../types";
import { contextSteps } from "../components/MessageView";
import { formatTokens } from "../lib/chatText";

export function SettingsScreen({ focus = "" }: { focus?: string }) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [providers, setProviders] = useState<ProviderStatus[]>([]);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [url, setUrl] = useState("");
  const [contextLength, setContextLength] = useState(8192);
  const [gpuLayers, setGpuLayers] = useState(99);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      const [nextSettings, nextProviders, nextRuntime] = await Promise.all([
        api.getSettings(),
        api.providerStatus(),
        api.runtimeStatus(),
      ]);
      setSettings(nextSettings);
      setProviders(nextProviders);
      setRuntime(nextRuntime);
      setUrl(nextSettings.llamaServerUrl);
      setContextLength(nextSettings.contextLength);
      setGpuLayers(nextSettings.gpuLayers);
      applyTheme(nextSettings.appearance);
    })();
    let stop = () => {};
    void onSettingsChanged(() => {
      void api.getSettings().then((next) => {
        setSettings(next);
        setUrl(next.llamaServerUrl);
        setContextLength(next.contextLength);
        setGpuLayers(next.gpuLayers);
        applyTheme(next.appearance);
      });
    }).then((unlisten) => {
      stop = unlisten;
    });
    return () => stop();
  }, []);
  useEffect(() => {
    if (!focus) return;
    document.querySelector(`[data-setting="${focus}"]`)?.scrollIntoView({ block: "center" });
  }, [focus]);

  if (!settings) {
    return (
      <main className="page">
        <p className="muted">Loading settings…</p>
      </main>
    );
  }

  async function chooseAppearance(appearance: Appearance) {
    if (!settings) return;
    const next = await api.saveSettings({ ...settings, appearance });
    setSettings(next);
    applyTheme(appearance);
  }

  async function saveRuntime() {
    if (!settings) return;
    try {
      const next = await api.saveSettings({
        ...settings,
        llamaServerUrl: url.trim(),
        contextLength,
        gpuLayers,
      });
      setSettings(next);
      const status = await api.restartRuntime();
      setRuntime(status);
      setNotice(status.detail);
    } catch (reason) {
      setNotice(reason instanceof Error ? reason.message : "Could not update the local runtime.");
    }
  }

  async function changeFolder() {
    if (!isTauri()) {
      setNotice("Choose the models folder from the installed Windows app.");
      return;
    }
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    const next = await api.setModelsDir(picked);
    setSettings(next);
    setNotice("Models folder updated. The local server will use it the next time it starts.");
  }

  async function openFolder() {
    if (!settings) return;
    if (!isTauri()) {
      setNotice(settings.modelsDir);
      return;
    }
    const { openPath } = await import("@tauri-apps/plugin-opener");
    await openPath(settings.modelsDir);
  }

  return (
    <main className="page">
      <div className="page-inner">
        <p className="eyebrow">Settings</p>
        <h1>Settings</h1>
        <p className="muted">
          Keys stay in Windows Credential Manager. Chat models live in this app&apos;s folder, not in ComfyUI.
        </p>

        <section className="card" data-setting="appearance">
          <h2>Appearance</h2>
          <div className="choice-row">
            {(["dark", "light", "system"] as Appearance[]).map((appearance) => (
              <button
                key={appearance}
                type="button"
                className={settings.appearance === appearance ? "choice on" : "choice"}
                onClick={() => void chooseAppearance(appearance)}
              >
                {appearance[0].toUpperCase() + appearance.slice(1)}
              </button>
            ))}
          </div>
        </section>

        <section className="card" data-setting="contextLength">
          <h2>Local runtime</h2>
          <p className={runtime?.running ? "status-ok" : "muted"}>{runtime?.detail}</p>
          <p className="hint">
            {runtime?.mode === "custom" ? "Custom server" : "Bundled server"} · {runtime?.url}
            {runtime?.binaryFound && runtime.binaryPath ? ` · ${runtime.binaryPath}` : ""}
          </p>
          <label className="field">
            Custom server URL
            <input
              value={url}
              placeholder="Leave empty to auto-start the bundled llama-server"
              onChange={(event) => setUrl(event.target.value)}
            />
          </label>
          <label className="field">
            Context length
            <input
              type="number"
              min={512}
              step={256}
              value={contextLength}
              onChange={(event) => setContextLength(Number(event.target.value))}
            />
          </label>
          <label className="field">
            GPU layers
            <input
              type="number"
              min={0}
              max={999}
              value={gpuLayers}
              onChange={(event) => setGpuLayers(Number(event.target.value))}
            />
          </label>
          <p className="hint">
            This number is only the fallback. Each local model gets its own context from VRAM and file size. A 9B Q4 on 12 GB lands near 32K. Changing a model slider restarts the bundled server.
          </p>
          <div className="button-row">
            <button type="button" className="primary" onClick={() => void saveRuntime()}>
              Save and restart
            </button>
            <button
              type="button"
              className="secondary"
              onClick={() => void api.unloadAll().then(setNotice).catch((reason: unknown) => setNotice(String(reason)))}
            >
              Unload all models
            </button>
          </div>
        </section>

        <section className="card" data-setting="modelsDir">
          <h2>Models folder</h2>
          <p>{settings.modelsDir}</p>
          <p className="hint">Put GGUF chat models here. This is separate from the ComfyUI models folder.</p>
          <div className="button-row">
            <button type="button" className="secondary" onClick={() => void changeFolder()}>
              Change folder
            </button>
            <button type="button" className="secondary" onClick={() => void openFolder()}>
              Open folder
            </button>
          </div>
        </section>

        <ModelContext onNotice={setNotice} />

        <section className="card">
          <h2>Feedback</h2>
          <p className="hint">Thumbs on an assistant reply are stored on this computer.</p>
          <button
            type="button"
            className="secondary"
            onClick={() => {
              void api.exportFeedback().then((json) => {
                const blob = new Blob([json], { type: "application/json" });
                const url = URL.createObjectURL(blob);
                const link = document.createElement("a");
                link.href = url;
                link.download = "central-llm-feedback.json";
                link.click();
                URL.revokeObjectURL(url);
                setNotice("Exported feedback.");
              });
            }}
          >
            Export feedback
          </button>
        </section>

        <ExtraFolders settings={settings} onChange={setSettings} onNotice={setNotice} />
        <WebSearchCard />
        <ApprovedFolders settings={settings} onChange={setSettings} />
        <McpServers settings={settings} onChange={setSettings} />

        <section className="card">
          <h2>Providers and keys</h2>
          <p className="hint">A saved key can be tested. The key itself is never shown again and is not stored in a file.</p>
          {PROVIDERS.map((provider) => (
            <ProviderRow
              key={provider.id}
              id={provider.id}
              label={provider.label}
              hint={provider.hint}
              saved={providers.some((item) => item.provider === provider.id && item.saved)}
              onChanged={async () => setProviders(await api.providerStatus())}
            />
          ))}
        </section>
        {notice ? <p className="hint">{notice}</p> : null}
      </div>
    </main>
  );
}

function ModelContext({ onNotice }: { onNotice: (text: string) => void }) {
  const [models, setModels] = useState<CatalogModel[]>([]);
  useEffect(() => {
    void api.listModels().then((catalog) => setModels(catalog.filter((model) => model.provider === "local")));
  }, []);
  if (models.length === 0) return null;
  return (
    <section className="card">
      <h2>Context per model</h2>
      <p className="hint">The token pill uses this window. Moving a slider saves it and restarts the local server.</p>
      {models.map((model) => {
        const steps = contextSteps(model.contextMax || model.contextLimit);
        const index = Math.max(0, steps.indexOf(model.contextLimit));
        return (
          <label key={model.id} className="field">
            {model.name}
            <span className="hint">
              {formatTokens(model.contextLimit)}
              {model.contextAuto ? ` · auto ${formatTokens(model.contextAuto)}` : ""}
              {model.mmprojPath ? " · vision projector found" : ""}
            </span>
            <input
              type="range"
              min={0}
              max={steps.length - 1}
              step={1}
              value={index}
              onChange={(event) => {
                const next = steps[Number(event.target.value)] ?? model.contextLimit;
                void api
                  .setModelContext(model.id, next)
                  .then((updated) => {
                    setModels((list) => list.map((item) => (item.id === updated.id ? updated : item)));
                    onNotice(`${model.name} is using ${formatTokens(updated.contextLimit)}.`);
                  })
                  .catch((reason: unknown) => onNotice(reason instanceof Error ? reason.message : String(reason)));
              }}
            />
          </label>
        );
      })}
    </section>
  );
}

function ExtraFolders({
  settings,
  onChange,
  onNotice,
}: {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
  onNotice: (notice: string) => void;
}) {
  async function addFolder() {
    if (!isTauri()) {
      onNotice("Choose extra model folders from the installed Windows app.");
      return;
    }
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    const next = await api.saveSettings({
      ...settings,
      extraModelDirs: [...settings.extraModelDirs, picked],
    });
    onChange(next);
    await api.restartRuntime();
    onNotice("That folder will be scanned in place. Nothing is copied.");
  }

  async function importLu() {
    const found = await api.detectLuFolder();
    if (!found) {
      onNotice("No Locally Uncensored model folder was found. Browse to it instead.");
      return;
    }
    if (settings.extraModelDirs.includes(found)) {
      onNotice("That Locally Uncensored folder is already added.");
      return;
    }
    const next = await api.saveSettings({
      ...settings,
      extraModelDirs: [...settings.extraModelDirs, found],
    });
    onChange(next);
    await api.restartRuntime();
    onNotice(`Added ${found}. Models stay where they are.`);
  }

  async function remove(path: string) {
    const next = await api.saveSettings({
      ...settings,
      extraModelDirs: settings.extraModelDirs.filter((item) => item !== path),
    });
    onChange(next);
    await api.restartRuntime();
  }

  return (
    <section className="card" data-setting="extraModelDirs">
      <h2>More model folders</h2>
      <p className="hint">
        GGUF files in these folders are read where they are. A later version can move them into this app&apos;s folder.
      </p>
      {settings.extraModelDirs.map((path) => (
        <div key={path} className="folder-row">
          <span>{path}</span>
          <button type="button" className="secondary" onClick={() => void remove(path)}>
            Remove
          </button>
        </div>
      ))}
      <div className="button-row">
        <button type="button" className="secondary" onClick={() => void addFolder()}>
          Add folder
        </button>
        <button type="button" className="secondary" onClick={() => void importLu()}>
          Import from Locally Uncensored
        </button>
      </div>
    </section>
  );
}

function WebSearchCard() {
  const [draft, setDraft] = useState("");
  const [saved, setSaved] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  useEffect(() => {
    void api.webSearchStatus().then(setSaved).catch(() => setSaved(false));
  }, []);
  return (
    <section className="card">
      <h2>Web search</h2>
      <p className="hint">
        Brave Search, with your own key. The free plan is enough to try it. The key stays in Windows Credential Manager.
      </p>
      <p className={saved ? "status-ok" : "hint"}>{saved ? "Brave key saved" : "No Brave key saved"}</p>
      <input
        type="password"
        autoComplete="off"
        placeholder="Paste a Brave Search key"
        aria-label="Brave Search API key"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
      />
      <div className="button-row">
        <button
          type="button"
          className="secondary"
          onClick={() =>
            void api.saveWebSearchKey(draft).then(() => {
              setDraft("");
              setSaved(true);
              setStatus("Saved in Windows Credential Manager.");
            }).catch((reason: unknown) => setStatus(reason instanceof Error ? reason.message : "Could not save that key."))
          }
        >
          Save
        </button>
        <button
          type="button"
          className="secondary"
          onClick={() =>
            void api.testWebSearch().then(setStatus).catch((reason: unknown) => setStatus(reason instanceof Error ? reason.message : "Search failed."))
          }
        >
          Test
        </button>
        <button
          type="button"
          className="secondary"
          disabled={!saved}
          onClick={() => void api.clearWebSearchKey().then(() => { setSaved(false); setStatus("Key removed."); })}
        >
          Remove
        </button>
      </div>
      {status ? <p className="hint">{status}</p> : null}
    </section>
  );
}

function ApprovedFolders({
  settings,
  onChange,
}: {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
}) {
  async function addFolder() {
    if (!isTauri()) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    onChange(await api.saveSettings({ ...settings, approvedFolders: [...settings.approvedFolders, picked] }));
  }
  return (
    <section className="card">
      <h2>Folders the Files tool may read</h2>
      <p className="hint">The Files tool stays off until you turn it on in a chat, and it can only read these folders.</p>
      {settings.approvedFolders.map((path) => (
        <div key={path} className="folder-row">
          <span>{path}</span>
          <button
            type="button"
            className="secondary"
            onClick={() =>
              void api
                .saveSettings({ ...settings, approvedFolders: settings.approvedFolders.filter((item) => item !== path) })
                .then(onChange)
            }
          >
            Remove
          </button>
        </div>
      ))}
      <button type="button" className="secondary" onClick={() => void addFolder()}>
        Approve a folder
      </button>
    </section>
  );
}

function McpServers({
  settings,
  onChange,
}: {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
}) {
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  async function addServer() {
    if (!name.trim() || !command.trim()) return;
    const next = await api.saveSettings({
      ...settings,
      mcpServers: [
        ...settings.mcpServers,
        {
          id: crypto.randomUUID(),
          name: name.trim(),
          command: command.trim(),
          args: args.split(" ").filter(Boolean),
          enabled: true,
        },
      ],
    });
    onChange(next);
    setName("");
    setCommand("");
    setArgs("");
  }
  return (
    <section className="card">
      <h2>MCP servers</h2>
      <p className="hint">Turn MCP on in a chat to use tools from these local servers. Each server is a command the app starts.</p>
      {settings.mcpServers.map((server) => (
        <div key={server.id} className="folder-row">
          <span>
            {server.name} · {server.command} {server.args.join(" ")}
          </span>
          <button
            type="button"
            className="secondary"
            onClick={() =>
              void api
                .saveSettings({ ...settings, mcpServers: settings.mcpServers.filter((item) => item.id !== server.id) })
                .then(onChange)
            }
          >
            Remove
          </button>
        </div>
      ))}
      <label className="field">
        Name
        <input value={name} onChange={(event) => setName(event.target.value)} />
      </label>
      <label className="field">
        Command
        <input value={command} onChange={(event) => setCommand(event.target.value)} placeholder="npx" />
      </label>
      <label className="field">
        Arguments
        <input value={args} onChange={(event) => setArgs(event.target.value)} placeholder="-y @modelcontextprotocol/server-filesystem" />
      </label>
      <button type="button" className="secondary" onClick={() => void addServer()}>
        Add server
      </button>
    </section>
  );
}

function ProviderRow({
  id,
  label,
  hint,
  saved,
  onChanged,
}: {
  id: ProviderId;
  label: string;
  hint: string;
  saved: boolean;
  onChanged: () => Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [bad, setBad] = useState(false);

  async function run(action: () => Promise<unknown>, ok: string) {
    try {
      const result = await action();
      setBad(false);
      setStatus(typeof result === "string" ? result : ok);
      await onChanged();
    } catch (reason) {
      setBad(true);
      setStatus(reason instanceof Error ? reason.message : "That did not work.");
    }
  }

  return (
    <div className="provider-card">
      <h3>
        {label} <span className="hint">{hint}</span>
      </h3>
      <p className={saved ? "status-ok" : "hint"}>{saved ? "Saved in Windows Credential Manager" : "No key saved"}</p>
      <input
        type="password"
        autoComplete="off"
        placeholder="Paste a new key"
        aria-label={`${label} API key`}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
      />
      <div className="provider-actions">
        <button
          type="button"
          className="secondary"
          onClick={() =>
            void run(async () => {
              await api.saveProviderKey(id, draft);
              setDraft("");
            }, "Saved in Windows Credential Manager.")
          }
        >
          Save
        </button>
        <button type="button" className="secondary" onClick={() => void run(() => api.testProvider(id, draft || undefined), "Connected.")}>
          Test connection
        </button>
        <button
          type="button"
          className="secondary"
          disabled={!saved}
          onClick={() => void run(() => api.clearProviderKey(id), "Key removed.")}
        >
          Remove
        </button>
      </div>
      {status ? <p className={bad ? "status-bad" : "hint"}>{status}</p> : null}
    </div>
  );
}
