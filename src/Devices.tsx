// Devices — control & settings for BWOC physical devices.
//  - bwoc-ball : ESP32-S3 XiaoZhi round-LCD ball, embodied as busaba.
//  - sc01plus  : WT32-SC01 Plus fleet panel, embodied as Anna (Mac-side agent).
// Controls route through the Rust `device_*` IPC commands (status read +
// component control). USB Wi-Fi provisioning reuses the ported `usb_*` commands.
// The UI is capability-aware: each device shows only the controls its firmware
// understands over the shared 115200 USB console. Controls that need a channel
// the firmware lacks degrade gracefully (the note shows "pending").
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Stat, Dot, cn } from "./ui";

type DeviceId = "bwoc-ball" | "sc01plus";

const DEVICES: { id: DeviceId; name: string; chip: string; desc: string }[] = [
  {
    id: "bwoc-ball",
    name: "bwoc·ball",
    chip: "🟢",
    desc: "ESP32-S3 N16R8 · GC9A01 round LCD · busaba",
  },
  {
    id: "sc01plus",
    name: "SC01 Plus",
    chip: "🪷",
    desc: "ESP32-S3 · SC01 Plus 3.5\" · Anna fleet panel",
  },
];

type BallStatus = {
  online: boolean;
  ip?: string;
  battery?: number; // %
  net?: string; // wifi ssid / rssi
  volume?: number; // 0-100
  brightness?: number; // 0-100
  mode?: number; // display mode index 0-6
  emotion?: string;
  llm?: string; // active brain
  // sc01plus extras
  source?: string; // AUTO | USB | CLOUD
  cloud?: string; // off | "mqtt · <status>" | "pull · <status>"
  summary?: string; // last fleet snapshot summary
  stale?: boolean; // snapshot older than the firmware stale window
};

// bwoc-ball (XiaoZhi) display modes + its rich emotion set (network WS/MCP).
const BALL_MODES = ["face", "face+status", "text", "character", "clock", "rec", "info"];
const BALL_EMOTIONS = [
  "idle", "listening", "thinking", "speaking", "happy", "love",
  "coding", "debugging", "investigating", "writing", "brainstorm", "sleep",
];
// sc01plus FACE expressions understood by the firmware `face_set_expr()`.
const SC01_FACES = [
  "idle", "think", "talk", "busy", "coding", "scan",
  "happy", "love", "wink", "cool", "wow", "sad", "error", "sleep",
];

function Section({ title, children, right }: { title: string; children: React.ReactNode; right?: React.ReactNode }) {
  return (
    <Card className="p-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-sm font-semibold">{title}</h3>
        {right}
      </div>
      {children}
    </Card>
  );
}

function Slider({ label, value, onCommit }: { label: string; value: number; onCommit: (v: number) => void }) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  return (
    <label className="block">
      <div className="mb-1 flex justify-between text-xs text-muted-foreground">
        <span>{label}</span>
        <span className="tabular-nums">{v}%</span>
      </div>
      <input
        type="range" min={0} max={100} value={v}
        onChange={(e) => setV(Number(e.target.value))}
        onMouseUp={() => onCommit(v)}
        onTouchEnd={() => onCommit(v)}
        className="w-full accent-primary"
      />
    </label>
  );
}

