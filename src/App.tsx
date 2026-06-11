import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  ControllerInfo,
  FleetSnapshot,
  HostInfo,
  HostListEntry,
} from "./types";

const POLL_MS = 15_000;

function StatusDot({ status }: { status: string }) {
  const cls =
    status === "active"
      ? "bg-emerald-400"
      : status === "stopped"
        ? "bg-zinc-500"
        : "bg-amber-400";
  return <span className={`inline-block h-2.5 w-2.5 rounded-full ${cls}`} />;
}

export default function App() {
  const [hosts, setHosts] = useState<HostListEntry[]>([{ name: "local", url: null }]);
  const [activeHost, setActiveHost] = useState("local");
  const [snap, setSnap] = useState<FleetSnapshot | null>(null);
  const [host, setHost] = useState<HostInfo | null>(null);
  const [controller, setController] = useState<ControllerInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);

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
  const okCount = health.filter((h) => h.status === "ok").length;

  return (
    <main className="min-h-screen bg-zinc-950 text-zinc-100 p-6 font-sans flex flex-col">
      <header className="mb-6 flex items-baseline justify-between gap-4">
        <div className="min-w-0">
          <h1 className="text-xl font-bold">🎛️ BWOC Control Center</h1>
          <p className="truncate text-sm text-zinc-400">
            {host
              ? `${host.hostname} · ${host.workspace} · ${host.version ?? "?"}`
              : "host unreachable"}
          </p>
        </div>
        <div className="flex items-center gap-3">
          <select
            value={activeHost}
            onChange={(e) => setActiveHost(e.target.value)}
            className="rounded-lg border border-zinc-700 bg-zinc-900 px-3 py-1.5 text-sm"
          >
            {hosts.map((h) => (
              <option key={h.name} value={h.name}>
                {h.name === "local" ? "💻 local" : `🛰️ ${h.name}`}
              </option>
            ))}
          </select>
          <div className="text-right text-xs text-zinc-500">
            {updatedAt && <p>updated {updatedAt.toLocaleTimeString()}</p>}
            {health.length > 0 && (
              <p>
                health {okCount}/{health.length} ok
              </p>
            )}
          </div>
        </div>
      </header>

      {error && (
        <div className="mb-4 rounded-lg border border-red-800 bg-red-950/40 p-3 text-sm text-red-300">
          {error}
        </div>
      )}

      <section className="rounded-xl border border-zinc-800 bg-zinc-900/60">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-zinc-800 text-left text-xs uppercase tracking-wide text-zinc-500">
              <th className="px-4 py-3">Agent</th>
              <th className="px-4 py-3">Backend</th>
              <th className="px-4 py-3">Status</th>
              <th className="px-4 py-3">Incarnated</th>
            </tr>
          </thead>
          <tbody>
            {agents.map((a) => (
              <tr key={a.id} className="border-b border-zinc-800/60 last:border-0">
                <td className="px-4 py-2.5 font-medium">{a.id}</td>
                <td className="px-4 py-2.5 text-zinc-400">{a.backend}</td>
                <td className="px-4 py-2.5">
                  <span className="inline-flex items-center gap-2">
                    <StatusDot status={a.status} />
                    {a.status}
                  </span>
                </td>
                <td className="px-4 py-2.5 text-zinc-500">{a.incarnated}</td>
              </tr>
            ))}
            {agents.length === 0 && !error && (
              <tr>
                <td colSpan={4} className="px-4 py-8 text-center text-zinc-500">
                  loading fleet…
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </section>

      {health.length > 0 && (
        <section className="mt-4 grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {health.map((h) => (
            <div
              key={h.condition}
              className="rounded-lg border border-zinc-800 bg-zinc-900/60 p-3 text-xs"
            >
              <p className="font-medium">
                {h.status === "ok" ? "✅" : h.status === "warn" ? "⚠️" : "ℹ️"} {h.name}
              </p>
              <p className="mt-1 text-zinc-500">{h.finding}</p>
            </div>
          ))}
        </section>
      )}

      {controller && (
        <footer className="mt-auto pt-6 text-center text-[11px] text-zinc-600">
          controller <span className="font-mono">{controller.id}</span> · pubkey{" "}
          <button
            className="font-mono hover:text-zinc-400"
            title="click to copy"
            onClick={() => navigator.clipboard.writeText(controller.public_key)}
          >
            {controller.public_key.slice(0, 16)}…
          </button>{" "}
          — register in a host&apos;s controllers.toml to connect
        </footer>
      )}
    </main>
  );
}
