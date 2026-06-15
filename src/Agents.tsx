import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AgentEntry } from "./types";
import { Card, cn } from "./ui";

// Backends the host's `bwoc` understands (mirrors the daemon's allow-list).
const BACKENDS = [
  "claude",
  "codex",
  "agy",
  "kimi",
  "ollama",
  "openai",
  "copilot",
  "openrouter",
];

/** Agent lifecycle management — create (`bwoc new`), retire (`bwoc retire`),
 *  and a per-agent status peek (`bwoc status`). Mutations run against the
 *  active host (LocalShell or a remote bwocd, transport-blind). */
export default function Agents({
  agents,
  host,
  onChanged,
}: {
  agents: AgentEntry[];
  host: string;
  onChanged: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [statusOf, setStatusOf] = useState<{ id: string; text: string } | null>(null);

  // create form
  const [name, setName] = useState("");
  const [backend, setBackend] = useState("claude");
  const [role, setRole] = useState("");
  const [model, setModel] = useState("");
  const [scope, setScope] = useState("");

  async function create() {
    setBusy("create");
    setErr(null);
    try {
      await invoke("agent_create", {
        host,
        name: name.trim(),
        backend,
        role: role.trim(),
        primaryModel: model.trim(),
        scope: scope.trim() || null,
      });
      setName("");
      setRole("");
      setModel("");
      setScope("");
      setOpen(false);
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function retire(id: string) {
    if (!confirm(`Retire ${id}? This removes the agent and its files.`)) return;
    setBusy(id);
    setErr(null);
    try {
      await invoke("agent_retire", { host, agent: id });
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function status(id: string) {
    setBusy(id);
    setErr(null);
    try {
      const v = await invoke<unknown>("agent_status", { host, agent: id });
      setStatusOf({ id, text: JSON.stringify(v, null, 2) });
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  }

  const valid = name.trim() && role.trim() && model.trim();

  return (
    <section className="space-y-2.5">
      <div className="flex items-center justify-between px-1">
        <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          Manage agents
        </p>
        <button
          onClick={() => {
            setOpen((o) => !o);
            setErr(null);
          }}
          className="rounded-lg bg-primary px-3 py-1.5 text-xs font-semibold text-primary-foreground"
        >
          {open ? "Cancel" : "+ New agent"}
        </button>
      </div>

      {/* create form */}
      {open && (
        <Card className="flex flex-col gap-2 p-3">
          <div className="flex gap-2">
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="name (e.g. luban)"
              autoCapitalize="none"
              autoCorrect="off"
              className="flex-1 rounded-lg border border-input bg-card px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
            <select
              value={backend}
              onChange={(e) => setBackend(e.target.value)}
              className="rounded-lg border border-input bg-card px-2 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {BACKENDS.map((b) => (
                <option key={b} value={b}>
                  {b}
                </option>
              ))}
            </select>
          </div>
          <input
            value={role}
            onChange={(e) => setRole(e.target.value)}
            placeholder="role — one line: this agent does X"
            className="rounded-lg border border-input bg-card px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder='primary model (id, or "auto")'
            autoCapitalize="none"
            autoCorrect="off"
            className="rounded-lg border border-input bg-card px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
          <input
            value={scope}
            onChange={(e) => setScope(e.target.value)}
            placeholder="scope (optional) — persona one-liner"
            className="rounded-lg border border-input bg-card px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
          <button
            onClick={create}
            disabled={!valid || busy === "create"}
            className="rounded-lg bg-primary py-2 text-sm font-semibold text-primary-foreground disabled:opacity-40"
          >
            {busy === "create" ? "incarnating…" : "Create agent"}
          </button>
        </Card>
      )}

      {err && <p className="px-1 text-xs text-destructive">{err}</p>}

      {/* manage list — status + retire per agent */}
      <Card className="divide-y divide-border">
        {agents.length === 0 && (
          <p className="px-4 py-6 text-center text-sm text-muted-foreground">no agents</p>
        )}
        {agents.map((a) => (
          <div key={a.id} className="flex items-center gap-2 px-3 py-2">
            <span className="min-w-0 flex-1">
              <span className="block truncate font-mono text-[13px]">{a.id}</span>
              <span className="text-xs text-muted-foreground">{a.backend}</span>
            </span>
            <button
              onClick={() => status(a.id)}
              disabled={busy === a.id}
              className="rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-muted/40"
            >
              status
            </button>
            <button
              onClick={() => retire(a.id)}
              disabled={busy === a.id}
              className="rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
              aria-label={`retire ${a.id}`}
            >
              ✕
            </button>
          </div>
        ))}
      </Card>

      {/* status peek */}
      {statusOf && (
        <Card className="p-3">
          <div className="mb-1 flex items-center justify-between">
            <span className="font-mono text-xs">{statusOf.id}</span>
            <button
              onClick={() => setStatusOf(null)}
              className="text-xs text-muted-foreground hover:text-foreground"
            >
              close
            </button>
          </div>
          <pre className={cn("overflow-x-auto rounded-lg bg-muted/40 p-2 text-[11px] leading-snug")}>
            {statusOf.text}
          </pre>
        </Card>
      )}
    </section>
  );
}