export default function Devices() {
  const [id, setId] = useState<DeviceId>("bwoc-ball");
  const dev = DEVICES.find((d) => d.id === id)!;
  const isSc01 = id === "sc01plus";
  const [st, setSt] = useState<BallStatus>({ online: false });
  const [busy, setBusy] = useState<string | null>(null);
  const [note, setNote] = useState<string>("");
  const [conn, setConn] = useState<string>("");

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<BallStatus>("device_status", { id });
      setSt(s);
    } catch {
      setSt((p) => ({ ...p, online: false }));
    }
  }, [id]);

  // On selecting a device, open + keep its USB console so status reads online.
  // Opening resets the ESP32-S3 once (USB-CDC auto-reset). The bwoc-ball has no
  // such console and resolves to an explanatory message instead of connecting.
  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setInterval> | undefined;
    setConn("connecting…");
    setSt({ online: false });
    (async () => {
      try {
        const port = await invoke<string>("usb_autoconnect", { id });
        if (alive) setConn(`USB ${port}`);
      } catch (e) {
        if (alive) setConn(String(e));
      }
      if (!alive) return;
      refresh();
      timer = setInterval(refresh, 5000);
    })();
    return () => { alive = false; if (timer) clearInterval(timer); };
  }, [id, refresh]);

  const control = useCallback(async (component: string, value: unknown) => {
    setBusy(component);
    setNote("");
    try {
      await invoke("device_control", { id, component, value });
      setNote(`✓ ${component} → ${value}`);
      refresh();
    } catch (e) {
      setNote(`control channel pending: ${component} (${String(e)})`);
    } finally {
      setBusy(null);
    }
  }, [id, refresh]);

  return (
    <div className="mx-auto w-full max-w-4xl space-y-4 px-4 py-4 sm:px-6 lg:px-8">
      {/* ── device picker ───────────────────────────────── */}
      <div className="flex gap-1.5 rounded-xl border border-border bg-card p-1">
        {DEVICES.map((d) => (
          <button
            key={d.id}
            onClick={() => setId(d.id)}
            className={cn(
              "flex-1 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors",
              id === d.id ? "bg-primary text-primary-foreground" : "text-muted-foreground",
            )}
          >
            {d.chip} {d.name}
          </button>
        ))}
      </div>

      {/* ── identity / status ───────────────────────────── */}
      <Card className="p-4">
        <div className="flex items-center gap-3">
          <Dot className={st.online ? "bg-emerald-500" : "bg-muted-foreground/40"} />
          <div className="flex-1">
            <div className="font-mono text-sm font-semibold">{dev.name}</div>
            <div className="text-xs text-muted-foreground">
              {dev.desc} {st.online ? "· online" : "· offline"}
            </div>
            {conn && <div className="mt-0.5 text-[11px] text-muted-foreground/80">{conn}</div>}
          </div>
          <button onClick={refresh} className="rounded-md border border-border px-2 py-1 text-xs">
            refresh
          </button>
        </div>
        <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Stat label="IP" value={st.ip ?? "—"} />
          <Stat label="Net" value={st.net ?? "—"} />
          {isSc01 ? (
            <>
              <Stat label="Source" value={st.source ?? "—"} />
              <Stat label="Cloud" value={st.cloud ?? "—"} />
            </>
          ) : (
            <>
              <Stat label="Battery" value={st.battery != null ? `${st.battery}%` : "—"} />
              <Stat label="Brain" value={st.llm ?? "—"} />
            </>
          )}
        </div>
        {isSc01 && st.summary && (
          <div className="mt-3 flex items-center gap-2 text-xs">
            <span className="text-muted-foreground">fleet:</span>
            <span className="font-mono">{st.summary}</span>
            {st.stale && (
              <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-amber-500">stale</span>
            )}
          </div>
        )}
      </Card>

      {note && <p className="px-1 text-xs text-muted-foreground">{note}</p>}

      {/* ════════════ sc01plus (Anna fleet panel) ════════════ */}
      {isSc01 && (
        <>
          {/* face & identity ──────────────────────────────── */}
          <Section title="Face & Identity">
            <div className="mb-3 flex flex-wrap gap-1.5">
              {SC01_FACES.map((e) => (
                <button key={e} disabled={busy === "face"} onClick={() => control("face", e)}
                  className={cn("rounded-md border px-2 py-1 text-xs",
                    st.emotion === e ? "border-primary text-primary" : "border-border text-muted-foreground")}>
                  {e}
                </button>
              ))}
            </div>
            <Nameplate onSet={(v) => control("who", v)} />
            <SayLine onSay={(v) => control("say", v)} />
          </Section>

          {/* display (brightness) ─────────────────────────── */}
          <Section title="Display">
            <Slider label="Brightness" value={st.brightness ?? 80} onCommit={(v) => control("brightness", v)} />
          </Section>

          {/* source policy ────────────────────────────────── */}
          <Section title="Source policy" right={
            <span className="text-xs text-muted-foreground">USB-first arbitration</span>
          }>
            <div className="flex gap-1.5">
              {["auto", "usb", "cloud"].map((s) => (
                <button key={s} disabled={busy === "source"} onClick={() => control("source", s)}
                  className={cn("flex-1 rounded-md border px-3 py-1.5 text-xs uppercase",
                    st.source?.toLowerCase() === s ? "border-primary text-primary" : "border-border text-muted-foreground")}>
                  {s}
                </button>
              ))}
            </div>
            <p className="mt-2 text-xs text-muted-foreground">
              auto = cloud yields while USB pushes · usb = push-only · cloud = always apply cloud
            </p>
          </Section>

          {/* cloud (standalone, no host on the LAN) ───────── */}
          <Section title="Cloud — standalone ingest">
            <CloudConfig onApply={(v) => control("cloud", v)} busy={busy === "cloud"} />
          </Section>
        </>
      )}

      {/* ════════════ bwoc-ball (busaba) ════════════ */}
      {!isSc01 && (
        <>
          <Section title="Display & Interface">
            <div className="mb-3 flex flex-wrap gap-1.5">
              {BALL_MODES.map((m, i) => (
                <button key={m} onClick={() => control("mode", i)}
                  className={cn("rounded-md border px-2 py-1 text-xs",
                    st.mode === i ? "border-primary text-primary" : "border-border text-muted-foreground")}>
                  {i}:{m}
                </button>
              ))}
            </div>
            <Slider label="Brightness" value={st.brightness ?? 50} onCommit={(v) => control("brightness", v)} />
          </Section>

          <Section title="Audio" right={
            <button disabled={busy === "tts_test"} onClick={() => control("tts_test", "สวัสดีค่ะที่รัก บุษบาพร้อมแล้วนะคะ")}
              className="rounded-md border border-border px-2 py-1 text-xs">test voice</button>
          }>
            <Slider label="Volume" value={st.volume ?? 60} onCommit={(v) => control("volume", v)} />
          </Section>

          <Section title="Face & Emotion">
            <div className="flex flex-wrap gap-1.5">
              {BALL_EMOTIONS.map((e) => (
                <button key={e} onClick={() => control("emotion", e)}
                  className={cn("rounded-md border px-2 py-1 text-xs",
                    st.emotion === e ? "border-primary text-primary" : "border-border text-muted-foreground")}>
                  {e}
                </button>
              ))}
            </div>
          </Section>

          <Section title="Brain (LLM backend)">
            <div className="flex gap-1.5">
              <button onClick={() => control("llm", "busaba-ball")}
                className={cn("rounded-md border px-3 py-1.5 text-xs",
                  st.llm === "busaba-ball" ? "border-primary text-primary" : "border-border")}>
                Claude session (busaba-ball)
              </button>
              <button onClick={() => control("llm", "typhoon")}
                className={cn("rounded-md border px-3 py-1.5 text-xs",
                  st.llm === "typhoon" ? "border-primary text-primary" : "border-border")}>
                typhoon (local, fast)
              </button>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">
              Claude = full agent-busaba, ~10-20s/turn · typhoon = local, ~0.4s, no Mac/Claude dependency
            </p>
          </Section>
        </>
      )}

      {/* ── network / provisioning (USB) — shared ───────── */}
      <Section title="Network — Wi-Fi provisioning (USB)">
        <DeviceProvisioning />
      </Section>
    </div>
  );
}

