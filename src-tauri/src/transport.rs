//! Transport layer — DESIGN.md §2.
//!
//! `LocalShell` shells `bwoc … --json` on this machine (no network).
//! `RemoteHttp` signs every request with this install's controller key
//! (cc-signing — the SAME canonicalizer bwocd verifies with) and talks to a
//! `bwocd` over the tailnet. The frontend never knows which one served it.

use std::path::{Path, PathBuf};

pub enum Transport {
    Local(LocalShell),
    Remote(RemoteHttp),
}

impl Transport {
    /// `host` = None/"local" → LocalShell; otherwise a host name from the host
    /// list → RemoteHttp against its base URL. `cfg_dir` is the app's writable
    /// config dir (where the controller + hosts live).
    pub fn resolve(
        cfg_dir: &Path,
        host: Option<String>,
        workspace: Option<String>,
    ) -> Result<Self, String> {
        match host.as_deref() {
            None | Some("local") => Ok(Self::Local(LocalShell::resolve(workspace)?)),
            Some(name) => {
                let hosts = crate::config::load_hosts(cfg_dir)?;
                let entry = hosts
                    .into_iter()
                    .find(|h| h.name == name)
                    .ok_or_else(|| format!("unknown host '{name}'"))?;
                let ctrl = crate::config::load_or_create_controller(cfg_dir)?;
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

    pub fn workspace_path(&self) -> &Path {
        &self.workspace
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
        if status == reqwest::StatusCode::UNAUTHORIZED {
            // Not trusted yet → self-enroll (best-effort) and tell the user to
            // approve. The next poll succeeds once the operator promotes us.
            let enrolled = self.enroll().await.is_ok();
            return Err(if enrolled {
                format!(
                    "awaiting approval — controller '{}' requested access to this host",
                    self.controller.id
                )
            } else {
                format!(
                    "not authorized on this host (controller '{}')",
                    self.controller.id
                )
            });
        }
        if !status.is_success() {
            return Err(format!(
                "bwocd {}: {}",
                status,
                res.text().await.unwrap_or_default()
            ));
        }
        res.json().await.map_err(|e| format!("bad bwocd JSON: {e}"))
    }

    /// Self-signed POST /enroll: hand this controller's public key to the host
    /// as a pending enrollment for the operator to approve.
    async fn enroll(&self) -> Result<(), String> {
        let public_key = cc_signing::public_key_hex(&self.controller.seed_hex)?;
        let body =
            serde_json::json!({ "id": self.controller.id, "public_key": public_key }).to_string();
        let canonical = cc_signing::canonical_path("/enroll", None);
        let headers = cc_signing::build_signed_headers(
            &self.controller.id,
            &self.controller.seed_hex,
            "POST",
            &canonical,
            body.as_bytes(),
        )?;
        let client = reqwest::Client::new();
        let mut req = client
            .post(format!("{}/enroll", self.base))
            .header("content-type", "application/json")
            .body(body);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let res = req
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("enroll failed: {e}"))?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(format!("enroll rejected: {}", res.status()))
        }
    }

    /// Signed POST with a JSON body; `timeout_secs` is generous for actions
    /// like chat where the agent thinks for a while.
    pub async fn post_json(
        &self,
        path: &str,
        body: serde_json::Value,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, String> {
        let body = body.to_string();
        let canonical = cc_signing::canonical_path(path, None);
        let headers = cc_signing::build_signed_headers(
            &self.controller.id,
            &self.controller.seed_hex,
            "POST",
            &canonical,
            body.as_bytes(),
        )?;
        let client = reqwest::Client::new();
        let mut req = client
            .post(format!("{}{}", self.base, path))
            .header("content-type", "application/json")
            .body(body);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let res = req
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| format!("bwocd unreachable: {e}"))?;
        if !res.status().is_success() {
            return Err(format!(
                "bwocd {}: {}",
                res.status(),
                res.text().await.unwrap_or_default()
            ));
        }
        res.json().await.map_err(|e| format!("bad bwocd JSON: {e}"))
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Load an agent's saved memory (mirrors bwocd's agentmem): the `MEMORY.md`
/// index if present, else the other `memories/*.md` files concatenated
/// (README skipped). Capped so the chat prompt stays bounded.
pub fn load_agent_memory(workspace: &Path, id: &str) -> Option<String> {
    const MAX: usize = 8000;
    let dir = workspace.join("agents").join(id).join("memories");
    if !dir.is_dir() {
        return None;
    }
    let cap = |s: &str| -> String {
        if s.len() <= MAX {
            s.to_string()
        } else {
            let mut end = MAX;
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}\n…(memory truncated)", &s[..end])
        }
    };

    let index = dir.join("MEMORY.md");
    if let Ok(c) = std::fs::read_to_string(&index) {
        let c = c.trim();
        if !c.is_empty() {
            return Some(cap(c));
        }
    }
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter(|p| {
            !p.file_name()
                .map(|n| n.eq_ignore_ascii_case("README.md"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    let mut parts = Vec::new();
    for p in paths {
        if let Ok(c) = std::fs::read_to_string(&p) {
            let c = c.trim();
            if !c.is_empty() {
                parts.push(format!(
                    "### {}\n{c}",
                    p.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    (!parts.is_empty()).then(|| cap(&parts.join("\n\n")))
}
