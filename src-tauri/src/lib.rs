//! BWOC Control Center — Tauri core.
//!
//! Commands resolve a `Transport` per call (DESIGN.md §2): host=None/"local"
//! shells `bwoc --json` on this machine; any other host name maps through
//! hosts.toml to a bwocd reached with ed25519-signed requests. The frontend
//! is transport-blind.

mod config;
mod device;
mod transport;

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use transport::Transport;

/// The app's writable per-platform config dir (Tauri resolves this correctly on
/// iOS — under Library/, which is writable — unlike a hardcoded `~/.config`).
fn cfg_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("no app config dir: {e}"))
}

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
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
) -> Result<FleetSnapshot, String> {
    match Transport::resolve(&cfg_dir(&app)?, host, workspace)? {
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
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    agent: String,
) -> Result<serde_json::Value, String> {
    if !valid_agent_id(&agent) {
        return Err("invalid agent id".into());
    }
    match Transport::resolve(&cfg_dir(&app)?, host, workspace)? {
        Transport::Local(t) => t.bwoc_json(&["inbox", &agent]).await,
        Transport::Remote(r) => r.get_json(&format!("/agents/{agent}/inbox")).await,
    }
}

/// Agent ids/names are kebab-case; reject anything that could smuggle a CLI
/// flag or path segment into the shell-out.
fn valid_agent_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Bare agent name for `bwoc new` — an id-shaped token capped at 48 chars so the
/// resulting `agent-<name>` id stays within the 64-char id limit (matches
/// bwocd's `valid_agent_name`).
fn valid_agent_name(name: &str) -> bool {
    name.len() <= 48 && valid_agent_id(name)
}

fn valid_backend(b: &str) -> bool {
    matches!(
        b,
        "claude" | "codex" | "agy" | "kimi" | "ollama" | "openai" | "copilot" | "openrouter"
    )
}

/// Create an agent — `bwoc new <name> --json …`. `name` is bare (no `agent-`
/// prefix); the agent lands at `agents/agent-<name>`. Long-ish (incarnation
/// writes the slot tree).
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command: flat args mirror the JS call
async fn agent_create(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    name: String,
    backend: String,
    role: String,
    primary_model: String,
    scope: Option<String>,
) -> Result<serde_json::Value, String> {
    let name = name.trim();
    if !valid_agent_name(name) {
        return Err("name must be lowercase letters/digits/'-' (max 48)".into());
    }
    if !valid_backend(&backend) {
        return Err("invalid backend".into());
    }
    if role.trim().is_empty() || primary_model.trim().is_empty() {
        return Err("role and primary model are required".into());
    }
    match Transport::resolve(&cfg_dir(&app)?, host, workspace)? {
        Transport::Local(t) => {
            let target = format!("agents/agent-{name}");
            // `bwoc new` requires the four verification gates at incarnation
            // (it can't prompt here). Default to the no-op `true`; refine later.
            // NB: bwoc_json already appends `--json` (+ `--workspace`); never
            // pass `--json` here or bwoc errors "used multiple times".
            let mut args = vec![
                "new",
                name,
                "--backend",
                &backend,
                "--role",
                role.trim(),
                "--primary-model",
                primary_model.trim(),
                "--lint-cmd",
                "true",
                "--format-cmd",
                "true",
                "--test-cmd",
                "true",
                "--build-cmd",
                "true",
                "--target",
                &target,
            ];
            let scope = scope.as_deref().map(str::trim).filter(|s| !s.is_empty());
            if let Some(s) = scope {
                args.push("--scope");
                args.push(s);
            }
            t.bwoc_json(&args).await
        }
        Transport::Remote(r) => {
            r.post_json(
                "/agents",
                serde_json::json!({
                    "name": name,
                    "backend": backend,
                    "role": role.trim(),
                    "primary_model": primary_model.trim(),
                    "scope": scope,
                }),
                180,
            )
            .await
        }
    }
}

/// Retire an agent — `bwoc retire <id> --yes --json`. Destructive (removes the
/// registry entry + files).
#[tauri::command]
async fn agent_retire(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    agent: String,
) -> Result<serde_json::Value, String> {
    if !valid_agent_id(&agent) {
        return Err("invalid agent id".into());
    }
    match Transport::resolve(&cfg_dir(&app)?, host, workspace)? {
        Transport::Local(t) => t.bwoc_json(&["retire", &agent, "--yes"]).await,
        Transport::Remote(r) => r.delete_json(&format!("/agents/{agent}")).await,
    }
}

/// One agent's health + identity — `bwoc status <id> --json`.
#[tauri::command]
async fn agent_status(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    agent: String,
) -> Result<serde_json::Value, String> {
    if !valid_agent_id(&agent) {
        return Err("invalid agent id".into());
    }
    match Transport::resolve(&cfg_dir(&app)?, host, workspace)? {
        Transport::Local(t) => t.bwoc_json(&["status", &agent]).await,
        Transport::Remote(r) => r.get_json(&format!("/agents/{agent}/status")).await,
    }
}

/// Self-describe one host (local analogue of bwocd's `/hosts`).
#[tauri::command]
async fn host_info(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
) -> Result<HostInfo, String> {
    match Transport::resolve(&cfg_dir(&app)?, host, workspace)? {
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

/// One chat turn with an agent: `bwoc run <agent> --task <prompt>`. Returns the
/// agent's text output. Long-running (the agent backend thinks).
#[tauri::command]
async fn chat_send(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    agent: String,
    prompt: String,
) -> Result<String, String> {
    if !valid_agent_id(&agent) {
        return Err("invalid agent id".into());
    }
    if prompt.trim().is_empty() {
        return Err("empty message".into());
    }
    let out = match Transport::resolve(&cfg_dir(&app)?, host, workspace)? {
        Transport::Local(t) => {
            // Match bwocd: prepend the agent's saved memory so the local chat
            // turn is incarnated the same way (persona auto-loads via cwd).
            let task = match transport::load_agent_memory(t.workspace_path(), &agent) {
                Some(mem) => format!(
                    "# Your saved memory — consult this before replying\n\n{mem}\n\n---\n\n{prompt}"
                ),
                None => prompt.clone(),
            };
            t.bwoc_json(&["run", &agent, "--task", &task, "--timeout", "120"])
                .await?
        }
        Transport::Remote(r) => {
            r.post_json(
                &format!("/agents/{agent}/chat"),
                serde_json::json!({ "prompt": prompt }),
                130,
            )
            .await?
        }
    };
    Ok(clean_agent_output(out["output"].as_str().unwrap_or("")))
}

/// `bwoc run` output is the full harness stdout: startup banner, then a
/// `done in N turn(s).` line, then the agent's final response. Keep only the
/// response. The marker is matched as the harness's own *line* (the first one),
/// so a reply that happens to contain "turn(s)." is never truncated. Falls back
/// to the trimmed raw text if the marker line is absent.
fn clean_agent_output(raw: &str) -> String {
    let mut offset = 0;
    for line in raw.split_inclusive('\n') {
        offset += line.len();
        let l = line.trim();
        if l.starts_with("done in ") && l.ends_with("turn(s).") {
            let after = raw[offset..].trim();
            if !after.is_empty() {
                return after.to_string();
            }
            break;
        }
    }
    raw.trim().to_string()
}

/// Hosts the UI can switch between. Desktop lists "local" first (LocalShell
/// against this machine's workspace); mobile omits it — there is no `bwoc` CLI
/// or workspace in the app sandbox, so only remote hosts make sense.
/// Run a fleet-control verb over the resolved transport: local `bwoc` for the
/// desktop, or the signed bwocd `POST /cli` verb proxy for a remote/mobile host.
/// Returns the command's stdout (trimmed). The remote allowlist decides which
/// verbs a given host permits.
async fn run_verb(
    app: &tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    argv: Vec<String>,
) -> Result<String, String> {
    // For a remote (multi-workspace) host the workspace is a *name* forwarded to
    // bwocd's `/cli`, which resolves it against its discovered set; for Local it's
    // the workspace path consumed by `Transport::resolve`. Keep a copy for the
    // remote body before `resolve` takes ownership.
    let ws_name = workspace.clone();
    match Transport::resolve(&cfg_dir(app)?, host, workspace)? {
        Transport::Local(t) => {
            let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
            t.bwoc_raw(&refs).await
        }
        Transport::Remote(r) => {
            let mut body = serde_json::json!({ "argv": argv });
            // A workspace *name* — unrelated to the synthetic "local" host, so a
            // remote workspace legitimately named "local" must still be sent.
            if let Some(ws) = ws_name.filter(|s| !s.is_empty()) {
                body["workspace"] = serde_json::Value::String(ws);
            }
            let v = r.post_json("/cli", body, 30).await?;
            if v["ok"].as_bool() == Some(false) || v["exit_code"].as_i64().unwrap_or(0) != 0 {
                return Err(v["stderr"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("remote cli failed")
                    .to_string());
            }
            Ok(v["stdout"].as_str().unwrap_or("").trim().to_string())
        }
    }
}

/// Send a message to one agent's inbox (`bwoc send <agent> <message>`).
#[tauri::command]
async fn agent_send(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    agent: String,
    message: String,
) -> Result<String, String> {
    if !valid_agent_id(&agent) {
        return Err("invalid agent id".into());
    }
    if message.trim().is_empty() {
        return Err("empty message".into());
    }
    run_verb(&app, host, workspace, vec!["send".into(), agent, message]).await
}

/// Broadcast to the whole workspace (`--all`) or one team (`--team <t>`).
#[tauri::command]
async fn fleet_broadcast(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    message: String,
    team: Option<String>,
) -> Result<String, String> {
    if message.trim().is_empty() {
        return Err("empty message".into());
    }
    let mut argv = vec!["send".into()];
    match team {
        Some(t) if !t.trim().is_empty() => {
            argv.push("--team".into());
            argv.push(t);
        }
        _ => argv.push("--all".into()),
    }
    argv.push(message);
    run_verb(&app, host, workspace, argv).await
}

/// Start an agent's daemon (`bwoc start <agent>`).
#[tauri::command]
async fn agent_start(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    agent: String,
) -> Result<String, String> {
    if !valid_agent_id(&agent) {
        return Err("invalid agent id".into());
    }
    run_verb(
        &app,
        host,
        workspace,
        vec!["start".into(), agent, "--yes".into()],
    )
    .await
}

/// Stop an agent's daemon (`bwoc stop <agent>`).
#[tauri::command]
async fn agent_stop(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    agent: String,
) -> Result<String, String> {
    if !valid_agent_id(&agent) {
        return Err("invalid agent id".into());
    }
    run_verb(&app, host, workspace, vec!["stop".into(), agent]).await
}

/// Retry spooled outbox messages (`bwoc outbox flush [--peer <id>]`).
#[tauri::command]
async fn outbox_flush(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
    peer: Option<String>,
) -> Result<String, String> {
    let mut argv = vec!["outbox".into(), "flush".into()];
    if let Some(p) = peer.filter(|p| !p.trim().is_empty()) {
        if !valid_agent_id(&p) {
            return Err("invalid peer id".into());
        }
        argv.push("--peer".into());
        argv.push(p);
    }
    run_verb(&app, host, workspace, argv).await
}

/// Pending durable-retry messages per peer (`bwoc outbox list`).
#[tauri::command]
async fn outbox_list(
    app: tauri::AppHandle,
    host: Option<String>,
    workspace: Option<String>,
) -> Result<String, String> {
    run_verb(&app, host, workspace, vec!["outbox".into(), "list".into()]).await
}

#[tauri::command]
fn list_hosts(app: tauri::AppHandle) -> Result<Vec<HostListEntry>, String> {
    let mut out = Vec::new();
    #[cfg(not(mobile))]
    out.push(HostListEntry {
        name: "local".into(),
        url: None,
    });
    for h in config::load_hosts(&cfg_dir(&app)?)? {
        out.push(HostListEntry {
            name: h.name,
            url: Some(h.url),
        });
    }
    Ok(out)
}

/// Add or update a remote host (writes hosts.toml).
#[tauri::command]
fn add_host(app: tauri::AppHandle, name: String, url: String) -> Result<(), String> {
    config::add_host(&cfg_dir(&app)?, &name, &url)
}

/// Remove a remote host by name.
#[tauri::command]
fn remove_host(app: tauri::AppHandle, name: String) -> Result<(), String> {
    config::remove_host(&cfg_dir(&app)?, &name)
}

/// This install's controller identity — shown in the UI so the public key can
/// be registered in a bwocd's controllers.toml (QR enrollment supersedes this).
#[tauri::command]
fn controller_info(app: tauri::AppHandle) -> Result<ControllerInfo, String> {
    let c = config::load_or_create_controller(&cfg_dir(&app)?)?;
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
        .manage(Arc::new(device::DeviceState::new()))
        .invoke_handler(tauri::generate_handler![
            fleet_snapshot,
            agent_inbox,
            agent_create,
            agent_retire,
            agent_status,
            host_info,
            list_hosts,
            controller_info,
            chat_send,
            agent_send,
            fleet_broadcast,
            agent_start,
            agent_stop,
            outbox_flush,
            outbox_list,
            add_host,
            remove_host,
            device::usb_list_ports,
            device::usb_connect,
            device::usb_autoconnect,
            device::usb_disconnect,
            device::usb_wifi_scan,
            device::usb_set_wifi,
            device::device_status,
            device::device_control
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_rejects_empty_flags_and_overlong() {
        assert!(valid_agent_id("agent-anna"));
        assert!(!valid_agent_id(""));
        assert!(!valid_agent_id("-h"));
        assert!(!valid_agent_id("--json"));
        assert!(!valid_agent_id("Agent"));
        assert!(!valid_agent_id("a/b"));
        assert!(!valid_agent_id(&"a".repeat(65)));
    }

    #[test]
    fn agent_name_is_capped_so_the_prefixed_id_fits() {
        assert!(valid_agent_name(&"a".repeat(48)));
        assert!(!valid_agent_name(&"a".repeat(49)));
        assert!(!valid_agent_name("-x"));
    }

    #[test]
    fn clean_output_splits_on_the_harness_line_only() {
        let raw = "banner\nloading…\ndone in 3 turn(s).\n\nThe reply mentions turn(s). twice: turn(s).\nend";
        assert_eq!(
            clean_agent_output(raw),
            "The reply mentions turn(s). twice: turn(s).\nend"
        );
        // A reply line that looks like the marker doesn't move the split.
        let raw = "done in 1 turn(s).\nfirst\ndone in 9 turn(s).\nsecond";
        assert_eq!(clean_agent_output(raw), "first\ndone in 9 turn(s).\nsecond");
        // No marker → trimmed raw text.
        assert_eq!(clean_agent_output("  just text \n"), "just text");
    }
}
