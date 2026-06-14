// Devices — control & settings for BWOC physical devices.
// P1: bwoc-ball (the ESP32-S3 XiaoZhi round-LCD ball, embodied as busaba).
// Controls route through the Rust `device_*` IPC commands (status read +
// component control). USB Wi-Fi provisioning reuses the ported `usb_*` commands.
// Component controls that need the live control channel degrade gracefully.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Stat, Dot, cn } from "./ui";

// One entry per physical device. P1 ships bwoc-ball only; the registry shape is
// ready for more (sc01-claw, esp32-cam, quadruped…).
type DeviceId = "bwoc-ball";

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
};

const MODES = ["face", "face+status", "text", "character", "clock", "rec", "info"];
const EMOTIONS = [
  "idle", "listening", "thinking", "speaking", "happy", "love",
  "coding", "debugging", "investigating", "writing", "brainstorm", "sleep",
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
  const [id] = useState<DeviceId>("bwoc-ball");
  const [st, setSt] = useState<BallStatus>({ online: false });
  const [busy, setBusy] = useState<string | null>(null);
  const [note, setNote] = useState<string>("");

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<BallStatus>("device_status", { id });
      setSt(s);
    } catch (e) {
      setSt((p) => ({ ...p, online: false }));
    }
  }, [id]);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  }, [refresh]);

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
    <div className="mx-auto max-w-2xl space-y-4 px-4 py-4">
      {/* ── identity / status ───────────────────────────── */}
      <Card className="p-4">
        <div className="flex items-center gap-3">
          <Dot className={st.online ? "bg-emerald-500" : "bg-muted-foreground/40"} />
          <div className="flex-1">
            <div className="font-mono text-sm font-semibold">{id}</div>
            <div className="text-xs text-muted-foreground">
              ESP32-S3 N16R8 · GC9A01 round LCD · busaba {st.online ? "· online" : "· offline"}
            </div>
          </div>
          <button onClick={refresh} className="rounded-md border border-border px-2 py-1 text-xs">
            refresh
          </button>
        </div>
        <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Stat label="IP" value={st.ip ?? "—"} />
          <Stat label="Battery" value={st.battery != null ? `${st.battery}%` : "—"} />
          <Stat label="Brain" value={st.llm ?? "—"} />
          <Stat label="Net" value={st.net ?? "—"} />
        </div>
      </Card>

      {note && <p className="px-1 text-xs text-muted-foreground">{note}</p>}

      {/* ── display / interface ─────────────────────────── */}
      <Section title="Display & Interface">
        <div className="mb-3 flex flex-wrap gap-1.5">
          {MODES.map((m, i) => (
            <button key={m} onClick={() => control("mode", i)}
              className={cn("rounded-md border px-2 py-1 text-xs",
                st.mode === i ? "border-primary text-primary" : "border-border text-muted-foreground")}>
              {i}:{m}
            </button>
          ))}
        </div>
        <Slider label="Brightness" value={st.brightness ?? 50} onCommit={(v) => control("brightness", v)} />
      </Section>

      {/* ── audio ───────────────────────────────────────── */}
      <Section title="Audio" right={
        <button disabled={busy === "tts_test"} onClick={() => control("tts_test", "สวัสดีค่ะที่รัก บุษบาพร้อมแล้วนะคะ")}
          className="rounded-md border border-border px-2 py-1 text-xs">test voice</button>
      }>
        <Slider label="Volume" value={st.volume ?? 60} onCommit={(v) => control("volume", v)} />
      </Section>

      {/* ── face / emotion ──────────────────────────────── */}
      <Section title="Face & Emotion">
        <div className="flex flex-wrap gap-1.5">
          {EMOTIONS.map((e) => (
            <button key={e} onClick={() => control("emotion", e)}
              className={cn("rounded-md border px-2 py-1 text-xs",
                st.emotion === e ? "border-primary text-primary" : "border-border text-muted-foreground")}>
              {e}
            </button>
          ))}
        </div>
      </Section>

      {/* ── brain / llm ─────────────────────────────────── */}
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

      {/* ── network / provisioning (USB) ────────────────── */}
      <Section title="Network — Wi-Fi provisioning (USB)">
        <DeviceProvisioning />
      </Section>
    </div>
  );
}

// USB Wi-Fi provisioning — reuses the ported usb_* commands (works when the ball
// is plugged into this Mac over USB-C).
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
