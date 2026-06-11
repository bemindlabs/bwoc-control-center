//! Transport layer — DESIGN.md §2.
//!
//! `LocalShell` shells `bwoc … --json` on this machine (no network).
//! `RemoteHttp` signs every request with this install's controller key
//! (cc-signing — the SAME canonicalizer bwocd verifies with) and talks to a
//! `bwocd` over the tailnet. The frontend never knows which one served it.

use std::path::PathBuf;

pub enum Transport {
    Local(LocalShell),
    Remote(RemoteHttp),
}

impl Transport {
    /// `host` = None/"local" → LocalShell; otherwise a host name from
    /// hosts.toml → RemoteHttp against its base URL.
    pub fn resolve(host: Option<String>, workspace: Option<String>) -> Result<Self, String> {
        match host.as_deref() {
            None | Some("local") => Ok(Self::Local(LocalShell::resolve(workspace)?)),
            Some(name) => {
                let hosts = crate::config::load_hosts()?;
                let entry = hosts
                    .into_iter()
                    .find(|h| h.name == name)
                    .ok_or_else(|| format!("unknown host '{name}' (not in hosts.toml)"))?;
                let ctrl = crate::config::load_or_create_controller()?;
                Ok(Self::Remote(RemoteHttp {
                    base: entry.url.trim_end_matches('/').to_string(),
                    controller: ctrl,
                }))
            }
        }
    }
}

// ---------------------------------------------------------------------------

pub struct LocalShell {
    workspace: PathBuf,
}

impl LocalShell {
    /// Resolve the target workspace: explicit arg > `BWOC_WORKSPACE` env >
    /// `~/workspaces/bwoc`. Fails when the directory does not exist — a wrong
    /// path must surface, not render an empty fleet.
    pub fn resolve(workspace: Option<String>) -> Result<Self, String> {
        let ws = workspace
            .or_else(|| std::env::var("BWOC_WORKSPACE").ok())
            .map(PathBuf::from)
            .or_else(|| dirs_home().map(|h| h.join("workspaces/bwoc")))
            .ok_or("cannot resolve a workspace path")?;
        if !ws.is_dir() {
            return Err(format!("workspace not found: {}", ws.display()));
        }
        Ok(Self { workspace: ws })
    }

    pub fn workspace(&self) -> &str {
        self.workspace.to_str().unwrap_or("")
    }

    /// Run `bwoc <args…> --workspace <ws> --json` and parse stdout as JSON.
    pub async fn bwoc_json(&self, args: &[&str]) -> Result<serde_json::Value, String> {
        let ws = self.workspace.clone();
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let out = tauri::async_runtime::spawn_blocking(move || {
            // `--workspace` is a per-subcommand option — it goes after the
            // subcommand words, not before them.
            std::process::Command::new(bwoc_bin())
                .args(&args)
                .arg("--workspace")
                .arg(&ws)
                .arg("--json")
                .output()
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("cannot exec bwoc: {e} — install it or set BWOC_BIN"))?;
        if !out.status.success() {
            return Err(format!(
                "bwoc exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("bwoc emitted non-JSON output: {e}"))
    }

    pub async fn bwoc_version(&self) -> Option<String> {
        let out = tauri::async_runtime::spawn_blocking(|| {
            std::process::Command::new(bwoc_bin())
                .arg("--version")
                .output()
        })
        .await
        .ok()?
        .ok()?;
        String::from_utf8(out.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    }
}

/// Resolve the `bwoc` binary. A GUI app launched from Finder gets a minimal
/// PATH that usually lacks `~/.cargo/bin` etc., so a bare `bwoc` would fail to
/// exec — probe the well-known install dirs (the bwoc-mcc approach), with a
/// `BWOC_BIN` override winning. Falls back to bare `bwoc` (honors PATH when the
/// app *was* launched from a shell, e.g. `tauri dev`).
fn bwoc_bin() -> PathBuf {
    if let Some(p) = std::env::var_os("BWOC_BIN") {
        return PathBuf::from(p);
    }
    let home = dirs_home();
    let candidates = [
        PathBuf::from("/opt/homebrew/bin/bwoc"),
        PathBuf::from("/usr/local/bin/bwoc"),
        home.as_ref()
            .map(|h| h.join(".local/bin/bwoc"))
            .unwrap_or_default(),
        home.as_ref()
            .map(|h| h.join(".cargo/bin/bwoc"))
            .unwrap_or_default(),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("bwoc"))
}

// ---------------------------------------------------------------------------

pub struct RemoteHttp {
    base: String,
    controller: crate::config::Controller,
}

impl RemoteHttp {
    /// Signed GET against bwocd; `path` must start with '/' and carry no query
    /// (extend with cc_signing::canonical_path sorting when queries appear).
    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let canonical = cc_signing::canonical_path(path, None);
        let headers = cc_signing::build_signed_headers(
            &self.controller.id,
            &self.controller.seed_hex,
            "GET",
            &canonical,
            b"",
        )?;
        let client = reqwest::Client::new();
        let mut req = client.get(format!("{}{}", self.base, path));
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let res = req
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("bwocd unreachable: {e}"))?;
        let status = res.status();
        if !status.is_success() {
            return Err(format!(
                "bwocd {}: {} (controller '{}' registered on that host?)",
                status,
                res.text().await.unwrap_or_default(),
                self.controller.id
            ));
        }
        res.json().await.map_err(|e| format!("bad bwocd JSON: {e}"))
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
