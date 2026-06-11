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
            std::process::Command::new("bwoc")
                .args(&args)
                .arg("--workspace")
                .arg(&ws)
                .arg("--json")
                .output()
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("cannot exec bwoc: {e} — is it on PATH?"))?;
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
            std::process::Command::new("bwoc").arg("--version").output()
        })
        .await
        .ok()?
        .ok()?;
        String::from_utf8(out.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    }
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
