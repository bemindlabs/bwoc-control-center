//! Signed-request verification — the bwoc-party wire format, ported to Rust.
//!
//! What is signed (client side, see party's `src/lib/party-sign/`): a
//! canonical JSON object `{bodyHash, method, nonce, path, sender, ts}` with
//! lexicographically sorted keys (RFC 8785 reduces to exactly this for a flat
//! object of strings), signed ed25519, sent as:
//!
//!   X-BWOC-Agent       <sender id>
//!   X-BWOC-Nonce       <16-byte hex>
//!   X-BWOC-Timestamp   <RFC3339>
//!   X-BWOC-Body-SHA256 <hex>
//!   X-BWOC-Signature   <hex>
//!
//! The header set is wire-compatible with `partyctl`, so the party client
//! doubles as a test client against bwocd. Verification is fail-closed:
//! unknown sender, hash mismatch, stale timestamp, replayed nonce, or a bad
//! signature each refuse with a distinct reason (logged, never echoed in
//! detail to the caller).

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

// One canonicalizer for both sides of the wire — drift-proof by construction.
pub use cc_signing::{canonical_path, sha256_hex, signing_payload, SKEW_SECS};

#[derive(Debug, PartialEq)]
pub enum Refusal {
    UnknownSender,
    BodyHashMismatch,
    StaleTimestamp,
    ReplayedNonce,
    BadSignature,
    MalformedHeaders,
}

#[derive(serde::Deserialize)]
struct RegistryFile {
    #[serde(default)]
    controller: Vec<ControllerEntry>,
}

#[derive(serde::Deserialize, Clone)]
pub struct ControllerEntry {
    pub id: String,
    /// 32-byte ed25519 public key, hex.
    pub public_key: String,
    /// "read" or "write". Phase 1 serves read-only routes regardless.
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "read".into()
}

pub struct ControllerRegistry {
    by_id: HashMap<String, (VerifyingKey, String)>,
}

impl ControllerRegistry {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let entries = if path.is_file() {
            let parsed: RegistryFile = toml::from_str(&std::fs::read_to_string(path)?)?;
            parsed.controller
        } else {
            Vec::new()
        };
        Self::from_entries(entries)
    }

    pub fn from_entries(entries: Vec<ControllerEntry>) -> anyhow::Result<Self> {
        let mut by_id = HashMap::new();
        for e in entries {
            let bytes: [u8; 32] = hex::decode(&e.public_key)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("controller {}: public_key must be 32 bytes", e.id))?;
            let key = VerifyingKey::from_bytes(&bytes)?;
            by_id.insert(e.id, (key, e.scope));
        }
        Ok(Self { by_id })
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    fn key_of(&self, id: &str) -> Option<&VerifyingKey> {
        self.by_id.get(id).map(|(k, _)| k)
    }
}

/// Sliding-window replay defence: remember (sender, nonce) until the paired
/// timestamp falls out of 2× the skew window, then prune opportunistically.
#[derive(Default)]
pub struct NonceStore {
    seen: Mutex<HashMap<(String, String), i64>>,
}

impl NonceStore {
    /// Returns false when the nonce was already seen (replay).
    pub fn insert(&self, sender: &str, nonce: &str, ts_epoch: i64, now_epoch: i64) -> bool {
        let mut seen = self.seen.lock().expect("nonce store poisoned");
        seen.retain(|_, t| now_epoch - *t <= 2 * SKEW_SECS);
        seen.insert((sender.to_string(), nonce.to_string()), ts_epoch)
            .is_none()
    }
}

pub struct SignedHeaders<'a> {
    pub sender: &'a str,
    pub nonce: &'a str,
    pub ts: &'a str,
    pub body_sha256: &'a str,
    pub signature: &'a str,
}

