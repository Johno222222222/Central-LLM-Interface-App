import { useEffect, useState } from "react";
import { api, isTauri } from "../api";
import { applyTheme } from "../theme";
import { PROVIDERS, type AppSettings, type Appearance, type ProviderId, type ProviderStatus, type RuntimeStatus } from "../types";

export function SettingsScreen() {
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
  }, []);

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

        <section className="card">
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

        <section className="card">
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
            The bundled server keeps one model loaded and leaves about 2 GB of VRAM free on a 12 GB card.
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

        <section className="card">
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
