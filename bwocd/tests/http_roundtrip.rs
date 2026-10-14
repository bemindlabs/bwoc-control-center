//! Wire-level roundtrip: the SHARED client signer (cc-signing, the same code
//! the control-center app uses for RemoteHttp) against the REAL bwocd router,
//! middleware included — in-process via tower::oneshot, no sockets.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bwocd::{auth, AppState, DEFAULT_MAX_CONCURRENCY};
use std::sync::Arc;
use tower::ServiceExt;

const SEED: [u8; 32] = [42u8; 32];

fn state() -> Arc<AppState> {
    let seed_hex = hex::encode(SEED);
    let registry = auth::ControllerRegistry::from_entries(vec![auth::ControllerEntry {
        id: "ctrl-test".into(),
        public_key: cc_signing::public_key_hex(&seed_hex).unwrap(),
        scope: "read".into(),
    }])
    .unwrap();
    Arc::new(AppState {
        workspaces: bwocd::workspaces::Workspaces::single(std::env::temp_dir()),
        registry,
        nonces: auth::NonceStore::default(),
        cors_origin: None,
        max_concurrency: DEFAULT_MAX_CONCURRENCY,
    })
}

fn state_with_cors(origin: &str, scope: &str) -> Arc<AppState> {
    let seed_hex = hex::encode(SEED);
    let registry = auth::ControllerRegistry::from_entries(vec![auth::ControllerEntry {
        id: "ctrl-test".into(),
        public_key: cc_signing::public_key_hex(&seed_hex).unwrap(),
        scope: scope.into(),
    }])
    .unwrap();
    Arc::new(AppState {
        workspaces: bwocd::workspaces::Workspaces::single(std::env::temp_dir()),
        registry,
        nonces: auth::NonceStore::default(),
        cors_origin: Some(origin.into()),
        max_concurrency: DEFAULT_MAX_CONCURRENCY,
    })
}

/// State with a specific concurrency ceiling, for the load-shed test.
fn state_capped(max: usize) -> Arc<AppState> {
    let seed_hex = hex::encode(SEED);
    let registry = auth::ControllerRegistry::from_entries(vec![auth::ControllerEntry {
        id: "ctrl-test".into(),
        public_key: cc_signing::public_key_hex(&seed_hex).unwrap(),
        scope: "read".into(),
    }])
    .unwrap();
    Arc::new(AppState {
        workspaces: bwocd::workspaces::Workspaces::single(std::env::temp_dir()),
        registry,
        nonces: auth::NonceStore::default(),
        cors_origin: None,
        max_concurrency: max,
    })
}

