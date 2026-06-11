//! BWOC Control Center — Tauri core.
//!
//! Commands resolve a `Transport` per call (DESIGN.md §2): host=None/"local"
//! shells `bwoc --json` on this machine; any other host name maps through
//! hosts.toml to a bwocd reached with ed25519-signed requests. The frontend
//! is transport-blind.

mod config;
mod transport;

use serde::Serialize;
use transport::Transport;

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
    version: Option<String>,
}

#[derive(Serialize)]
pub struct HostListEntry {
    name: String,
    url: Option<String>,
}

#[derive(Serialize)]
pub struct ControllerInfo {
    id: String,
    public_key: String,
}

/// Full fleet view: registry + the seven Aparihāniya health signals.
#[tauri::command]
async fn fleet_snapshot(
    host: Option<String>,
    workspace: Option<String>,
) -> Result<FleetSnapshot, String> {
    match Transport::resolve(host, workspace)? {
        Transport::Local(t) => {
            // `bwoc list --json` returns `{"agents": [...]}` — unwrap.
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
        Transport::Remote(r) => {
            let mut v = r.get_json("/fleet").await?;
            Ok(FleetSnapshot {
                workspace: v["workspace"].as_str().unwrap_or("").to_string(),
                agents: v["agents"].take(),
                health: v["health"].take(),
            })
        }
    }
}

/// Read one agent's inbox.
#[tauri::command]
async fn agent_inbox(
    host: Option<String>,
    workspace: Option<String>,
    agent: String,
) -> Result<serde_json::Value, String> {
    if !agent
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("invalid agent id".into());
    }
    match Transport::resolve(host, workspace)? {
        Transport::Local(t) => t.bwoc_json(&["inbox", &agent]).await,
        Transport::Remote(r) => r.get_json(&format!("/agents/{agent}/inbox")).await,
    }
}

/// Self-describe one host (local analogue of bwocd's `/hosts`).
#[tauri::command]
async fn host_info(host: Option<String>, workspace: Option<String>) -> Result<HostInfo, String> {
    match Transport::resolve(host, workspace)? {
        Transport::Local(t) => Ok(HostInfo {
            hostname: local_hostname(),
            workspace: t.workspace().to_string(),
            version: t.bwoc_version().await,
        }),
        Transport::Remote(r) => {
            let v = r.get_json("/hosts").await?;
            Ok(HostInfo {
                hostname: v["hostname"].as_str().unwrap_or("?").to_string(),
                workspace: v["workspace"].as_str().unwrap_or("").to_string(),
                version: v["bwocd_version"].as_str().map(|s| format!("bwocd {s}")),
            })
        }
    }
}

/// Hosts the UI can switch between. Desktop lists "local" first (LocalShell
/// against this machine's workspace); mobile omits it — there is no `bwoc` CLI
/// or workspace in the app sandbox, so only remote hosts make sense.
#[tauri::command]
fn list_hosts() -> Result<Vec<HostListEntry>, String> {
    let mut out = Vec::new();
    #[cfg(not(mobile))]
    out.push(HostListEntry {
        name: "local".into(),
        url: None,
    });
    for h in config::load_hosts()? {
        out.push(HostListEntry {
            name: h.name,
            url: Some(h.url),
        });
    }
    Ok(out)
}

/// This install's controller identity — shown in the UI so the public key can
/// be registered in a bwocd's controllers.toml (QR enrollment supersedes this).
#[tauri::command]
fn controller_info() -> Result<ControllerInfo, String> {
    let c = config::load_or_create_controller()?;
    Ok(ControllerInfo {
        public_key: cc_signing::public_key_hex(&c.seed_hex)?,
        id: c.id,
    })
}

fn local_hostname() -> String {
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
            host_info,
            list_hosts,
            controller_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
