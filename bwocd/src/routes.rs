//! HTTP surface — DESIGN.md §4, phase-1 read-only set.
//!
//! `/health` is the only unauthenticated route (liveness, no data). Everything
//! else passes the signed-request middleware; refusals are logged with reason
//! but answered with a uniform 401 (no oracle for attackers).

use axum::{
    extract::{Path as AxPath, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

use crate::auth;
use crate::bwoc::bwoc_json;
use crate::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    let signed = Router::new()
        .route("/hosts", get(hosts))
        .route("/fleet", get(fleet))
        .route("/agents/{id}", get(agent))
        .route("/agents/{id}/inbox", get(agent_inbox))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            verify_signed_request,
        ));

    Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"ok": true})) }),
        )
        // /enroll is self-authenticating (signature checked against the body's
        // own key), so it sits outside the registry middleware.
        .route("/enroll", post(enroll))
        .merge(signed)
        .with_state(state)
}

#[derive(serde::Deserialize)]
struct EnrollBody {
    id: String,
    public_key: String,
}

/// Self-signed enrollment: record a controller's public key as **pending** for
/// an operator to approve. The request must be signed by the key it carries
/// (proving ownership) and the X-BWOC-Agent header must equal the body id.
async fn enroll(State(state): State<Arc<AppState>>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let parsed: EnrollBody = match serde_json::from_slice(&bytes) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "expected {id, public_key}").into_response(),
    };

    let h = |name: &str| -> String {
        parts
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let (sender, nonce, ts, body_sha, sig) = (
        h("x-bwoc-agent"),
        h("x-bwoc-nonce"),
        h("x-bwoc-timestamp"),
        h("x-bwoc-body-sha256"),
        h("x-bwoc-signature"),
    );
    if sender != parsed.id {
        return (StatusCode::BAD_REQUEST, "sender/id mismatch").into_response();
    }
    let headers = auth::SignedHeaders {
        sender: &sender,
        nonce: &nonce,
        ts: &ts,
        body_sha256: &body_sha,
        signature: &sig,
    };
    let canonical = auth::canonical_path(parts.uri.path(), parts.uri.query());
    let now = chrono::Utc::now();

    match auth::verify_with_key(
        &state.nonces,
        &headers,
        &parsed.public_key,
        parts.method.as_str(),
        &canonical,
        &bytes,
        now.timestamp(),
    ) {
        Ok(()) => match crate::enroll::record(
            &state.workspace,
            &parsed.id,
            &parsed.public_key,
            &now.to_rfc3339(),
        ) {
            Ok(()) => {
                eprintln!(
                    "bwocd: ENROLL pending {} (awaiting operator approval)",
                    parsed.id
                );
                (
                    StatusCode::ACCEPTED,
                    "enrollment requested — awaiting approval",
                )
                    .into_response()
            }
            Err(e) => {
                eprintln!("bwocd: enroll record error: {e}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        },
        Err(reason) => {
            eprintln!("bwocd: ENROLL refused {sender:?} reason={reason:?}");
            (StatusCode::UNAUTHORIZED, "self-signed request required").into_response()
        }
    }
}

async fn verify_signed_request(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };

    let h = |name: &str| -> String {
        parts
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let (sender, nonce, ts, body_sha, sig) = (
        h("x-bwoc-agent"),
        h("x-bwoc-nonce"),
        h("x-bwoc-timestamp"),
        h("x-bwoc-body-sha256"),
        h("x-bwoc-signature"),
    );
    let headers = auth::SignedHeaders {
        sender: &sender,
        nonce: &nonce,
        ts: &ts,
        body_sha256: &body_sha,
        signature: &sig,
    };
    let canonical = auth::canonical_path(parts.uri.path(), parts.uri.query());
    let now = chrono::Utc::now().timestamp();

    match auth::verify(
        &state.registry,
        &state.nonces,
        &headers,
        parts.method.as_str(),
        &canonical,
        &bytes,
        now,
    ) {
        Ok(()) => {
            // Access log: who reached which route. Pairs with the REFUSED line
            // below so the journal is a complete audit trail, not just failures.
            eprintln!("bwocd: OK {} {} sender={sender}", parts.method, canonical);
            let req = Request::from_parts(parts, axum::body::Body::from(bytes));
            next.run(req).await
        }
        Err(reason) => {
            eprintln!(
                "bwocd: REFUSED {} {} sender={:?} reason={:?}",
                parts.method, canonical, sender, reason
            );
            (StatusCode::UNAUTHORIZED, "signed request required").into_response()
        }
    }
}

async fn hosts(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    Json(serde_json::json!({
        "hostname": hostname,
        "workspace": state.workspace.display().to_string(),
        "bwocd_version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn fleet(State(state): State<Arc<AppState>>) -> Response {
    // `bwoc list --json` returns `{"agents": [...]}` — unwrap to the array.
    let agents = match bwoc_json(&state.workspace, &["list"]).await {
        Ok(mut v) => v["agents"].take(),
        Err(e) => return upstream_error(e),
    };
    let health = bwoc_json(&state.workspace, &["fleet", "health"])
        .await
        .unwrap_or(serde_json::Value::Null);
    Json(serde_json::json!({
        "workspace": state.workspace.display().to_string(),
        "agents": agents,
        "health": health,
    }))
    .into_response()
}

async fn agent(State(state): State<Arc<AppState>>, AxPath(id): AxPath<String>) -> Response {
    if !valid_agent_id(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match bwoc_json(&state.workspace, &["list"]).await {
        Ok(mut v) => match v["agents"].take() {
            serde_json::Value::Array(items) => {
                match items.into_iter().find(|a| a["id"] == id.as_str()) {
                    Some(found) => Json(found).into_response(),
                    None => StatusCode::NOT_FOUND.into_response(),
                }
            }
            _ => upstream_error("unexpected bwoc list shape".into()),
        },
        Err(e) => upstream_error(e),
    }
}

async fn agent_inbox(State(state): State<Arc<AppState>>, AxPath(id): AxPath<String>) -> Response {
    if !valid_agent_id(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match bwoc_json(&state.workspace, &["inbox", &id]).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => upstream_error(e),
    }
}

/// Agent ids are kebab-case (`agent-<name>`): reject anything that could
/// smuggle CLI flags or path segments into the shell-out.
fn valid_agent_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn upstream_error(e: String) -> Response {
    eprintln!("bwocd: upstream bwoc error: {e}");
    (StatusCode::BAD_GATEWAY, "bwoc CLI error").into_response()
}