fn signed_request(path: &str) -> Request<Body> {
    let seed_hex = hex::encode(SEED);
    let canonical = cc_signing::canonical_path(path, None);
    let headers =
        cc_signing::build_signed_headers("ctrl-test", &seed_hex, "GET", &canonical, b"").unwrap();
    let mut req = Request::builder().uri(path).method("GET");
    for (k, v) in headers {
        req = req.header(k, v);
    }
    req.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn health_is_open() {
    let app = bwocd::routes::router(state());
    let res = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn signed_hosts_passes_the_real_middleware() {
    let app = bwocd::routes::router(state());
    let res = app.oneshot(signed_request("/hosts")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn unsigned_hosts_is_refused() {
    let app = bwocd::routes::router(state());
    let res = app
        .oneshot(Request::get("/hosts").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn whoami_returns_caller_identity_and_caps() {
    let st = state_with_cors("chrome-extension://abc", "read,exec");
    let res = bwocd::routes::router(st)
        .oneshot(signed_request("/whoami"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["controller_id"], "ctrl-test");
    assert_eq!(v["caps"], serde_json::json!(["read", "exec"]));
    assert!(v["bwocd_version"].is_string());
}

#[tokio::test]
async fn cors_preflight_is_answered_without_signature() {
    // An OPTIONS preflight carries no signature; it must still succeed and echo
    // the configured origin + allowed signing headers.
    let app = bwocd::routes::router(state_with_cors("chrome-extension://abc", "read"));
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/whoami")
        .header("origin", "chrome-extension://abc")
        .header("access-control-request-method", "GET")
        .header("access-control-request-headers", "x-bwoc-signature")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert!(res.status().is_success());
    assert_eq!(
        res.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("chrome-extension://abc")
    );
}

#[tokio::test]
async fn no_cors_headers_when_origin_unset() {
    // Default posture (native/CLI controllers): no CORS layer, no echo header.
    let app = bwocd::routes::router(state());
    let res = app.oneshot(signed_request("/hosts")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn signed_requests_are_load_shed_at_the_concurrency_ceiling() {
    // A 0-permit ceiling sheds every signed request with 503 (not 500), proving
    // the load-shed + concurrency-limit stack is wired onto the signed surface.
    let app = bwocd::routes::router(state_capped(0));
    let res = app.oneshot(signed_request("/hosts")).await.unwrap();
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn health_is_exempt_from_the_concurrency_ceiling() {
    // Liveness must always answer — the gate is scoped to the signed sub-router,
    // so even a fully-shed daemon reports healthy.
    let app = bwocd::routes::router(state_capped(0));
    let res = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn replayed_headers_are_refused() {
    let st = state();
    let req1 = signed_request("/hosts");
    // Clone the exact header set for a byte-identical replay.
    let mut req2 = Request::builder().uri("/hosts").method("GET");
    for (k, v) in req1.headers() {
        req2 = req2.header(k, v);
    }
    let req2 = req2.body(Body::empty()).unwrap();

    let res1 = bwocd::routes::router(st.clone())
        .oneshot(req1)
        .await
        .unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    let res2 = bwocd::routes::router(st).oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::UNAUTHORIZED);
}

// ── /enroll (self-signed, outside the registry middleware) ──────────────────

/// State whose primary workspace is a fresh temp dir, so `/enroll` writes its
/// own pending.toml without touching other tests.
fn state_enroll(label: &str) -> (Arc<AppState>, std::path::PathBuf) {
    let ws = std::env::temp_dir().join(format!("bwocd-enroll-http-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();
    let st = Arc::new(AppState {
        workspaces: bwocd::workspaces::Workspaces::single(ws.clone()),
        registry: auth::ControllerRegistry::from_entries(vec![]).unwrap(),
        nonces: auth::NonceStore::default(),
        cors_origin: None,
        max_concurrency: DEFAULT_MAX_CONCURRENCY,
    });
    (st, ws)
}

/// POST /enroll whose headers are signed by `signer_seed` as `header_id`, with
/// a body carrying `body_id` + `body_key`.
fn enroll_request(
    header_id: &str,
    signer_seed: [u8; 32],
    body_id: &str,
    body_key: &str,
) -> Request<Body> {
    let body = serde_json::json!({ "id": body_id, "public_key": body_key }).to_string();
    let headers = cc_signing::build_signed_headers(
        header_id,
        &hex::encode(signer_seed),
        "POST",
        &cc_signing::canonical_path("/enroll", None),
        body.as_bytes(),
    )
    .unwrap();
    let mut req = Request::builder()
        .uri("/enroll")
        .method("POST")
        .header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(k, v);
    }
    req.body(Body::from(body)).unwrap()
}

fn pubkey(seed: [u8; 32]) -> String {
    cc_signing::public_key_hex(&hex::encode(seed)).unwrap()
}

#[tokio::test]
async fn enroll_valid_self_signed_is_accepted_as_pending() {
    let (st, ws) = state_enroll("ok");
    let seed = [7u8; 32];
    let res = bwocd::routes::router(st)
        .oneshot(enroll_request(
            "ctrl-aa11",
            seed,
            "ctrl-aa11",
            &pubkey(seed),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let pending = std::fs::read_to_string(ws.join(".bwoc/bwocd/pending.toml")).unwrap();
    assert!(pending.contains("ctrl-aa11") && pending.contains(&pubkey(seed)));
    let _ = std::fs::remove_dir_all(&ws);
}

#[tokio::test]
async fn enroll_signed_by_another_key_is_refused() {
    // Body claims key B but the request is signed with key A → no proof of
    // ownership of B.
    let (st, ws) = state_enroll("wrongkey");
    let res = bwocd::routes::router(st)
        .oneshot(enroll_request(
            "ctrl-bb22",
            [8u8; 32],
            "ctrl-bb22",
            &pubkey([9u8; 32]),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(!ws.join(".bwoc/bwocd/pending.toml").exists());
    let _ = std::fs::remove_dir_all(&ws);
}

#[tokio::test]
async fn enroll_header_body_id_mismatch_is_bad_request() {
    let (st, ws) = state_enroll("mismatch");
    let seed = [10u8; 32];
    let res = bwocd::routes::router(st)
        .oneshot(enroll_request(
            "ctrl-cc33",
            seed,
            "ctrl-other",
            &pubkey(seed),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_dir_all(&ws);
}

#[tokio::test]
async fn enroll_cannot_swap_the_key_of_a_pending_id() {
    let (st, ws) = state_enroll("swap");
    let (victim, attacker) = ([11u8; 32], [12u8; 32]);
    let first = bwocd::routes::router(st.clone())
        .oneshot(enroll_request(
            "ctrl-dd44",
            victim,
            "ctrl-dd44",
            &pubkey(victim),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    // Same id, attacker's own (validly self-signed) key → refused, key kept.
    let second = bwocd::routes::router(st)
        .oneshot(enroll_request(
            "ctrl-dd44",
            attacker,
            "ctrl-dd44",
            &pubkey(attacker),
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let pending = std::fs::read_to_string(ws.join(".bwoc/bwocd/pending.toml")).unwrap();
    assert!(pending.contains(&pubkey(victim)) && !pending.contains(&pubkey(attacker)));
    let _ = std::fs::remove_dir_all(&ws);
}
