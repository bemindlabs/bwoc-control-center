//! Transport layer — DESIGN.md §2.
//!
//! `LocalShell` is the desktop-on-the-same-machine path: shell `bwoc … --json`
//! directly, no network. `RemoteHttp` (mobile / cross-host, ed25519-signed
//! requests to bwocd over the tailnet) lands in phase 1 alongside enrollment;
//! both will sit behind one trait once there are two implementations to unify.

use std::path::PathBuf;

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

    /// Run `bwoc --workspace <ws> <args…> --json` and parse stdout as JSON.
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

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
