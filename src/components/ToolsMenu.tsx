import { useState } from "react";
import { TOOL_TOGGLES, type ChatSettings } from "../types";

export function ToolsMenu({
  settings,
  disabled,
  onChange,
}: {
  settings: ChatSettings;
  disabled: boolean;
  onChange: (settings: ChatSettings) => void;
}) {
  const [open, setOpen] = useState(false);
  const enabled = TOOL_TOGGLES.filter((tool) => settings.tools[tool.key]).length;
  return (
    <>
      <button
        type="button"
        className={enabled > 0 ? "chip on" : "chip"}
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
      >
        Tools{enabled > 0 ? ` ${enabled}` : ""}
      </button>
      {open ? (
        <div className="popover tools-menu" role="dialog" aria-label="Tools">
          {TOOL_TOGGLES.map((tool) => {
            const on = settings.tools[tool.key];
            return (
              <label key={tool.key} className="tool-row">
                <span className={on ? "dot on" : "dot"} aria-hidden="true" />
                <span>
                  <strong>{tool.label}</strong>
                  <small>{tool.hint}</small>
                </span>
                <input
                  type="checkbox"
                  checked={on}
                  onChange={() =>
                    onChange({
                      ...settings,
                      tools: { ...settings.tools, [tool.key]: !on },
                    })
                  }
                />
              </label>
            );
          })}
          <p className="hint">App can look at this app. A settings change asks first. Shell asks before every command. Files only read folders you approve in Settings.</p>
          <button type="button" className="secondary" onClick={() => setOpen(false)}>
            Close
          </button>
        </div>
      ) : null}
    </>
  );
}