// Set the face-page nameplate (sc01plus `WHO`). Defaults to the device agent.
function Nameplate({ onSet }: { onSet: (v: string) => void }) {
  const [name, setName] = useState("anna");
  return (
    <div className="mt-1 flex gap-2 text-xs">
      <input placeholder="nameplate (WHO)" value={name} onChange={(e) => setName(e.target.value)}
        className="flex-1 rounded-md border border-border bg-background px-2 py-1" />
      <button disabled={!name.trim()} onClick={() => onSet(name.trim())}
        className="rounded-md border border-border px-3 py-1">set name</button>
    </div>
  );
}

// Append an activity line to the face page (sc01plus `SAY`). The firmware
// auto-detects the mood from keywords in the text.
function SayLine({ onSay }: { onSay: (v: string) => void }) {
  const [text, setText] = useState("");
  const send = () => { if (text.trim()) { onSay(text.trim()); setText(""); } };
  return (
    <div className="mt-2 flex gap-2 text-xs">
      <input placeholder="say… (appends an activity line)" value={text}
        onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && send()}
        className="flex-1 rounded-md border border-border bg-background px-2 py-1" />
      <button disabled={!text.trim()} onClick={send}
        className="rounded-md border border-border px-3 py-1">say</button>
    </div>
  );
}

