import type { AgentEntry, HostInfo, HostListEntry } from "./types";
import { cn, Dot, Stat } from "./ui";

export default function Home({
  hosts,
  activeHost,
  setActiveHost,
  agents,
  host,
  error,
  onOpenFleet,
}: {
  hosts: HostListEntry[];
  activeHost: string;
  setActiveHost: (h: string) => void;
  agents: AgentEntry[];
  host: HostInfo | null;
  error: string | null;
  onOpenFleet: () => void;
}) {
  const activeCount = agents.filter((a) => a.status === "active").length;
  const reachable = !error && host !== null;

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6 p-4 sm:p-6">
      {/* hero */}
      <header className="flex flex-col items-center gap-3 pt-6 text-center">
        <div className="grid size-16 place-items-center rounded-3xl bg-primary/15 text-3xl ring-1 ring-primary/25">
          🎛️
        </div>
        <div>
          <h1 className="text-xl font-semibold">BWOC Control Center</h1>
          <p className="text-sm text-muted-foreground">
            Your agent fleet, on every device.
          </p>
        </div>
      </header>

      {/* active host summary */}
      <section className="grid grid-cols-3 gap-3">
        <Stat label="Agents" value={agents.length} />
        <Stat label="Active" value={activeCount} accent="text-help-resolved" />
        <Stat
          label="Host"
          value={
            <span className="inline-flex items-center gap-2 text-lg">
              <Dot
                className={cn(reachable ? "bg-help-resolved" : "bg-help-open")}
              />
              <span className="truncate text-sm font-medium normal-case">
                {activeHost}
              </span>
            </span>
          }
          hint={reachable ? "reachable" : "connecting…"}
        />
      </section>

      {/* hosts launcher */}
      <section>
        <p className="mb-2 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          Hosts
        </p>
        <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
          {hosts.map((h) => (
            <button
              key={h.name}
              onClick={() => {
                setActiveHost(h.name);
                onOpenFleet();
              }}
              className={cn(
                "flex items-center gap-3 rounded-xl border bg-card px-4 py-3 text-left transition-colors hover:bg-muted/40",
                h.name === activeHost ? "border-primary/50" : "border-border",
              )}
            >
              <span className="text-xl">
                {h.name === "local" ? "💻" : "🛰"}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">{h.name}</span>
                <span className="block truncate text-xs text-muted-foreground">
                  {h.url ? h.url.replace(/^https?:\/\//, "") : "this machine"}
                </span>
              </span>
              <span className="text-muted-foreground">›</span>
            </button>
          ))}
        </div>
      </section>

      {/* primary action */}
      <button
        onClick={onOpenFleet}
        className="rounded-xl bg-primary py-3 text-sm font-semibold text-primary-foreground"
      >
        View fleet →
      </button>
    </div>
  );
}
