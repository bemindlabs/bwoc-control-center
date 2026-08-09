//! Shell-out layer: bwocd never reimplements bwoc logic — the CLI is the
//! single source of truth (the same rule bwoc-mcc holds).
//!
//! Every spawn runs under a hard **server-side timeout**: a hung/slow `bwoc`
//! child (lock contention, a wedged agent daemon) must not pin a task forever
//! and, in aggregate, wedge the daemon. On timeout the child is killed
//! (`kill_on_drop`) so its pipes close and the task returns an error.

use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::process::Command;

/// Hard ceiling for read/JSON verbs — they should be quick.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Spawn `cmd` and await its output under `timeout`; kill the child if it
/// overruns (kill_on_drop → dropping the timed-out future terminates it).
async fn run(mut cmd: Command, timeout: Duration, label: &str) -> Result<Output, String> {
    // MUST pipe stdout/stderr: unlike `std::process::Command::output()`,
    // `tokio::process::Command::spawn()` + `wait_with_output()` does NOT capture
    // by default — the child inherits the parent's fds and `wait_with_output`
    // returns EMPTY. Without this, every `bwoc --json` came back as "" → an "EOF
    // non-JSON" error while the real JSON leaked to bwocd's own stdout/log.
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| format!("cannot exec bwoc: {e}"))?;
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(format!("bwoc I/O error: {e}")),
        Err(_) => Err(format!(
            "bwoc timed out after {}s ({label}) — killed",
            timeout.as_secs()
        )),
    }
}

pub async fn bwoc_json(workspace: &Path, args: &[&str]) -> Result<serde_json::Value, String> {
    let mut cmd = Command::new("bwoc");
    // `--workspace` is a per-subcommand option, not a global one — it must
    // come after the subcommand words.
    cmd.args(args)
        .arg("--workspace")
        .arg(workspace)
        .arg("--json");
    let out = run(cmd, READ_TIMEOUT, "json").await?;
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
/// can't retarget a different workspace, and enforces a server-side `timeout_secs`
/// ceiling so an exec verb can't run unbounded. A non-zero exit is returned in
/// `CliOutput` (not an `Err`) so the caller can inspect stderr/exit_code; `Err`
/// is reserved for a failure to *spawn* bwoc or a timeout.
pub async fn bwoc_exec(
    workspace: &Path,
    argv: &[String],
    timeout_secs: u64,
) -> Result<CliOutput, String> {
    let mut cmd = Command::new("bwoc");
    // `--workspace` is a per-subcommand option — append after the argv.
    cmd.args(argv).arg("--workspace").arg(workspace);
    let out = run(cmd, Duration::from_secs(timeout_secs), "cli").await?;
    Ok(CliOutput {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        exit_code: out.status.code().unwrap_or(-1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard for the piped-stdio fix: `run` MUST capture a child's
    /// stdout. (Before the fix, tokio's `spawn()` inherited the parent's fds and
    /// `wait_with_output()` returned empty, so every `bwoc --json` failed.)
    #[tokio::test]
    async fn run_captures_child_stdout() {
        let mut cmd = Command::new("echo");
        cmd.arg("hello-from-child");
        let out = run(cmd, Duration::from_secs(5), "test").await.unwrap();
        assert!(out.status.success());
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "hello-from-child",
            "stdout must be captured, not inherited"
        );
    }
}