// Standalone cloud ingest config (sc01plus `CLOUD MQTT|PULL|OFF`). The full
// argument string is forwarded verbatim to the firmware CLOUD verb.
function CloudConfig({ onApply, busy }: { onApply: (v: string) => void; busy: boolean }) {
  const [tab, setTab] = useState<"mqtt" | "pull">("mqtt");
  const [broker, setBroker] = useState("");
  const [port, setPort] = useState("1883");
  const [topic, setTopic] = useState("bwoc/fleet/snapshot");
  const [url, setUrl] = useState("");
  const [interval, setInterval] = useState("15");

  return (
    <div className="space-y-2 text-xs">
      <div className="flex gap-1.5">
        {(["mqtt", "pull"] as const).map((t) => (
          <button key={t} onClick={() => setTab(t)}
            className={cn("rounded-md border px-3 py-1 uppercase",
              tab === t ? "border-primary text-primary" : "border-border text-muted-foreground")}>
            {t}
          </button>
        ))}
        <button disabled={busy} onClick={() => onApply("OFF")}
          className="ml-auto rounded-md border border-border px-3 py-1">disable</button>
      </div>

      {tab === "mqtt" ? (
        <div className="space-y-2">
          <div className="flex gap-2">
            <input placeholder="broker host" value={broker} onChange={(e) => setBroker(e.target.value)}
              className="flex-1 rounded-md border border-border bg-background px-2 py-1" />
            <input placeholder="port" value={port} onChange={(e) => setPort(e.target.value)}
              className="w-20 rounded-md border border-border bg-background px-2 py-1" />
          </div>
          <input placeholder="topic" value={topic} onChange={(e) => setTopic(e.target.value)}
            className="w-full rounded-md border border-border bg-background px-2 py-1" />
          <button disabled={busy || !broker.trim()}
            onClick={() => onApply(`MQTT ${broker.trim()} ${port.trim() || 1883} ${topic.trim()}`)}
            className="rounded-md border border-primary px-3 py-1.5 text-primary">subscribe broker</button>
        </div>
      ) : (
        <div className="space-y-2">
          <input placeholder="https://host/fleet.json" value={url} onChange={(e) => setUrl(e.target.value)}
            className="w-full rounded-md border border-border bg-background px-2 py-1" />
          <div className="flex gap-2">
            <input placeholder="interval (s)" value={interval} onChange={(e) => setInterval(e.target.value)}
              className="w-28 rounded-md border border-border bg-background px-2 py-1" />
            <button disabled={busy || !url.trim()}
              onClick={() => onApply(`PULL ${url.trim()} ${interval.trim() || 15}`)}
              className="rounded-md border border-primary px-3 py-1.5 text-primary">poll URL</button>
          </div>
        </div>
      )}
      <p className="text-muted-foreground">
        Needs a working STA Wi-Fi link. MQTT needs a producer publishing the snapshot to the same topic.
      </p>
    </div>
  );
}

// USB Wi-Fi provisioning — reuses the ported usb_* commands (works when the
// device is plugged into this Mac over USB-C).
function DeviceProvisioning() {
  const [ports, setPorts] = useState<string[]>([]);
  const [port, setPort] = useState<string>("");
  const [nets, setNets] = useState<string[]>([]);
  const [ssid, setSsid] = useState("");
  const [pass, setPass] = useState("");
  const [msg, setMsg] = useState("");

  const listPorts = useCallback(async () => {
    try { setPorts(await invoke<string[]>("usb_list_ports")); } catch (e) { setMsg(String(e)); }
  }, []);
  useEffect(() => { listPorts(); }, [listPorts]);

  return (
    <div className="space-y-2 text-xs">
      <div className="flex gap-2">
        <select value={port} onChange={(e) => setPort(e.target.value)}
          className="flex-1 rounded-md border border-border bg-background px-2 py-1">
          <option value="">{ports.length ? "select port…" : "no USB device"}</option>
          {ports.map((p) => <option key={p} value={p}>{p}</option>)}
        </select>
        <button onClick={listPorts} className="rounded-md border border-border px-2 py-1">rescan</button>
        <button disabled={!port} onClick={async () => {
          try { await invoke("usb_connect", { port }); setNets(await invoke<string[]>("usb_wifi_scan")); }
          catch (e) { setMsg(String(e)); }
        }} className="rounded-md border border-border px-2 py-1">connect + scan</button>
      </div>
      {nets.length > 0 && (
        <select value={ssid} onChange={(e) => setSsid(e.target.value)}
          className="w-full rounded-md border border-border bg-background px-2 py-1">
          <option value="">select network…</option>
          {nets.map((n) => <option key={n} value={n}>{n}</option>)}
        </select>
      )}
      <input placeholder="SSID" value={ssid} onChange={(e) => setSsid(e.target.value)}
        className="w-full rounded-md border border-border bg-background px-2 py-1" />
      <input placeholder="password" type="password" value={pass} onChange={(e) => setPass(e.target.value)}
        className="w-full rounded-md border border-border bg-background px-2 py-1" />
      <button disabled={!ssid} onClick={async () => {
        try { await invoke("usb_set_wifi", { ssid, pass }); setMsg("✓ Wi-Fi sent — device will reconnect"); }
        catch (e) { setMsg(String(e)); }
      }} className="rounded-md border border-primary px-3 py-1.5 text-primary">send Wi-Fi</button>
      {msg && <p className="text-muted-foreground">{msg}</p>}
    </div>
  );
}
