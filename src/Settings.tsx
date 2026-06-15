import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import type { ControllerInfo, HostListEntry } from "./types";
import { Card, cn, Dot } from "./ui";

export default function Settings({
  hosts,
  activeHost,
  setActiveHost,
  controller,
  refreshHosts,
}: {
  hosts: HostListEntry[];
  activeHost: string;
  setActiveHost: (h: string) => void;
  controller: ControllerInfo | null;
  refreshHosts: () => Promise<void>;
}) {
  const [version, setVersion] = useState("");
  const [copied, setCopied] = useState(false);
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState("");
  const [newUrl, setNewUrl] = useState("");
  const [hostErr, setHostErr] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  async function addHost() {
    setHostErr(null);
    try {
      await invoke("add_host", { name: newName.trim(), url: newUrl.trim() });
      setNewName("");
      setNewUrl("");
      setAdding(false);
      await refreshHosts();
    } catch (e) {
      setHostErr(String(e));
    }
  }

  async function removeHost(name: string) {
    setHostErr(null);
    try {
      await invoke("remove_host", { name });
      await refreshHosts();
    } catch (e) {
      setHostErr(String(e));
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-5 p-4 sm:p-6 lg:px-8">
      <h1 className="text-lg font-semibold">Settings</h1>

      {/* hosts */}
      <section>
        <p className="mb-2 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          Hosts
        </p>
        <Card className="divide-y divide-border">
          {hosts.map((h) => (
            <div
              key={h.name}
              className="flex w-full items-center gap-3 px-4 py-3"
            >
              <button
                onClick={() => setActiveHost(h.name)}
                className="flex min-w-0 flex-1 items-center gap-3 text-left"
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
              {h.name !== "local" && (
                <button
                  onClick={() => removeHost(h.name)}
                  className="shrink-0 rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                  aria-label={`remove ${h.name}`}
                >
                  ✕
                </button>
              )}
            </div>
          ))}
        </Card>

        {/* add host */}
        {adding ? (
          <Card className="mt-2.5 flex flex-col gap-2 p-3">
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="name (e.g. anna)"
              autoCapitalize="none"
              autoCorrect="off"
              className="rounded-lg border border-input bg-card px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
            <input
              value={newUrl}
              onChange={(e) => setNewUrl(e.target.value)}
              placeholder="https://host.tailnet.ts.net:10130"
              autoCapitalize="none"
              autoCorrect="off"
              className="rounded-lg border border-input bg-card px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
            <div className="flex gap-2">
              <button
                onClick={() => {
                  setAdding(false);
                  setHostErr(null);
                }}
                className="flex-1 rounded-lg border border-border py-2 text-sm text-muted-foreground"
              >
                Cancel
              </button>
              <button
                onClick={addHost}
                disabled={!newName.trim() || !newUrl.trim()}
                className="flex-1 rounded-lg bg-primary py-2 text-sm font-medium text-primary-foreground disabled:opacity-40"
              >
                Add
              </button>
            </div>
          </Card>
        ) : (
          <button
            onClick={() => setAdding(true)}
            className="mt-2.5 w-full rounded-xl border border-dashed border-border py-2.5 text-sm text-muted-foreground hover:bg-muted/40"
          >
            + Add host
          </button>
        )}
        {hostErr && (
          <p className="mt-2 px-1 text-xs text-destructive">{hostErr}</p>
        )}
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
