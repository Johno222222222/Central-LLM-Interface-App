import { useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { providerLabel } from "../lib/chatText";
import type { ChatMessage } from "../types";

export function MessageView({ message, streaming }: { message: ChatMessage; streaming: boolean }) {
  const user = message.role === "user";
  const body = message.content || (streaming ? "…" : "");
  return (
    <article className={user ? "msg user" : "msg assistant"}>
      <div className="who">
        {user ? "You" : "Assistant"}
        {!user && message.modelId ? ` · ${message.modelId}` : ""}
        {!user && message.provider ? ` · ${providerLabel(message.provider)}` : ""}
      </div>
      <div className="bubble">
        {message.toolSteps?.length ? (
          <div className="tool-steps">
            {message.toolSteps.map((step) => (
              <details key={step.id} className="thinking" open={step.status === "running" || step.status === "approval"}>
                <summary>
                  {step.name} · {step.status}
                </summary>
                <pre className="code-block">{step.arguments}</pre>
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
