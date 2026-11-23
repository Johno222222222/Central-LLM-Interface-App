import { useEffect, useRef, useState, type Dispatch, type RefObject, type SetStateAction } from "react";
import { api, isTauri, subscribe } from "../api";
import { MessageView } from "../components/MessageView";
import { ToolsMenu } from "../components/ToolsMenu";
import { ModelPicker } from "../components/ModelPicker";
import { SamplingPopover } from "../components/SamplingPopover";
import { cycleEffort, effortLabel, estimateTokens, formatContext } from "../lib/chatText";
import type { Attachment, CatalogModel, Chat, ChatMessage, ChatSummary } from "../types";

export function ChatScreen() {
  const [chats, setChats] = useState<ChatSummary[]>([]);
  const [chat, setChat] = useState<Chat | null>(null);
  const [models, setModels] = useState<CatalogModel[]>([]);
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState<Attachment[]>([]);
  const [streamingId, setStreamingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sampling, setSampling] = useState(false);
  const [pendingApproval, setPendingApproval] = useState<string | null>(null);
  const buffers = useRef(new Map<string, { text: string; thinking: string }>());
  const fileRef = useRef<HTMLInputElement>(null);
  const chatId = chat?.id;

  useEffect(() => {
    let stop = () => {};
    void subscribe({
      onDelta: (event) => {
        const current = buffers.current.get(event.messageId) ?? { text: "", thinking: "" };
        current.text += event.text;
        current.thinking += event.thinking;
        buffers.current.set(event.messageId, current);
        setChat((existing) => paint(existing, event.messageId, buffers.current));
      },
      onToolStep: (event) => {
        setChat((existing) => {
          if (!existing || existing.id !== event.chatId) return existing;
          return {
            ...existing,
            messages: existing.messages.map((message) => {
              if (message.id !== event.messageId) return message;
              const steps = message.toolSteps ?? [];
              const index = steps.findIndex((step) => step.id === event.step.id);
              const toolSteps = index === -1 ? [...steps, event.step] : steps.map((step, i) => (i === index ? event.step : step));
              return { ...message, toolSteps };
            }),
          };
        });
        if (event.step.status === "approval") setPendingApproval(event.step.id);
        if (event.step.status === "done" || event.step.status === "denied") {
          setPendingApproval((current) => (current === event.step.id ? null : current));
        }
      },
      onDone: (event) => {
        setStreamingId((id) => (id === event.messageId ? null : id));
        setChat((existing) => {
          if (!existing || existing.id !== event.chatId) return existing;
          const painted = paint(existing, event.messageId, buffers.current) ?? existing;
          return {
            ...painted,
            title: event.title || painted.title,
            contextUsed: event.contextUsed,
            contextLimit: event.contextLimit,
            messages: painted.messages.map((message) =>
              message.id === event.messageId && event.error && !message.content
                ? { ...message, content: event.error }
                : message,
            ),
          };
        });
        setChats((list) =>
          list.map((item) =>
            item.id === event.chatId
              ? { ...item, title: event.title || item.title, contextUsed: event.contextUsed, contextLimit: event.contextLimit }
              : item,
          ),
        );
        if (event.error) setError(event.error);
      },
    }).then((unlisten) => {
      stop = unlisten;
    });
    return () => stop();
  }, []);

  useEffect(() => {
    void (async () => {
      const [list, catalog] = await Promise.all([api.listChats(), api.listModels()]);
      setChats(list);
      setModels(catalog);
      if (list[0]) setChat(await api.getChat(list[0].id));
    })().catch((reason: unknown) => setError(messageOf(reason)));
  }, []);

  async function openChat(id: string) {
    setError(null);
    setChat(await api.getChat(id));
  }

  async function newChat() {
    const provider = chat?.provider ?? "local";
    const modelId = chat?.modelId || models.find((model) => model.provider === "local")?.id || "";
    const created = await api.createChat(provider, modelId);
    setChats((list) => [created, ...list.filter((item) => item.id !== created.id)]);
    setChat(created);
    setDraft("");
    setPending([]);
  }

  async function removeChat(id: string) {
    if (!window.confirm("Delete this chat?")) return;
    await api.deleteChat(id);
    const list = chats.filter((item) => item.id !== id);
    setChats(list);
    if (chatId === id) setChat(list[0] ? await api.getChat(list[0].id) : null);
  }

  async function chooseModel(model: CatalogModel) {
    if (!chat) {
      const created = await api.createChat(model.provider, model.id);
      setChat(created);
      setChats((list) => [created, ...list]);
      return;
    }
    const next = await api.updateChatModel(chat.id, model.provider, model.id);
    setChat(next);
    setChats((list) => list.map((item) => (item.id === next.id ? next : item)));
  }

  async function saveSettings(next: Chat["settings"]) {
    if (!chat) return;
    setChat({ ...chat, settings: next });
    await api.updateChatSettings(chat.id, next);
  }

  async function send() {
    if (!chat || streamingId) return;
    setError(null);
    try {
      const started = await api.sendMessage(
        chat.id,
        draft,
        pending.map((file) => file.path),
      );
      setDraft("");
      setPending([]);
      setStreamingId(started.assistant.id);
      setChat((current) => {
        if (!current) return current;
        const withMessages: Chat = {
          ...current,
          title: started.title,
          messages: [...current.messages, started.user, started.assistant],
        };
        return paint(withMessages, started.assistant.id, buffers.current);
      });
      setChats((list) =>
        list.map((item) => (item.id === chat.id ? { ...item, title: started.title } : item)),
      );
    } catch (reason) {
      setStreamingId(null);
      setError(messageOf(reason));
    }
  }

  const used = (chat?.contextUsed ?? 0) + estimateTokens(draft);
  const limit = chat?.contextLimit || 8192;

  return (
    <div className="chat-shell">
      <aside className="sidebar">
        <button type="button" className="new-chat" onClick={() => void newChat()}>
          New chat
        </button>
        <div className="chat-list">
          {chats.map((item) => (
            <div key={item.id} className={item.id === chatId ? "chat-row active" : "chat-row"}>
              <button type="button" className="chat-open" onClick={() => void openChat(item.id)}>
                <strong>{item.title}</strong>
                <span>{item.modelId || "No model"}</span>
              </button>
              <button type="button" className="icon-btn" aria-label={`Delete ${item.title}`} onClick={() => void removeChat(item.id)}>
                ×
              </button>
            </div>
          ))}
        </div>
      </aside>
      <section className="thread">
        <div className="messages">
          {chat && chat.messages.length > 0 ? (
            chat.messages.map((message: ChatMessage) => (
              <MessageView key={message.id} message={message} streaming={message.id === streamingId} />
            ))
          ) : (
            <div className="empty-thread">
              <h1>Chat</h1>
              <p className="muted">Pick a model, then send a message. Threads stay on this computer.</p>
            </div>
          )}
        </div>
        <form
          className="composer"
          onSubmit={(event) => {
            event.preventDefault();
            void send();
          }}
        >
          <div className="composer-tools">
            <ModelPicker
              models={models}
              selectedId={chat?.modelId ?? ""}
              disabled={Boolean(streamingId)}
              onSelect={(model) => void chooseModel(model)}
              onModels={setModels}
            />
            <span className="meter" title="Approximate context used">
              {formatContext(used, limit)}
            </span>
            <span className="spacer" />
            <button
              type="button"
              className={chat?.settings.think ? "chip on" : "chip"}
              disabled={!chat}
              onClick={() => chat && void saveSettings({ ...chat.settings, think: !chat.settings.think })}
            >
              Think
            </button>
            {chat ? (
              <ToolsMenu
                settings={chat.settings}
                disabled={Boolean(streamingId)}
                onChange={(settings) => void saveSettings(settings)}
              />
            ) : null}
            <button
              type="button"
              className="chip"
              disabled={!chat}
              title="Used when Think is on"
              onClick={() =>
                chat && void saveSettings({ ...chat.settings, effort: cycleEffort(chat.settings.effort) })
              }
            >
              Effort: {effortLabel(chat?.settings.effort ?? "default")}
            </button>
            <button type="button" className="chip" disabled title="Coming later">
              Code
            </button>
            <button type="button" className="chip" disabled title="Coming later">
              Voice
            </button>
          </div>
          {pending.length > 0 ? (
            <div className="chip-row">
              {pending.map((file) => (
                <span key={file.path} className="pending-file">
                  {file.name}
                  <button type="button" onClick={() => setPending((files) => files.filter((item) => item.path !== file.path))}>
                    ×
                  </button>
                </span>
              ))}
            </div>
          ) : null}
          {sampling && chat ? (
            <SamplingPopover
              settings={chat.settings}
              onChange={(settings) => void saveSettings(settings)}
              onClose={() => setSampling(false)}
            />
          ) : null}
          <div className="composer-row">
            <button type="button" className="chip" onClick={() => void attachImage(fileRef, setPending, setError)}>
              Attach
            </button>
            <input
              ref={fileRef}
              type="file"
              accept="image/png,image/jpeg,image/gif,image/webp"
              hidden
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (!file) return;
                const url = URL.createObjectURL(file);
                setPending((files) => [...files, { name: file.name, path: url, mime: file.type || "image/png" }]);
                event.target.value = "";
              }}
            />
            <textarea
              aria-label="Message"
              value={draft}
              placeholder="Message"
              rows={2}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  void send();
                }
              }}
            />
            <button type="button" className="chip" disabled={!chat} onClick={() => setSampling((value) => !value)}>
              Sampling
            </button>
            {streamingId ? (
              <button type="button" className="send" onClick={() => chat && void api.stopGeneration(chat.id)}>
                Stop
              </button>
            ) : (
              <button type="submit" className="send">
                Send
              </button>
            )}
          </div>
          {pendingApproval ? (
            <div className="choice-row">
              <span className="hint">Allow this action?</span>
              <button type="button" className="primary" onClick={() => void api.resolveToolApproval(pendingApproval, true)}>
                Allow
              </button>
              <button type="button" className="secondary" onClick={() => void api.resolveToolApproval(pendingApproval, false)}>
                Deny
              </button>
            </div>
          ) : null}
          {error ? <p className="banner">{error}</p> : null}
        </form>
      </section>
    </div>
  );
}

function paint(
  chat: Chat | null,
  messageId: string,
  buffers: Map<string, { text: string; thinking: string }>,
): Chat | null {
  if (!chat) return chat;
  const buffer = buffers.get(messageId);
  if (!buffer) return chat;
  return {
    ...chat,
    messages: chat.messages.map((message) =>
      message.id === messageId
        ? { ...message, content: buffer.text, thinking: buffer.thinking || undefined }
        : message,
    ),
  };
}

async function attachImage(
  fileRef: RefObject<HTMLInputElement | null>,
  setPending: Dispatch<SetStateAction<Attachment[]>>,
  setError: (value: string | null) => void,
) {
  if (!isTauri()) {
    fileRef.current?.click();
    return;
  }
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
    });
    if (typeof picked !== "string") return;
    const staged = await api.stageAttachment(picked);
    setPending((files) => [...files, staged]);
  } catch (reason) {
    setError(messageOf(reason));
  }
}

function messageOf(reason: unknown): string {
  return reason instanceof Error ? reason.message : "Something went wrong.";
}
