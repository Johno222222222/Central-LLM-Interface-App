import type { ChatSettings } from "../types";

export function SamplingPopover({
  settings,
  onChange,
  onClose,
}: {
  settings: ChatSettings;
  onChange: (settings: ChatSettings) => void;
  onClose: () => void;
}) {
  const entries = Object.entries(settings.customParams);
  function update(patch: Partial<ChatSettings>) {
    onChange({ ...settings, ...patch });
  }
  function setParam(index: number, key: string, raw: string) {
    const next = Object.fromEntries(entries);
    const previous = entries[index]?.[0];
    if (previous && previous !== key) delete next[previous];
    if (key.trim()) next[key.trim()] = parseParam(raw);
    update({ customParams: next });
  }

  return (
    <div className="popover" role="dialog" aria-label="Sampling">
      <div className="field">
        <label htmlFor="temperature">Temperature</label>
        <input
          id="temperature"
          type="number"
          min={0}
          max={2}
          step={0.05}
          value={settings.temperature}
          onChange={(event) => update({ temperature: Number(event.target.value) })}
        />
      </div>
      <div className="field">
        <label htmlFor="top-p">Top P</label>
        <input
          id="top-p"
          type="number"
          min={0}
          max={1}
          step={0.01}
          value={settings.topP}
          onChange={(event) => update({ topP: Number(event.target.value) })}
        />
      </div>
      <div className="field">
        <label htmlFor="max-tokens">Max tokens</label>
        <input
          id="max-tokens"
          type="number"
          min={1}
          step={1}
          value={settings.maxTokens}
          onChange={(event) => update({ maxTokens: Number(event.target.value) })}
        />
      </div>
      <div className="field">
        <label htmlFor="think-budget">Think budget</label>
        <input
          id="think-budget"
          type="number"
          min={0}
          step={1}
          placeholder="Anthropic and Gemini"
          value={settings.thinkBudget ?? ""}
          onChange={(event) => {
            const value = event.target.value;
            update({ thinkBudget: value === "" ? undefined : Number(value) });
          }}
        />
      </div>
      <div className="field">
        <span>Custom parameters</span>
        {entries.map(([key, value], index) => (
          <div className="choice-row" key={`${key}-${index}`}>
            <input
              aria-label="Parameter name"
              value={key}
              onChange={(event) => setParam(index, event.target.value, String(value ?? ""))}
            />
            <input
              aria-label="Parameter value"
              value={String(value ?? "")}
              onChange={(event) => setParam(index, key, event.target.value)}
            />
          </div>
        ))}
        <button
          type="button"
          className="secondary"
          onClick={() => update({ customParams: { ...settings.customParams, "": "" } })}
        >
          Add parameter
        </button>
      </div>
      <p className="hint">
        Temperature, top P, and max tokens go out with the request. Think budget is sent to
        Anthropic and Gemini. Custom parameters are added on OpenAI-compatible calls.
      </p>
      <button type="button" className="secondary" onClick={onClose}>
        Close
      </button>
    </div>
  );
}

function parseParam(raw: string): unknown {
  const trimmed = raw.trim();
  if (trimmed === "true") return true;
  if (trimmed === "false") return false;
  if (trimmed !== "" && !Number.isNaN(Number(trimmed))) return Number(trimmed);
  if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
    try {
      return JSON.parse(trimmed) as unknown;
    } catch {
      return raw;
    }
  }
  return raw;
}
