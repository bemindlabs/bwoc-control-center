import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { cn, Dot } from "./ui";

type Msg = { role: "user" | "agent"; text: string };

export default function Chat({
  host,
  agent,
  onBack,
}: {
  host: string;
  agent: string;
  onBack: () => void;
}) {
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [msgs, sending]);

  async function send() {
    const text = input.trim();
    if (!text || sending) return;
    // One-shot per turn, but pass the running transcript so the reply has
    // conversational context (bwoc run spawns a fresh agent each time).
    const history = [...msgs, { role: "user" as const, text }];
    setMsgs(history);
    setInput("");
    setSending(true);
    setError(null);
    const transcript = history
      .map((m) => `${m.role === "user" ? "User" : "You"}: ${m.text}`)
      .join("\n");
    const prompt =
      msgs.length > 0
        ? `Conversation so far:\n${transcript}\n\nReply to the last User message.`
        : text;
    try {
      const reply = await invoke<string>("chat_send", { host, agent, prompt });
      setMsgs((m) => [...m, { role: "agent", text: reply || "(no output)" }]);
    } catch (e) {
      setError(String(e));
    } finally {
      setSending(false);
    }
  }

  return (
    <main className="flex h-[100dvh] flex-col overflow-x-hidden bg-background text-foreground">
      {/* header — padding-top clears the notch / status bar */}
      <header
        className="flex shrink-0 items-center gap-3 border-b border-border px-4 py-3"
        style={{ paddingTop: "calc(env(safe-area-inset-top) + 0.5rem)" }}
      >
        <button
          onClick={onBack}
          className="rounded-lg px-2 py-1 text-sm text-muted-foreground hover:bg-muted hover:text-foreground"
        >
          ‹ Back
        </button>
        <span className="flex items-center gap-2 font-medium">
          <Dot className="bg-actor-agent/70" />
          <span className="font-mono text-sm">{agent}</span>
        </span>
        <span className="ml-auto text-xs text-muted-foreground">
          {host === "local" ? "💻 local" : `🛰 ${host}`}
        </span>
      </header>

      {/* messages */}
      <div className="flex-1 space-y-3 overflow-y-auto p-4">
        {msgs.length === 0 && !sending && (
          <p className="mt-10 text-center text-sm text-muted-foreground">
            Message {agent} to start the conversation.
          </p>
        )}
        {msgs.map((m, i) => (
          <div
            key={i}
            className={cn(
              "max-w-[85%] whitespace-pre-wrap break-words rounded-2xl px-3.5 py-2 text-sm",
              m.role === "user"
                ? "ml-auto bg-primary text-primary-foreground"
                : "mr-auto border border-border bg-card",
            )}
          >
            {m.text}
          </div>
        ))}
        {sending && (
          <div className="mr-auto flex items-center gap-1.5 rounded-2xl border border-border bg-card px-3.5 py-2.5 text-sm text-muted-foreground">
            <span className="size-1.5 animate-pulse rounded-full bg-muted-foreground" />
            thinking…
          </div>
        )}
        {error && (
          <div className="mr-auto max-w-[85%] break-words rounded-xl border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}
        <div ref={endRef} />
      </div>

      {/* input — padding-bottom clears the home indicator */}
      <div
        className="flex shrink-0 items-end gap-2 border-t border-border p-3"
        style={{ paddingBottom: "calc(env(safe-area-inset-bottom) + 0.75rem)" }}
      >
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
          rows={1}
          placeholder={`Message ${agent}…`}
          className="max-h-32 min-h-[2.5rem] flex-1 resize-none rounded-xl border border-input bg-card px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
        />
        <button
          onClick={send}
          disabled={sending || !input.trim()}
          className="h-10 shrink-0 rounded-xl bg-primary px-4 text-sm font-medium text-primary-foreground disabled:opacity-40"
        >
          Send
        </button>
      </div>
    </main>
  );
}
