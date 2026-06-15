//! Shell-out layer: bwocd never reimplements bwoc logic — the CLI is the
//! single source of truth (the same rule bwoc-mcc holds).

use std::path::Path;

pub async fn bwoc_json(workspace: &Path, args: &[&str]) -> Result<serde_json::Value, String> {
    let ws = workspace.to_path_buf();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let out = tokio::task::spawn_blocking(move || {
        // `--workspace` is a per-subcommand option, not a global one — it must
        // come after the subcommand words.
        std::process::Command::new("bwoc")
            .args(&args)
            .arg("--workspace")
            .arg(&ws)
            .arg("--json")
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("cannot exec bwoc: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "bwoc exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("bwoc emitted non-JSON: {e}"))
}
