//! BWOC Control Center — Tauri core.
//!
//! Phase 1 ships the **LocalShell** transport only: every command shells the
//! `bwoc` CLI with `--json` against a workspace on this machine (the same
//! pattern as bwoc-devices-app). The **RemoteHttp** transport (signed requests
//! to a `bwocd` over the tailnet) plugs into the same command surface later —
//! the frontend never knows which transport served it.

mod transport;

use serde::Serialize;
use transport::LocalShell;

#[derive(Serialize)]
pub struct FleetSnapshot {
    workspace: String,
    agents: serde_json::Value,
    health: serde_json::Value,
}

#[derive(Serialize)]
pub struct HostInfo {
    hostname: String,
    workspace: String,
    bwoc_version: Option<String>,
}

/// Full fleet view: registry + the seven Aparihāniya health signals.
#[tauri::command]
async fn fleet_snapshot(workspace: Option<String>) -> Result<FleetSnapshot, String> {
    let t = LocalShell::resolve(workspace)?;
    // `bwoc list --json` returns `{"agents": [...]}` — unwrap to the array.
    let agents = t.bwoc_json(&["list"]).await?["agents"].take();
    // `fleet health` is report-only; tolerate failure so a half-healthy
    // workspace still renders its agent list.
    let health = t
        .bwoc_json(&["fleet", "health"])
        .await
        .unwrap_or(serde_json::Value::Null);
    Ok(FleetSnapshot {
        workspace: t.workspace().to_string(),
        agents,
        health,
    })
}

/// Read one agent's inbox (most recent first, CLI default ordering).
#[tauri::command]
async fn agent_inbox(
    workspace: Option<String>,
    agent: String,
) -> Result<serde_json::Value, String> {
    let t = LocalShell::resolve(workspace)?;
    t.bwoc_json(&["inbox", &agent]).await
}

/// Self-describe this host (the LocalShell analogue of bwocd's `/hosts`).
#[tauri::command]
async fn host_info(workspace: Option<String>) -> Result<HostInfo, String> {
    let t = LocalShell::resolve(workspace)?;
    Ok(HostInfo {
        hostname: hostname(),
        workspace: t.workspace().to_string(),
        bwoc_version: t.bwoc_version().await,
    })
}

fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            fleet_snapshot,
            agent_inbox,
            host_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
