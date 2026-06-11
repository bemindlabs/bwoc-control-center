//! Wire-level roundtrip: the SHARED client signer (cc-signing, the same code
//! the control-center app uses for RemoteHttp) against the REAL bwocd router,
//! middleware included — in-process via tower::oneshot, no sockets.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bwocd::{auth, AppState};
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
        workspace: std::env::temp_dir(),
        registry,
        nonces: auth::NonceStore::default(),
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
