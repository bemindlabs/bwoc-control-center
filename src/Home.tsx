import type { AgentEntry, HostInfo, HostListEntry } from "./types";
import { cn } from "./ui";

/** The app's dharmachakra-hub mark (echoes the app icon): an 8-spoke wheel
 *  (Noble Eightfold Path) whose spokes double as fleet links to 8 rim nodes,
 *  with a glowing teal core. Rendered in currentColor. */
export function HubMark({ className }: { className?: string }) {
  // 8-fold radial symmetry; angles from straight-down, stepping 45°.
  const R = 36; // rim radius
  const nodes = Array.from({ length: 8 }, (_, i) => {
    const a = (Math.PI / 4) * i + Math.PI / 2; // start pointing down
    return [50 + R * Math.cos(a), 50 + R * Math.sin(a)] as const;
  });
  return (
    <svg viewBox="0 0 100 100" className={className} fill="none">
      {/* faint wheel rim */}
      <circle cx="50" cy="50" r={R} stroke="currentColor" strokeOpacity="0.15" strokeWidth="1" />
      {/* spokes */}
      <g stroke="currentColor" strokeOpacity="0.4" strokeWidth="1.75" strokeLinecap="round">
        {nodes.map(([x, y], i) => (
          <line key={i} x1={50 + 14 * Math.cos((Math.PI / 4) * i + Math.PI / 2)} y1={50 + 14 * Math.sin((Math.PI / 4) * i + Math.PI / 2)} x2={x} y2={y} />
        ))}
      </g>
      {/* rim nodes */}
      {nodes.map(([x, y], i) => (
        <circle key={i} cx={x} cy={y} r="4.5" fill="currentColor" opacity="0.85" />
      ))}
      {/* luminous core */}
      <circle cx="50" cy="50" r="15" fill="currentColor" opacity="0.16" />
      <circle cx="50" cy="50" r="9.5" fill="currentColor" opacity="0.9" />
      <circle cx="50" cy="50" r="3" fill="currentColor" />
    </svg>
  );
}

function HomeStat({
  label,
  value,
  accent,
}: {
  label: string;
  value: React.ReactNode;
  accent?: string;
}) {
  return (
    <div className="rounded-2xl border border-border bg-card/70 px-3 py-3 text-center backdrop-blur">
      <p className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
        {label}
      </p>
      <p className={cn("mt-1 text-2xl font-semibold tabular-nums", accent)}>
        {value}
      </p>
    </div>
  );
}

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
    <div className="relative mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 pb-6 sm:px-6 lg:px-8">
      {/* ── hero ──────────────────────────────────────────── */}
      <header className="relative flex flex-col items-center gap-4 pt-12 text-center">
        {/* teal glow */}
        <div
          aria-hidden
          className="pointer-events-none absolute left-1/2 top-6 -z-10 size-56 -translate-x-1/2 rounded-full bg-primary/25 blur-[64px]"
        />
        <div className="grid size-24 place-items-center rounded-[1.75rem] bg-gradient-to-b from-card to-background text-primary shadow-xl ring-1 ring-primary/30">
          <HubMark className="size-14" />
        </div>
        <div className="space-y-1">
          <h1 className="bg-gradient-to-b from-foreground to-foreground/70 bg-clip-text text-2xl font-bold text-transparent">
            BWOC Control Center
          </h1>
          <p className="text-sm text-muted-foreground">
            Your agent fleet, on every device.
          </p>
        </div>
        <span
          className={cn(
            "inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs",
            reachable
              ? "border-help-resolved/30 bg-help-resolved/10 text-help-resolved"
              : "border-help-open/30 bg-help-open/10 text-help-open",
          )}
        >
          <span
            className={cn(
              "size-1.5 rounded-full",
              reachable ? "bg-help-resolved" : "bg-help-open animate-pulse",
            )}
          />
          {reachable
            ? `connected · ${activeHost}`
            : error
              ? `unreachable · ${activeHost}`
              : "connecting…"}
        </span>
      </header>

      {/* ── stats ─────────────────────────────────────────── */}
      <section className="grid grid-cols-3 gap-3">
        <HomeStat label="Agents" value={agents.length} />
        <HomeStat
          label="Active"
          value={activeCount}
          accent="text-help-resolved"
        />
        <HomeStat
          label="Hosts"
          value={hosts.filter((h) => h.name !== "local").length || hosts.length}
          accent="text-primary"
        />
      </section>

      {/* ── hosts ─────────────────────────────────────────── */}
      <section className="space-y-2.5">
        <p className="px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          Hosts
        </p>
        <div className="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
        {hosts.map((h) => {
          const active = h.name === activeHost;
          return (
            <button
              key={h.name}
              onClick={() => {
                setActiveHost(h.name);
                onOpenFleet();
              }}
              className={cn(
                "group flex w-full items-center gap-3.5 rounded-2xl border bg-card/70 px-4 py-3.5 text-left backdrop-blur transition-all",
                active
                  ? "border-primary/40 shadow-[0_0_0_1px_var(--color-primary)]/20 ring-1 ring-primary/20"
                  : "border-border hover:border-border/80 hover:bg-muted/40",
              )}
            >
              <span
                className={cn(
                  "grid size-10 shrink-0 place-items-center rounded-xl text-lg",
                  active
                    ? "bg-primary/15 text-primary ring-1 ring-primary/30"
                    : "bg-muted text-muted-foreground",
                )}
              >
                {h.name === "local" ? "💻" : "🛰"}
              </span>
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-2">
                  <span className="truncate text-sm font-semibold">
                    {h.name}
                  </span>
                  {active && reachable && (
                    <span className="size-1.5 rounded-full bg-help-resolved" />
                  )}
                </span>
                <span className="block truncate text-xs text-muted-foreground">
                  {h.url ? h.url.replace(/^https?:\/\//, "") : "this machine"}
                </span>
              </span>
              <span
                aria-hidden
                className="text-muted-foreground transition-transform group-active:translate-x-0.5"
              >
                ›
              </span>
            </button>
          );
        })}
        </div>
      </section>

      {/* ── primary action ────────────────────────────────── */}
      <button
        onClick={onOpenFleet}
        className="rounded-2xl bg-primary py-3.5 text-sm font-semibold text-primary-foreground shadow-lg shadow-primary/20 active:scale-[0.99]"
      >
        View fleet →
      </button>
    </div>
  );
}
