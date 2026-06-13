import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import type { ControllerInfo, HostListEntry } from "./types";
import { Card, cn, Dot } from "./ui";

export default function Settings({
  hosts,
  activeHost,
  setActiveHost,
  controller,
}: {
  hosts: HostListEntry[];
  activeHost: string;
  setActiveHost: (h: string) => void;
  controller: ControllerInfo | null;
}) {
  const [version, setVersion] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5 p-4 sm:p-6">
      <h1 className="text-lg font-semibold">Settings</h1>

      {/* hosts */}
      <section>
        <p className="mb-2 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          Hosts
        </p>
        <Card className="divide-y divide-border">
          {hosts.map((h) => (
            <button
              key={h.name}
              onClick={() => setActiveHost(h.name)}
              className="flex w-full items-center gap-3 px-4 py-3 text-left first:rounded-t-xl last:rounded-b-xl hover:bg-muted/40"
            >
              <span className="text-base">
                {h.name === "local" ? "💻" : "🛰"}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">{h.name}</span>
                {h.url && (
                  <span className="block truncate text-xs text-muted-foreground">
                    {h.url.replace(/^https?:\/\//, "")}
                  </span>
                )}
              </span>
              {h.name === activeHost && (
                <Dot className="shrink-0 bg-help-resolved" />
              )}
            </button>
          ))}
        </Card>
      </section>

      {/* controller identity */}
      {controller && (
        <section>
          <p className="mb-2 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            This device
          </p>
          <Card className="p-4">
            <p className="text-sm">
              controller <span className="font-mono">{controller.id}</span>
            </p>
            <button
              onClick={() => {
                navigator.clipboard?.writeText(controller.public_key);
                setCopied(true);
                setTimeout(() => setCopied(false), 1200);
              }}
              className={cn(
                "mt-2 block w-full break-all rounded-lg border border-border bg-muted/40 p-2.5 text-left font-mono text-[11px] leading-snug",
                copied ? "text-help-resolved" : "text-muted-foreground",
              )}
            >
              {copied ? "copied ✓" : controller.public_key}
            </button>
            <p className="mt-2 text-xs text-muted-foreground">
              Register this key on a host to connect, or let the app request
              access automatically (an operator approves it).
            </p>
          </Card>
        </section>
      )}

      {/* about */}
      <section>
        <p className="mb-2 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          About
        </p>
        <Card className="p-4 text-sm">
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">BWOC Control Center</span>
            <span className="font-mono tabular-nums">v{version || "—"}</span>
          </div>
        </Card>
      </section>
    </div>
  );
}
