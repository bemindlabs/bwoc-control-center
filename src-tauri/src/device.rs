//! Physical-device control for the Devices tab — currently bwoc-ball (the
//! ESP32-S3 XiaoZhi round-LCD ball, embodied as busaba).
//!
//! Two channels, per the firmware:
//!  - **USB serial** (115200 8N1, newline console) — Wi-Fi provisioning, and a
//!    `WIFI STATUS` query we surface as online/ip. Fully wired here (ported from
//!    bwoc-devices-app, which mirrors `tools/bwoc-push.py`). Desktop only —
//!    `serialport` has no iOS/Android backend, so mobile gets stubs.
//!  - **Network WS/MCP** (port 81, JSON-RPC over WebSocket) — the firmware's
//!    real control surface (brightness/volume/status). NOT wired yet: it needs a
//!    device address (no mDNS discovery), a WS auth token, and firmware MCP tools
//!    for mode/emotion/tts/llm that don't exist yet. `device_control` therefore
//!    degrades gracefully (the UI shows "control channel pending").

use serde::Serialize;

/// Status the Devices tab renders. Fields beyond online/ip/net come from the
/// network WS/MCP channel (not wired yet) and stay `None` over USB.
#[derive(Serialize, Default)]
pub struct BallStatus {
    online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    battery: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    net: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    brightness: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    llm: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// macOS: real USB serial implementation.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
mod imp {
    use super::BallStatus;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Default)]
    pub struct DeviceState {
        /// The open device console port, kept across commands (reopening reboots
        /// the device). `None` until `usb_connect`.
        serial: Mutex<Option<Box<dyn serialport::SerialPort>>>,
    }

    /// Write `line` + '\n', looping until every byte is out.
    fn write_line(port: &mut Box<dyn serialport::SerialPort>, line: &str) -> Result<(), String> {
        let mut buf = line.as_bytes().to_vec();
        buf.push(b'\n');
        let mut off = 0;
        while off < buf.len() {
            match port.write(&buf[off..]) {
                Ok(0) => return Err("serial write returned 0".to_string()),
                Ok(n) => off += n,
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => return Err(format!("serial write: {e}")),
            }
        }
        let _ = port.flush();
        Ok(())
    }

    /// Read device lines until `stop` matches a yielded protocol line or
    /// `timeout` elapses. Only `<<`/`!!` lines yielded; `[tag]` debug dropped.
    fn read_lines<F: Fn(&str) -> bool>(
        port: &mut Box<dyn serialport::SerialPort>,
        timeout: Duration,
        stop: F,
    ) -> Vec<String> {
        let deadline = std::time::Instant::now() + timeout;
        let mut acc: Vec<u8> = Vec::new();
        let mut buf = [0u8; 4096];
        let mut out: Vec<String> = Vec::new();
        while std::time::Instant::now() < deadline {
            match port.read(&mut buf) {
                Ok(0) => {}
                Ok(n) => acc.extend_from_slice(&buf[..n]),
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
            while let Some(pos) = acc.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = acc.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&raw[..raw.len().saturating_sub(1)])
                    .trim_matches(|c| c == '\r' || c == '\n')
                    .to_string();
                if line.starts_with("<<") || line.starts_with("!!") {
                    let matched = stop(&line);
                    out.push(line);
                    if matched {
                        return out;
                    }
                }
            }
        }
        out
    }

    fn drain_serial(port: &mut Box<dyn serialport::SerialPort>) {
        let _ = read_lines(port, Duration::from_millis(120), |_| false);
    }

    fn usb_command(
        port: &mut Box<dyn serialport::SerialPort>,
        line: &str,
        want: &[&str],
        timeout: Duration,
    ) -> Result<Vec<String>, String> {
        drain_serial(port);
        write_line(port, line)?;
        let wants: Vec<String> = want.iter().map(|s| s.to_string()).collect();
        Ok(read_lines(port, timeout, |l| {
            wants.iter().any(|w| l.starts_with(w.as_str()))
        }))
    }

    /// List USB serial devices — real USB `cu.*` ports (macOS pairs cu.*/tty.*),
    /// returning the openable path. Falls back to every `cu.*`.
    #[tauri::command]
    pub async fn usb_list_ports() -> Result<Vec<String>, String> {
        tauri::async_runtime::spawn_blocking(|| {
            let ports = serialport::available_ports().map_err(|e| format!("list ports: {e}"))?;
            let mut out: Vec<String> = Vec::new();
            for p in &ports {
                if p.port_name.contains("/tty.") {
                    continue;
                }
                if matches!(p.port_type, serialport::SerialPortType::UsbPort(_)) {
                    out.push(p.port_name.clone());
                }
            }
            if out.is_empty() {
                for p in &ports {
                    if p.port_name.contains("/cu.") {
                        out.push(p.port_name.clone());
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| format!("join: {e}"))?
    }

    /// Open the console at 115200 8N1 and stash it. Opening resets the
    /// ESP32-S3, so wait ~2.5s for reboot then drain the boot banner.
    #[tauri::command]
    pub async fn usb_connect(
        port: String,
        state: tauri::State<'_, Arc<DeviceState>>,
    ) -> Result<(), String> {
        let port = port.trim().to_string();
        if port.is_empty() {
            return Err("serial port is empty".into());
        }
        let app = Arc::clone(state.inner());
        tauri::async_runtime::spawn_blocking(move || {
            let opened = serialport::new(&port, 115_200)
                .data_bits(serialport::DataBits::Eight)
                .parity(serialport::Parity::None)
                .stop_bits(serialport::StopBits::One)
                .flow_control(serialport::FlowControl::None)
                .timeout(Duration::from_millis(300))
                .open()
                .map_err(|e| format!("open {port}: {e}"))?;
            let mut guard = app.serial.lock().map_err(|_| "serial lock poisoned")?;
            *guard = Some(opened);
            if let Some(p) = guard.as_mut() {
                std::thread::sleep(Duration::from_millis(2500));
                let _ = read_lines(p, Duration::from_millis(400), |_| false);
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|e| format!("join: {e}"))?
    }

    /// Drop the stored port (closes it). Safe when nothing is open.
    #[tauri::command]
    pub async fn usb_disconnect(state: tauri::State<'_, Arc<DeviceState>>) -> Result<(), String> {
        let app = Arc::clone(state.inner());
        tauri::async_runtime::spawn_blocking(move || {
            if let Ok(mut guard) = app.serial.lock() {
                guard.take();
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|e| format!("join: {e}"))?
    }

    /// Scan Wi-Fi, wait, pull results. Returns SSIDs sorted by signal, de-duped.
    #[tauri::command]
    pub async fn usb_wifi_scan(
        state: tauri::State<'_, Arc<DeviceState>>,
    ) -> Result<Vec<String>, String> {
        let app = Arc::clone(state.inner());
        tauri::async_runtime::spawn_blocking(move || {
            let mut guard = app.serial.lock().map_err(|_| "serial lock poisoned")?;
            let port = guard.as_mut().ok_or("USB not connected")?;

            usb_command(
                port,
                "WIFI SCAN",
                &["<<OK", "<<ERR"],
                Duration::from_secs(2),
            )?;
            std::thread::sleep(Duration::from_millis(4500));
            let lines = usb_command(port, "WIFI SCANRESULT", &["<<END"], Duration::from_secs(5))?;

            let mut nets: Vec<(String, i32)> = lines
                .iter()
                .filter_map(|l| {
                    let body = l.strip_prefix("<<NET ")?.trim();
                    let mut parts = body.split('\t');
                    let ssid = parts.next()?.trim().to_string();
                    if ssid.is_empty() {
                        return None;
                    }
                    let rssi = parts
                        .next()
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(-100);
                    Some((ssid, rssi))
                })
                .collect();
            nets.sort_by_key(|(_, rssi)| -rssi);
            let mut seen = std::collections::HashSet::new();
            let ssids = nets
                .into_iter()
                .filter(|(ssid, _)| seen.insert(ssid.clone()))
                .map(|(ssid, _)| ssid)
                .collect();
            Ok::<Vec<String>, String>(ssids)
        })
        .await
        .map_err(|e| format!("join: {e}"))?
    }

    /// Run set-wifi (SSID → PASS → APPLY), poll WIFI STATUS up to ~18s until
    /// connected:<ip> or failed. Returns the final status payload.
    #[tauri::command]
    pub async fn usb_set_wifi(
        ssid: String,
        pass: String,
        state: tauri::State<'_, Arc<DeviceState>>,
    ) -> Result<String, String> {
        if ssid.trim().is_empty() {
            return Err("SSID is empty".into());
        }
        let app = Arc::clone(state.inner());
        tauri::async_runtime::spawn_blocking(move || {
            let mut guard = app.serial.lock().map_err(|_| "serial lock poisoned")?;
            let port = guard.as_mut().ok_or("USB not connected")?;

            usb_command(
                port,
                &format!("WIFI SSID {ssid}"),
                &["<<OK", "<<ERR"],
                Duration::from_secs(3),
            )?;
            usb_command(
                port,
                &format!("WIFI PASS {pass}"),
                &["<<OK", "<<ERR"],
                Duration::from_secs(3),
            )?;
            usb_command(
                port,
                "WIFI APPLY 1",
                &["<<OK", "<<ERR"],
                Duration::from_secs(3),
            )?;

            let mut last = String::new();
            for _ in 0..12 {
                std::thread::sleep(Duration::from_millis(1500));
                let lines = usb_command(port, "WIFI STATUS", &["<<WIFI"], Duration::from_secs(2))?;
                if let Some(payload) = lines
                    .iter()
                    .rev()
                    .find_map(|l| l.strip_prefix("<<WIFI ").map(|s| s.trim().to_string()))
                {
                    last = payload;
                    if last.starts_with("connected:") || last.starts_with("failed") {
                        break;
                    }
                }
            }
            Ok::<String, String>(last)
        })
        .await
        .map_err(|e| format!("join: {e}"))?
    }

    /// Read status. Over USB this reports online + ip from `WIFI STATUS`; richer
    /// fields need the network WS/MCP channel and stay absent. Offline when no
    /// port is open.
    #[tauri::command]
    pub async fn device_status(
        id: String,
        state: tauri::State<'_, Arc<DeviceState>>,
    ) -> Result<BallStatus, String> {
        if id != "bwoc-ball" {
            return Err(format!("unknown device '{id}'"));
        }
        let app = Arc::clone(state.inner());
        tauri::async_runtime::spawn_blocking(move || {
            let mut guard = app.serial.lock().map_err(|_| "serial lock poisoned")?;
            let Some(port) = guard.as_mut() else {
                return Ok(BallStatus::default()); // not tethered → offline
            };
            let lines = usb_command(port, "WIFI STATUS", &["<<WIFI"], Duration::from_secs(2))?;
            let payload = lines
                .iter()
                .rev()
                .find_map(|l| l.strip_prefix("<<WIFI ").map(|s| s.trim().to_string()))
                .unwrap_or_default();
            // idle | scanning | connecting:<ssid> | connected:<ip> | failed
            let mut st = BallStatus::default();
            if let Some(ip) = payload.strip_prefix("connected:") {
                st.online = true;
                st.ip = Some(ip.trim().to_string());
            } else if let Some(ssid) = payload.strip_prefix("connecting:") {
                st.net = Some(ssid.trim().to_string());
            }
            Ok::<BallStatus, String>(st)
        })
        .await
        .map_err(|e| format!("join: {e}"))?
    }

    /// Set a control. The firmware's control surface is network WS/MCP, not
    /// wired yet, so this returns a clear "pending" error the UI surfaces.
    #[tauri::command]
    pub async fn device_control(
        id: String,
        component: String,
        _value: serde_json::Value,
    ) -> Result<(), String> {
        if id != "bwoc-ball" {
            return Err(format!("unknown device '{id}'"));
        }
        Err(format!(
            "{component}: network control (Wi-Fi WS/MCP) not wired yet — only USB Wi-Fi provisioning is connected"
        ))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Non-macOS (iOS, Android, Linux/Windows CI): no serial backend. Stubs keep the
// IPC surface identical so the frontend degrades gracefully (Devices tab shows
// offline / "pending").
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(not(target_os = "macos"))]
mod imp {
    use super::BallStatus;
    use std::sync::Arc;

    #[derive(Default)]
    pub struct DeviceState;

    const NO_USB: &str = "USB serial is desktop-only — connect the device to a Mac to provision it";

    #[tauri::command]
    pub async fn usb_list_ports() -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    #[tauri::command]
    pub async fn usb_connect(
        _port: String,
        _state: tauri::State<'_, Arc<DeviceState>>,
    ) -> Result<(), String> {
        Err(NO_USB.into())
    }

    #[tauri::command]
    pub async fn usb_disconnect(_state: tauri::State<'_, Arc<DeviceState>>) -> Result<(), String> {
        Ok(())
    }

    #[tauri::command]
    pub async fn usb_wifi_scan(
        _state: tauri::State<'_, Arc<DeviceState>>,
    ) -> Result<Vec<String>, String> {
        Err(NO_USB.into())
    }

    #[tauri::command]
    pub async fn usb_set_wifi(
        _ssid: String,
        _pass: String,
        _state: tauri::State<'_, Arc<DeviceState>>,
    ) -> Result<String, String> {
        Err(NO_USB.into())
    }

    #[tauri::command]
    pub async fn device_status(
        id: String,
        _state: tauri::State<'_, Arc<DeviceState>>,
    ) -> Result<BallStatus, String> {
        if id != "bwoc-ball" {
            return Err(format!("unknown device '{id}'"));
        }
        Ok(BallStatus::default()) // offline — no USB on mobile
    }

    #[tauri::command]
    pub async fn device_control(
        id: String,
        component: String,
        _value: serde_json::Value,
    ) -> Result<(), String> {
        if id != "bwoc-ball" {
            return Err(format!("unknown device '{id}'"));
        }
        Err(format!(
            "{component}: device control not available on mobile"
        ))
    }
}

pub use imp::*;