/// Full fail-closed verification. `now_epoch` is injected for testability.
pub fn verify(
    registry: &ControllerRegistry,
    nonces: &NonceStore,
    h: &SignedHeaders<'_>,
    method: &str,
    canonical_path: &str,
    body: &[u8],
    now_epoch: i64,
) -> Result<(), Refusal> {
    let key = registry.key_of(h.sender).ok_or(Refusal::UnknownSender)?;

    let actual = sha256_hex(body);
    if actual != h.body_sha256.to_lowercase() {
        return Err(Refusal::BodyHashMismatch);
    }

    let ts = chrono::DateTime::parse_from_rfc3339(h.ts)
        .map_err(|_| Refusal::MalformedHeaders)?
        .timestamp();
    if (now_epoch - ts).abs() > SKEW_SECS {
        return Err(Refusal::StaleTimestamp);
    }

    let payload = signing_payload(h.sender, method, canonical_path, h.nonce, h.ts, &actual);
    let sig_bytes: [u8; 64] = hex::decode(h.signature)
        .map_err(|_| Refusal::MalformedHeaders)?
        .try_into()
        .map_err(|_| Refusal::MalformedHeaders)?;
    key.verify(payload.as_bytes(), &Signature::from_bytes(&sig_bytes))
        .map_err(|_| Refusal::BadSignature)?;

    // Nonce check LAST: a forged request must not be able to burn a nonce.
    if !nonces.insert(h.sender, h.nonce, ts, now_epoch) {
        return Err(Refusal::ReplayedNonce);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn registry_with(id: &str, key: &SigningKey) -> ControllerRegistry {
        ControllerRegistry::from_entries(vec![ControllerEntry {
            id: id.into(),
            public_key: hex::encode(key.verifying_key().to_bytes()),
            scope: "read".into(),
        }])
        .unwrap()
    }

    fn sign(
        key: &SigningKey,
        sender: &str,
        method: &str,
        path: &str,
        nonce: &str,
        ts: &str,
        body: &[u8],
    ) -> (String, String) {
        let body_hash = sha256_hex(body);
        let payload = signing_payload(sender, method, path, nonce, ts, &body_hash);
        let sig = key.sign(payload.as_bytes());
        (body_hash, hex::encode(sig.to_bytes()))
    }

    fn now_and_ts() -> (i64, String) {
        let now = chrono::Utc::now();
        (now.timestamp(), now.to_rfc3339())
    }

    #[test]
    fn valid_request_verifies() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let reg = registry_with("ctrl-1", &key);
        let nonces = NonceStore::default();
        let (now, ts) = now_and_ts();
        let (body_hash, sig) = sign(&key, "ctrl-1", "GET", "/fleet", "abcd", &ts, b"");
        let h = SignedHeaders {
            sender: "ctrl-1",
            nonce: "abcd",
            ts: &ts,
            body_sha256: &body_hash,
            signature: &sig,
        };
        assert!(verify(&reg, &nonces, &h, "GET", "/fleet", b"", now).is_ok());
    }

    #[test]
    fn unknown_sender_refused() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let reg = registry_with("ctrl-1", &key);
        let nonces = NonceStore::default();
        let (now, ts) = now_and_ts();
        let (body_hash, sig) = sign(&key, "ghost", "GET", "/fleet", "abcd", &ts, b"");
        let h = SignedHeaders {
            sender: "ghost",
            nonce: "abcd",
            ts: &ts,
            body_sha256: &body_hash,
            signature: &sig,
        };
        assert_eq!(
            verify(&reg, &nonces, &h, "GET", "/fleet", b"", now),
            Err(Refusal::UnknownSender)
        );
    }

    #[test]
    fn tampered_body_refused() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let reg = registry_with("ctrl-1", &key);
        let nonces = NonceStore::default();
        let (now, ts) = now_and_ts();
        let (body_hash, sig) = sign(&key, "ctrl-1", "POST", "/x", "n1", &ts, b"original");
        let h = SignedHeaders {
            sender: "ctrl-1",
            nonce: "n1",
            ts: &ts,
            body_sha256: &body_hash,
            signature: &sig,
        };
        assert_eq!(
            verify(&reg, &nonces, &h, "POST", "/x", b"tampered", now),
            Err(Refusal::BodyHashMismatch)
        );
    }

    #[test]
    fn stale_timestamp_refused() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let reg = registry_with("ctrl-1", &key);
        let nonces = NonceStore::default();
        let old = chrono::Utc::now() - chrono::Duration::seconds(SKEW_SECS + 60);
        let ts = old.to_rfc3339();
        let now = chrono::Utc::now().timestamp();
        let (body_hash, sig) = sign(&key, "ctrl-1", "GET", "/fleet", "n2", &ts, b"");
        let h = SignedHeaders {
            sender: "ctrl-1",
            nonce: "n2",
            ts: &ts,
            body_sha256: &body_hash,
            signature: &sig,
        };
        assert_eq!(
            verify(&reg, &nonces, &h, "GET", "/fleet", b"", now),
            Err(Refusal::StaleTimestamp)
        );
    }

    #[test]
    fn replayed_nonce_refused() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let reg = registry_with("ctrl-1", &key);
        let nonces = NonceStore::default();
        let (now, ts) = now_and_ts();
        let (body_hash, sig) = sign(&key, "ctrl-1", "GET", "/fleet", "n3", &ts, b"");
        let h = SignedHeaders {
            sender: "ctrl-1",
            nonce: "n3",
            ts: &ts,
            body_sha256: &body_hash,
            signature: &sig,
        };
        assert!(verify(&reg, &nonces, &h, "GET", "/fleet", b"", now).is_ok());
        assert_eq!(
            verify(&reg, &nonces, &h, "GET", "/fleet", b"", now),
            Err(Refusal::ReplayedNonce)
        );
    }

    #[test]
    fn wrong_key_signature_refused() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let reg = registry_with("ctrl-1", &key);
        let nonces = NonceStore::default();
        let (now, ts) = now_and_ts();
        let (body_hash, sig) = sign(&other, "ctrl-1", "GET", "/fleet", "n4", &ts, b"");
        let h = SignedHeaders {
            sender: "ctrl-1",
            nonce: "n4",
            ts: &ts,
            body_sha256: &body_hash,
            signature: &sig,
        };
        assert_eq!(
            verify(&reg, &nonces, &h, "GET", "/fleet", b"", now),
            Err(Refusal::BadSignature)
        );
    }

    #[test]
    fn canonical_path_sorts_query() {
        assert_eq!(canonical_path("/fleet", None), "/fleet");
        assert_eq!(canonical_path("/a", Some("z=2&a=1")), "/a?a=1&z=2");
    }

    #[test]
    fn path_mismatch_refused_via_signature() {
        // Signing /fleet but requesting /agents must fail: path is inside the
        // signed payload.
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let reg = registry_with("ctrl-1", &key);
        let nonces = NonceStore::default();
        let (now, ts) = now_and_ts();
        let (body_hash, sig) = sign(&key, "ctrl-1", "GET", "/fleet", "n5", &ts, b"");
        let h = SignedHeaders {
            sender: "ctrl-1",
            nonce: "n5",
            ts: &ts,
            body_sha256: &body_hash,
            signature: &sig,
        };
        assert_eq!(
            verify(&reg, &nonces, &h, "GET", "/agents", b"", now),
            Err(Refusal::BadSignature)
        );
    }
}
