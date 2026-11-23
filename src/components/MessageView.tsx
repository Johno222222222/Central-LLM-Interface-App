import { useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { api } from "../api";
import { providerLabel } from "../lib/chatText";
import type { CatalogModel, ChatMessage } from "../types";

const CONTEXT_STEPS = [2048, 4096, 8192, 16384, 32768, 65536, 131072];

export function contextSteps(max: number): number[] {
  const ceiling = Math.max(2048, max || 131072);
  const steps = CONTEXT_STEPS.filter((step) => step <= ceiling);
  return steps.length ? steps : [2048];
}

export function MessageView({
  message,
  streaming,
  models,
  busy,
  onEdit,
  onRetry,
  onBranch,
  onFeedback,
}: {
  message: ChatMessage;
  streaming: boolean;
  models: CatalogModel[];
  busy: boolean;
  onEdit: (message: ChatMessage) => void;
  onRetry: (message: ChatMessage, modelId?: string) => void;
  onBranch: (message: ChatMessage) => void;
  onFeedback: (message: ChatMessage, rating: "up" | "down", note: string) => void;
}) {
  const user = message.role === "user";
  const body = message.content || (streaming ? "…" : "");
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [note, setNote] = useState("");
  const [rating, setRating] = useState<"up" | "down" | null>(null);
  const [retryModel, setRetryModel] = useState("");
  const choices = models.filter((model) => model.installed);

  function copy() {
    void navigator.clipboard?.writeText(message.content).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    });
  }

  return (
    <article className={user ? "msg user" : "msg assistant"}>
      <div className="who">
        {user ? "You" : "Assistant"}
        {!user && message.modelId ? ` · ${message.modelId}` : ""}
        {!user && message.provider ? ` · ${providerLabel(message.provider)}` : ""}
      </div>
      <div className={`msg-actions${open ? " open" : ""}`}>
        <button type="button" onClick={copy} disabled={!message.content}>
          {copied ? "Copied" : "Copy"}
        </button>
        {user ? (
          <button type="button" disabled={busy || streaming} onClick={() => onEdit(message)}>
            Edit
          </button>
        ) : (
          <div className="overflow">
            <button type="button" aria-label="Message actions" onClick={() => setOpen((value) => !value)}>
              ···
            </button>
            {open ? (
              <div className="overflow-menu">
                <button
                  type="button"
                  disabled={busy || streaming}
                  onClick={() => {
                    setOpen(false);
                    onRetry(message);
                  }}
                >
                  Retry
                </button>
                <label className="menu-field">
                  Retry with
                  <select value={retryModel} onChange={(event) => setRetryModel(event.target.value)}>
                    <option value="">Current model</option>
                    {choices.map((model) => (
                      <option key={`${model.provider}:${model.id}`} value={model.id}>
                        {model.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  disabled={busy || streaming || !retryModel}
                  onClick={() => {
                    setOpen(false);
                    onRetry(message, retryModel);
                  }}
                >
                  Regenerate
                </button>
                <button
                  type="button"
                  disabled={busy || streaming}
                  onClick={() => {
                    setOpen(false);
                    onBranch(message);
                  }}
                >
                  Branch
                </button>
                <div className="thumb-row">
                  <button
                    type="button"
                    className={rating === "up" ? "choice on" : "choice"}
                    onClick={() => {
                      setRating("up");
                      onFeedback(message, "up", note);
                    }}
                  >
                    Up
                  </button>
                  <button
                    type="button"
                    className={rating === "down" ? "choice on" : "choice"}
                    onClick={() => setRating("down")}
                  >
                    Down
                  </button>
                </div>
                {rating === "down" ? (
                  <form
                    className="menu-note"
                    onSubmit={(event) => {
                      event.preventDefault();
                      onFeedback(message, "down", note);
                      setOpen(false);
                    }}
                  >
                    <input
                      value={note}
                      placeholder="Optional note"
                      onChange={(event) => setNote(event.target.value)}
                    />
                    <button type="submit">Save</button>
                  </form>
                ) : null}
              </div>
            ) : null}
          </div>
        )}
      </div>
      <div className="bubble">
        {message.toolSteps?.length ? (
          <div className="tool-steps">
            {message.toolSteps.map((step) => (
              <details key={step.id} className="thinking" open={step.status === "running" || step.status === "approval"}>
                <summary>
                  {step.name} · {step.status}
                </summary>
                {step.name === "update_settings" && step.status === "approval" ? (
                  <div className="approval-card">
                    <strong>Change setting</strong>
                    <p>{step.arguments}</p>
                    <div className="choice-row">
                      <button type="button" className="primary" onClick={() => void api.resolveToolApproval(step.id, true)}>
                        Approve
                      </button>
                      <button type="button" className="secondary" onClick={() => void api.resolveToolApproval(step.id, false)}>
                        Deny
                      </button>
                    </div>
                  </div>
                ) : (
                  <pre className="code-block">{step.arguments}</pre>
                )}
                {step.result ? <p>{step.result}</p> : null}
              </details>
            ))}
          </div>
        ) : null}
        {message.thinking ? (
          <details className="thinking" open={streaming}>
            <summary>Thinking</summary>
            <p>{message.thinking}</p>
          </details>
        ) : null}
        {message.attachments.length > 0 ? (
          <p className="muted">{message.attachments.map((file) => file.name).join(", ")}</p>
        ) : null}
        <ReactMarkdown remarkPlugins={[remarkGfm]} components={{ code: CodeBlock }}>
          {body}
        </ReactMarkdown>
      </div>
    </article>
  );
}

function CodeBlock({ className, children }: { className?: string; children?: ReactNode }) {
  const text = String(children).replace(/\n$/, "");
  const inline = !className && !text.includes("\n");
  const [copied, setCopied] = useState(false);
  if (inline) return <code className="inline-code">{text}</code>;
  const language = className?.replace("language-", "") ?? "";
  return (
    <pre className="code-block">
      {language ? <div className="code-lang">{language}</div> : null}
      <button
        type="button"
        className="copy-code"
        onClick={() => {
          void navigator.clipboard?.writeText(text).then(() => {
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1200);
          });
        }}
      >
        {copied ? "Copied" : "Copy"}
      </button>
      <code className={className}>{text}</code>
    </pre>
  );
}
