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

function AgentRow({ a }: { a: AgentEntry }) {
  return (
    <tr className="border-b border-border/60 transition-colors last:border-0 hover:bg-muted/40">
      <td className="px-4 py-2.5">
        <span className="inline-flex items-center gap-2">
          <Dot className="bg-actor-agent/70" />
          <span className="font-mono text-[13px]">{a.id}</span>
        </span>
      </td>
      <td className="px-4 py-2.5">
        <span className="rounded-md bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
          {a.backend}
        </span>
      </td>
      <td className="px-4 py-2.5">
        <StatusPill status={a.status} />
      </td>
      <td className="px-4 py-2.5 text-xs text-muted-foreground tabular-nums">
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
  const [hosts, setHosts] = useState<HostListEntry[]>([
    { name: "local", url: null },
  ]);
  const [activeHost, setActiveHost] = useState("local");
  const [snap, setSnap] = useState<FleetSnapshot | null>(null);
  const [host, setHost] = useState<HostInfo | null>(null);
  const [controller, setController] = useState<ControllerInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [copied, setCopied] = useState(false);

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
    invoke<HostListEntry[]>("list_hosts").then(setHosts).catch(() => {});
    invoke<ControllerInfo>("controller_info").then(setController).catch(() => {});
  }, []);

  useEffect(() => {
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

  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto flex max-w-3xl flex-col gap-5 p-6">
        {/* ── header ─────────────────────────────────────── */}
        <header className="flex items-start justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="grid size-10 place-items-center rounded-xl bg-primary/15 text-lg ring-1 ring-primary/25">
              🎛️
            </div>
            <div className="min-w-0">
              <h1 className="text-lg font-semibold leading-tight">
                BWOC Control Center
              </h1>
              <p className="truncate text-xs text-muted-foreground">
                {host
                  ? `${host.hostname} · ${host.workspace}${host.version ? ` · ${host.version}` : ""}`
                  : "host unreachable"}
              </p>
            </div>
          </div>
          <div className="flex flex-col items-end gap-1.5">
            <select
              value={activeHost}
              onChange={(e) => setActiveHost(e.target.value)}
              className="rounded-lg border border-input bg-card px-3 py-1.5 text-sm shadow-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {hosts.map((h) => (
                <option key={h.name} value={h.name}>
                  {h.name === "local" ? "💻 local" : `🛰 ${h.name}`}
                </option>
              ))}
            </select>
            {updatedAt && (
              <p className="text-[11px] text-muted-foreground tabular-nums">
                updated {updatedAt.toLocaleTimeString()}
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
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-[11px] uppercase tracking-wide text-muted-foreground">
                <th className="px-4 py-2.5 font-medium">Agent</th>
                <th className="px-4 py-2.5 font-medium">Backend</th>
                <th className="px-4 py-2.5 font-medium">Status</th>
                <th className="px-4 py-2.5 font-medium">Incarnated</th>
              </tr>
            </thead>
            <tbody>
              {agents.map((a) => (
                <AgentRow key={a.id} a={a} />
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

        {/* ── footer ─────────────────────────────────────── */}
        {controller && (
          <footer className="mt-auto pt-2 text-center text-[11px] text-muted-foreground">
            controller <span className="font-mono">{controller.id}</span> ·{" "}
            <button
              className="font-mono underline-offset-2 hover:text-foreground hover:underline"
              title="copy public key"
              onClick={() => {
                navigator.clipboard.writeText(controller.public_key);
                setCopied(true);
                setTimeout(() => setCopied(false), 1200);
              }}
            >
              {copied ? "copied ✓" : `${controller.public_key.slice(0, 16)}…`}
            </button>
          </footer>
        )}
      </div>
    </main>
  );
}
