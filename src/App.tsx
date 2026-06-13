import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentEntry,
  ControllerInfo,
  FleetSnapshot,
  HostInfo,
  HostListEntry,
} from "./types";
import { Card, cn, Dot, Stat } from "./ui";
import Chat from "./Chat";
import Settings from "./Settings";

const POLL_MS = 15_000;

function statusColor(status: string): string {
  switch (status) {
    case "active":
      return "bg-help-resolved";
    case "stopped":
      return "bg-muted-foreground";
    default:
      return "bg-help-open";
  }
}

function StatusPill({ status }: { status: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-foreground/80">
      <Dot className={statusColor(status)} />
      {status}
    </span>
  );
}

function AgentRow({ a, onOpen }: { a: AgentEntry; onOpen: (id: string) => void }) {
  return (
    <tr
      onClick={() => onOpen(a.id)}
      className="cursor-pointer border-b border-border/60 transition-colors last:border-0 hover:bg-muted/40 active:bg-muted/60"
    >
      <td className="px-3 py-2.5 sm:px-4">
        <span className="flex items-center gap-2">
          <Dot className="shrink-0 bg-actor-agent/70" />
          <span className="whitespace-nowrap font-mono text-[13px]">{a.id}</span>
        </span>
      </td>
      <td className="px-3 py-2.5 sm:px-4">
        <span className="rounded-md bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
          {a.backend}
        </span>
      </td>
      <td className="px-3 py-2.5 sm:px-4">
        <StatusPill status={a.status} />
      </td>
      <td className="hidden px-4 py-2.5 text-xs text-muted-foreground tabular-nums sm:table-cell">
        {a.incarnated?.slice(0, 10)}
      </td>
    </tr>
  );
}

function healthColor(status: string): string {
  switch (status) {
    case "ok":
      return "bg-help-resolved";
    case "warn":
      return "bg-help-open";
    default:
      return "bg-muted-foreground";
  }
}

