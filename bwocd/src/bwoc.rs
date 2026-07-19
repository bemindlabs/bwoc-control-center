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

/// Captured output of a raw `bwoc` invocation (used by the `/cli` proxy, which
/// forwards the caller's argv faithfully instead of forcing `--json`).
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Run `bwoc <argv> --workspace <ws>` and capture the output **without** forcing
/// `--json` — the `/cli` caller controls its own output shape (some verbs, e.g.
/// `send`, don't support `--json`). bwocd still pins `--workspace` so a caller
/// can't retarget a different workspace. A non-zero exit is returned in
/// `CliOutput` (not an `Err`) so the caller can inspect stderr/exit_code; `Err`
/// is reserved for a failure to *spawn* bwoc.
pub async fn bwoc_exec(workspace: &Path, argv: &[String]) -> Result<CliOutput, String> {
    let ws = workspace.to_path_buf();
    let argv: Vec<String> = argv.to_vec();
    let out = tokio::task::spawn_blocking(move || {
        std::process::Command::new("bwoc")
            .args(&argv)
            // `--workspace` is a per-subcommand option — append after the argv.
            .arg("--workspace")
            .arg(&ws)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("cannot exec bwoc: {e}"))?;
    Ok(CliOutput {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        exit_code: out.status.code().unwrap_or(-1),
    })
}
