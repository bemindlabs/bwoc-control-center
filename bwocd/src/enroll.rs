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

/// Ceiling on distinct pending enrollments. `/enroll` is self-signed but
/// unauthenticated — an attacker can mint unlimited valid (id, key) pairs and
/// POST each once, growing `pending.toml` without bound. `record` refuses a
/// *new* id past this cap (existing ids still refresh, so a legit pending
/// controller can retry). A real fleet has a handful awaiting approval; 32 is
/// generous. Operators clear the queue by approving/pruning `pending.toml`.
pub const MAX_PENDING: usize = 32;

/// Why a `record` was refused. The `/enroll` route maps each to a distinct HTTP
/// status so a client can tell "queue full" (retry later / contact operator)
/// from "bad request" from a server fault.
#[derive(Debug)]
pub enum RecordError {
    /// Pending queue at `MAX_PENDING` and the id is new.
    Full,
    /// Controller id failed validation (charset/length).
    InvalidId,
    /// The id is already pending under a *different* public key. The first-seen
    /// key is kept so a later self-signed request can't swap the key an
    /// operator is about to approve.
    KeyMismatch,
    /// Filesystem or TOML (de)serialization failure.
    Io(String),
}

/// A controller id must be 1–64 ASCII letters/digits or `.`, `_`, `-` (upper
/// and lower case allowed). Rejecting anything else keeps `pending.toml` keys
/// sane and blocks oversized/garbage ids from bloating the file.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Record a pending enrollment. Idempotent per (id, key) — a controller that
/// retries while awaiting approval doesn't pile up duplicates. Refuses a *new*
/// id once the queue is at [`MAX_PENDING`] (flood cap), a malformed id, and a
/// known id presenting a different key ([`RecordError::KeyMismatch`]).
pub fn record(
    workspace: &Path,
    id: &str,
    public_key: &str,
    now_rfc3339: &str,
) -> Result<(), RecordError> {
    if !valid_id(id) {
        return Err(RecordError::InvalidId);
    }
    let path = pending_path(workspace);
    let mut file: PendingFile = if path.is_file() {
        toml::from_str(&std::fs::read_to_string(&path).map_err(|e| RecordError::Io(e.to_string()))?)
            .map_err(|e| RecordError::Io(format!("bad pending.toml: {e}")))?
    } else {
        PendingFile::default()
    };

    if let Some(e) = file.pending.iter().find(|e| e.id == id) {
        // Same key → nothing to change (a retry). Different key → refuse: never
        // replace the key under an operator's pending review.
        return if e.public_key == public_key {
            Ok(())
        } else {
            Err(RecordError::KeyMismatch)
        };
    } else {
        // New id: enforce the flood cap before growing the file.
        if file.pending.len() >= MAX_PENDING {
            return Err(RecordError::Full);
        }
        file.pending.push(PendingEntry {
            id: id.to_string(),
            public_key: public_key.to_string(),
            first_seen: now_rfc3339.to_string(),
        });
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| RecordError::Io(e.to_string()))?;
    }
    let body = toml::to_string_pretty(&file).map_err(|e| RecordError::Io(e.to_string()))?;
    std::fs::write(&path, body).map_err(|e| RecordError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        // Unique per test (pid + per-process counter — no clock assumptions);
        // each gets its own pending.toml.
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let base = std::env::temp_dir().join(format!(
            "bwocd-enroll-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn caps_distinct_pending_ids_but_still_refreshes_existing() {
        let ws = tmp();
        for i in 0..MAX_PENDING {
            record(&ws, &format!("ctrl-{i}"), "deadbeef", "t0").unwrap();
        }
        // A brand-new id past the cap is refused …
        assert!(matches!(
            record(&ws, "ctrl-overflow", "deadbeef", "t1"),
            Err(RecordError::Full)
        ));
        // … but an existing pending id can still retry with its own key.
        record(&ws, "ctrl-0", "deadbeef", "t2").unwrap();
    }

    #[test]
    fn keeps_first_seen_key_for_a_pending_id() {
        let ws = tmp();
        record(&ws, "ctrl-abc123", "aaaa", "t0").unwrap();
        // A second enrollment for the same id with another key is refused …
        assert!(matches!(
            record(&ws, "ctrl-abc123", "bbbb", "t1"),
            Err(RecordError::KeyMismatch)
        ));
        // … and the stored key is still the first one.
        let raw = std::fs::read_to_string(pending_path(&ws)).unwrap();
        assert!(raw.contains("aaaa") && !raw.contains("bbbb"), "{raw}");
        // Retrying with the original key is fine.
        record(&ws, "ctrl-abc123", "aaaa", "t2").unwrap();
    }

    #[test]
    fn empty_pending_file_reads_as_empty_queue() {
        // approve-controllers.sh truncates pending.toml to "" after approval;
        // an empty file is a valid (empty) TOML document, not an error.
        let ws = tmp();
        std::fs::create_dir_all(pending_path(&ws).parent().unwrap()).unwrap();
        std::fs::write(pending_path(&ws), "").unwrap();
        record(&ws, "ctrl-1", "deadbeef", "t0").unwrap();
    }

    #[test]
    fn rejects_malformed_ids() {
        let ws = tmp();
        for bad in ["", "has space", "slash/y", &"x".repeat(65)] {
            assert!(matches!(
                record(&ws, bad, "deadbeef", "t0"),
                Err(RecordError::InvalidId)
            ));
        }
        record(&ws, "agent-anna_1.2", "deadbeef", "t0").unwrap();
    }
}
