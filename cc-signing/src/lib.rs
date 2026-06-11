//! Shared signed-request primitives (the bwoc-party wire format).
//!
//! Both sides of the wire import THIS crate — the app (signer) and bwocd
//! (verifier) — so the canonical payload can never drift apart. That mirrors
//! bwoc-party, where `party-sign/jcs.mjs` is imported by client and server.
//!
//! Headers on the wire:
//!   X-BWOC-Agent       <sender id>
//!   X-BWOC-Nonce       <16-byte hex>
//!   X-BWOC-Timestamp   <RFC3339>
//!   X-BWOC-Body-SHA256 <hex>
//!   X-BWOC-Signature   <hex over the canonical payload>

use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Allowed clock skew, ±seconds — matches bwoc-party's SKEW_MS.
pub const SKEW_SECS: i64 = 300;

/// The exact canonical payload both sides sign over: a flat JSON object with
/// lexicographically sorted keys (RFC 8785 reduces to exactly this shape for
/// flat string values). Keep it byte-identical on both sides.
pub fn signing_payload(
    sender: &str,
    method: &str,
    path: &str,
    nonce: &str,
    ts: &str,
    body_hash: &str,
) -> String {
    format!(
        "{{\"bodyHash\":\"{body_hash}\",\"method\":\"{method}\",\"nonce\":\"{nonce}\",\"path\":\"{path}\",\"sender\":\"{sender}\",\"ts\":\"{ts}\"}}",
        method = method.to_uppercase()
    )
}

/// Canonical request path: pathname + query entries sorted by key then value.
pub fn canonical_path(path: &str, query: Option<&str>) -> String {
    let Some(q) = query.filter(|q| !q.is_empty()) else {
        return path.to_string();
    };
    let mut entries: Vec<(&str, &str)> = q
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
        .collect();
    entries.sort();
    let qs: Vec<String> = entries.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{path}?{}", qs.join("&"))
}

pub fn sha256_hex(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

/// Client side: build the five X-BWOC-* headers for one request.
/// `canonical` must come from [`canonical_path`].
pub fn build_signed_headers(
    sender: &str,
    seed_hex: &str,
    method: &str,
    canonical: &str,
    body: &[u8],
) -> Result<Vec<(&'static str, String)>, String> {
    let seed: [u8; 32] = hex::decode(seed_hex.trim())
        .map_err(|e| format!("bad seed hex: {e}"))?
        .try_into()
        .map_err(|_| "seed must be 32 bytes".to_string())?;
    let key = SigningKey::from_bytes(&seed);

    let mut nonce_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = hex::encode(nonce_bytes);
    let ts = chrono::Utc::now().to_rfc3339();
    let body_hash = sha256_hex(body);

    let payload = signing_payload(sender, method, canonical, &nonce, &ts, &body_hash);
    let sig = key.sign(payload.as_bytes());

    Ok(vec![
        ("X-BWOC-Agent", sender.to_string()),
        ("X-BWOC-Nonce", nonce),
        ("X-BWOC-Timestamp", ts),
        ("X-BWOC-Body-SHA256", body_hash),
        ("X-BWOC-Signature", hex::encode(sig.to_bytes())),
    ])
}

/// Derive the hex public key from a hex seed (for controller registration).
pub fn public_key_hex(seed_hex: &str) -> Result<String, String> {
    let seed: [u8; 32] = hex::decode(seed_hex.trim())
        .map_err(|e| format!("bad seed hex: {e}"))?
        .try_into()
        .map_err(|_| "seed must be 32 bytes".to_string())?;
    Ok(hex::encode(
        SigningKey::from_bytes(&seed).verifying_key().to_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    #[test]
    fn payload_is_sorted_and_uppercased() {
        let p = signing_payload("s", "get", "/a?b=1", "n", "t", "h");
        assert_eq!(
            p,
            r#"{"bodyHash":"h","method":"GET","nonce":"n","path":"/a?b=1","sender":"s","ts":"t"}"#
        );
    }

    #[test]
    fn canonical_path_sorts_query() {
        assert_eq!(canonical_path("/fleet", None), "/fleet");
        assert_eq!(canonical_path("/a", Some("z=2&a=1")), "/a?a=1&z=2");
    }

    #[test]
    fn client_headers_verify_with_dalek() {
        let seed_hex = hex::encode([5u8; 32]);
        let headers = build_signed_headers("ctrl-x", &seed_hex, "GET", "/fleet", b"").unwrap();
        let get = |k: &str| {
            headers
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        let payload = signing_payload(
            "ctrl-x",
            "GET",
            "/fleet",
            &get("X-BWOC-Nonce"),
            &get("X-BWOC-Timestamp"),
            &get("X-BWOC-Body-SHA256"),
        );
        let pub_hex = public_key_hex(&seed_hex).unwrap();
        let key =
            VerifyingKey::from_bytes(&hex::decode(pub_hex).unwrap().try_into().unwrap()).unwrap();
        let sig_bytes: [u8; 64] = hex::decode(get("X-BWOC-Signature"))
            .unwrap()
            .try_into()
            .unwrap();
        key.verify(payload.as_bytes(), &Signature::from_bytes(&sig_bytes))
            .unwrap();
    }
}
