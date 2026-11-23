import { useMemo, useState } from "react";
import { api } from "../api";
import { groupModels, providerLabel } from "../lib/chatText";
import type { CatalogModel, ProviderId } from "../types";

export function ModelPicker({
  models,
  selectedId,
  disabled,
  onSelect,
  onModels,
}: {
  models: CatalogModel[];
  selectedId: string;
  disabled: boolean;
  onSelect: (model: CatalogModel) => void;
  onModels: (models: CatalogModel[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [customProvider, setCustomProvider] = useState<ProviderId>("openai");
  const [customId, setCustomId] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const selected = models.find((model) => model.id === selectedId);
  const groups = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = needle
      ? models.filter((model) => `${model.name} ${model.family}`.toLowerCase().includes(needle))
      : models;
    return groupModels(filtered);
  }, [models, query]);

  const providers = [...new Set(groups.map((group) => group.provider))];

  async function unload() {
    try {
      setNote(await api.unloadAll());
      onModels(await api.listModels());
    } catch (error) {
      setNote(error instanceof Error ? error.message : "Could not unload models.");
    }
  }

  async function refresh(provider: ProviderId) {
    try {
      const fresh = await api.refreshHosted(provider);
      const rest = models.filter((model) => model.provider !== provider);
      onModels([...rest, ...fresh]);
      setNote(`Updated the ${providerLabel(provider)} list.`);
    } catch (error) {
      setNote(error instanceof Error ? error.message : "Could not refresh that provider.");
    }
  }

  return (
    <>
      <button
        type="button"
        className="model-button"
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
      >
        {selected ? `${providerLabel(selected.provider)} · ${selected.name}` : selectedId || "Pick a model"}
      </button>
      {open ? (
        <div className="picker" role="dialog" aria-label="Models">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search models"
            aria-label="Search models"
          />
          <div className="picker-scroll">
            {providers.map((provider) => (
              <section key={provider}>
                <div className="choice-row">
                  <h3>{providerLabel(provider)}</h3>
                  {provider !== "local" ? (
                    <button type="button" className="chip" onClick={() => void refresh(provider)}>
                      Refresh
                    </button>
                  ) : null}
                </div>
                {groups
                  .filter((group) => group.provider === provider)
                  .map((group) => (
                    <div key={`${group.provider}-${group.family}`}>
                      <h4>{group.family}</h4>
                      {group.models.map((model) => (
                        <button
                          key={`${model.provider}-${model.id}`}
                          type="button"
                          className="model-option"
                          disabled={!model.keySaved && model.provider !== "local"}
                          onClick={() => {
                            onSelect(model);
                            setOpen(false);
                          }}
                        >
                          <strong>{model.name}</strong>
                          <small>
                            {model.loaded ? "Loaded" : model.keySaved || model.provider === "local" ? "" : "Add a key in Settings"}
                          </small>
                        </button>
                      ))}
                    </div>
                  ))}
              </section>
            ))}
            {groups.length === 0 ? <p className="hint">No models match that search.</p> : null}
          </div>
          <div className="custom-model">
            <select
              aria-label="Provider for a custom model id"
              value={customProvider}
              onChange={(event) => setCustomProvider(event.target.value as ProviderId)}
            >
              {(["local", "openai", "anthropic", "gemini", "deepseek", "openrouter"] as ProviderId[]).map(
                (provider) => (
                  <option key={provider} value={provider}>
                    {providerLabel(provider)}
                  </option>
                ),
              )}
            </select>
            <input
              aria-label="Custom model id"
              value={customId}
              placeholder="Custom model id"
              onChange={(event) => setCustomId(event.target.value)}
            />
            <button
              type="button"
              className="secondary"
              onClick={() => {
                const id = customId.trim();
                if (!id) return;
                onSelect({
                  id,
                  name: id,
                  provider: customProvider,
                  family: "Custom",
                  contextLimit: customProvider === "local" ? 8192 : 128000,
                  installed: true,
                  loaded: false,
                  keySaved: true,
                });
                setOpen(false);
              }}
            >
              Use
            </button>
          </div>
          <div className="picker-foot">
            <button type="button" className="secondary" onClick={() => void unload()}>
              Unload all models
            </button>
            {note ? <span className="hint">{note}</span> : null}
          </div>
        </div>
      ) : null}
    </>
  );
}