export default function App() {
  const [hosts, setHosts] = useState<HostListEntry[]>([]);
  // Empty until list_hosts resolves; we then default to its first entry —
  // "local" on desktop, the first remote host on mobile (no local there).
  const [activeHost, setActiveHost] = useState("");
  const [snap, setSnap] = useState<FleetSnapshot | null>(null);
  const [host, setHost] = useState<HostInfo | null>(null);
  const [controller, setController] = useState<ControllerInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [chatAgent, setChatAgent] = useState<string | null>(null);
  const [tab, setTab] = useState<"fleet" | "settings">("fleet");

  const refresh = useCallback(async (hostName: string) => {
    try {
      const s = await invoke<FleetSnapshot>("fleet_snapshot", { host: hostName });
      setSnap(s);
      setError(null);
      setUpdatedAt(new Date());
    } catch (e) {
      setError(String(e));
    }
    invoke<HostInfo>("host_info", { host: hostName })
      .then(setHost)
      .catch(() => setHost(null));
  }, []);

  useEffect(() => {
    invoke<HostListEntry[]>("list_hosts")
      .then((h) => {
        setHosts(h);
        // Default to the first host the backend offers (local on desktop,
        // a remote host on mobile).
        setActiveHost((cur) => cur || h[0]?.name || "");
      })
      .catch(() => {});
    invoke<ControllerInfo>("controller_info").then(setController).catch(() => {});
  }, []);

  useEffect(() => {
    if (!activeHost) return;
    setSnap(null);
    refresh(activeHost);
    const t = setInterval(() => refresh(activeHost), POLL_MS);
    return () => clearInterval(t);
  }, [activeHost, refresh]);

  const agents = snap?.agents ?? [];
  const health = snap?.health ?? [];
  const activeCount = agents.filter((a) => a.status === "active").length;
  const okCount = health.filter((h) => h.status === "ok").length;
  const loading = snap === null && !error;

  if (chatAgent) {
    return (
      <Chat
        host={activeHost}
        agent={chatAgent}
        onBack={() => setChatAgent(null)}
      />
    );
  }

  return (
    <main className="flex h-[100dvh] flex-col overflow-x-hidden bg-background text-foreground">
      {/* scrollable content; padding-top clears the notch / status bar */}
      <div
        className="flex-1 overflow-y-auto"
        style={{ paddingTop: "env(safe-area-inset-top)" }}
      >
        {tab === "settings" ? (
          <Settings
            hosts={hosts}
            activeHost={activeHost}
            setActiveHost={setActiveHost}
            controller={controller}
          />
        ) : (
          <div className="mx-auto flex max-w-3xl flex-col gap-5 p-4 sm:p-6">
            {/* ── header ─────────────────────────────────────── */}
            <header className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-primary/15 text-lg ring-1 ring-primary/25">
              🎛️
            </div>
            <div className="min-w-0">
              <h1 className="text-base font-semibold leading-tight sm:text-lg">
                BWOC Control Center
              </h1>
              <p className="truncate text-xs text-muted-foreground">
                {host
                  ? `${host.hostname}${host.version ? ` · ${host.version}` : ""}`
                  : "host unreachable"}
              </p>
            </div>
          </div>
          <div className="flex shrink-0 flex-col items-end gap-1.5">
            <select
              value={activeHost}
              onChange={(e) => setActiveHost(e.target.value)}
              className="rounded-lg border border-input bg-card px-2.5 py-1.5 text-sm shadow-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {hosts.map((h) => (
                <option key={h.name} value={h.name}>
                  {h.name === "local" ? "💻 local" : `🛰 ${h.name}`}
                </option>
              ))}
            </select>
            {updatedAt && (
              <p className="text-[11px] text-muted-foreground tabular-nums">
                {updatedAt.toLocaleTimeString()}
              </p>
            )}
          </div>
        </header>

        {/* ── stat row ───────────────────────────────────── */}
        <section className="grid grid-cols-3 gap-3">
          <Stat label="Agents" value={agents.length} />
          <Stat
            label="Active"
            value={activeCount}
            accent="text-help-resolved"
            hint={`${agents.length - activeCount} idle`}
          />
          <Stat
            label="Health"
            value={
              <span>
                {okCount}
                <span className="text-base text-muted-foreground">
                  /{health.length || "–"}
                </span>
              </span>
            }
            accent="text-primary"
            hint="signals ok"
          />
        </section>

        {/* ── error ──────────────────────────────────────── */}
        {error && (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        {/* ── fleet table ────────────────────────────────── */}
        <Card>
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <h2 className="text-sm font-medium">Fleet</h2>
            <span className="text-xs text-muted-foreground">
              {agents.length} agent{agents.length === 1 ? "" : "s"}
            </span>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-[11px] uppercase tracking-wide text-muted-foreground">
                  <th className="px-3 py-2.5 font-medium sm:px-4">Agent</th>
                  <th className="px-3 py-2.5 font-medium sm:px-4">Backend</th>
                  <th className="px-3 py-2.5 font-medium sm:px-4">Status</th>
                  <th className="hidden px-4 py-2.5 font-medium sm:table-cell">
                    Incarnated
                  </th>
                </tr>
              </thead>
              <tbody>
                {agents.map((a) => (
                  <AgentRow key={a.id} a={a} onOpen={setChatAgent} />
                ))}
                {agents.length === 0 && (
                  <tr>
                    <td
                      colSpan={4}
                      className="px-4 py-10 text-center text-sm text-muted-foreground"
                    >
                      {loading ? "loading fleet…" : "no agents"}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </Card>

        {/* ── health signals ─────────────────────────────── */}
        {health.length > 0 && (
          <section className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
            {health.map((h) => (
              <Card key={h.condition} className="px-3.5 py-3">
                <p className="flex items-center gap-2 text-sm font-medium">
                  <Dot className={cn(healthColor(h.status), "shrink-0")} />
                  {h.name}
                </p>
                <p className="mt-1 pl-4 text-xs text-muted-foreground">
                  {h.finding}
                </p>
              </Card>
            ))}
          </section>
        )}

          </div>
        )}
      </div>

      {/* ── bottom nav (clears the home indicator) ─────────── */}
      <nav
        className="flex shrink-0 border-t border-border bg-card"
        style={{ paddingBottom: "env(safe-area-inset-bottom)" }}
      >
        {(["fleet", "settings"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={cn(
              "flex flex-1 flex-col items-center gap-0.5 py-2.5 text-[11px] font-medium transition-colors",
              tab === t ? "text-primary" : "text-muted-foreground",
            )}
          >
            <span className="text-lg leading-none">
              {t === "fleet" ? "🛰" : "⚙️"}
            </span>
            {t === "fleet" ? "Fleet" : "Settings"}
          </button>
        ))}
      </nav>
    </main>
  );
}
