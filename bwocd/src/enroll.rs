//! Enrollment pending store.
//!
//! A controller that isn't yet trusted POSTs its public key to `/enroll`
//! (self-signed — see `auth::verify_with_key`). bwocd records it here as
//! **pending**; an operator promotes it into `controllers.toml` out of band.
//! Pending ≠ trusted: writing here grants nothing until an operator approves.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone)]
pub struct PendingEntry {
    pub id: String,
    pub public_key: String,
    pub first_seen: String,
}

#[derive(Serialize, Deserialize, Default)]
struct PendingFile {
    #[serde(default)]
    pending: Vec<PendingEntry>,
}

fn pending_path(workspace: &Path) -> PathBuf {
    workspace.join(".bwoc/bwocd/pending.toml")
}

/// Record (or refresh) a pending enrollment. Idempotent per id — a controller
/// that retries while awaiting approval doesn't pile up duplicates.
pub fn record(
    workspace: &Path,
    id: &str,
    public_key: &str,
    now_rfc3339: &str,
) -> Result<(), String> {
    let path = pending_path(workspace);
    let mut file: PendingFile = if path.is_file() {
        toml::from_str(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("bad pending.toml: {e}"))?
    } else {
        PendingFile::default()
    };

    if let Some(e) = file.pending.iter_mut().find(|e| e.id == id) {
        e.public_key = public_key.to_string();
    } else {
        file.pending.push(PendingEntry {
            id: id.to_string(),
            public_key: public_key.to_string(),
            first_seen: now_rfc3339.to_string(),
        });
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = toml::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())
}
