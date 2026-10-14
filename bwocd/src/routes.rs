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
    Extension, Json, Router,
};
use std::sync::Arc;

use crate::auth;
use crate::bwoc::{bwoc_exec, bwoc_json};
use crate::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    // Build the CORS layer (if a browser origin is configured) before `state`
    // is moved into the router via `with_state`.
    let cors = cors_layer(state.cors_origin.as_deref());

    let signed = Router::new()
        // whoami: the calling controller's identity + granted caps + version.
        // Lets a browser client confirm enrollment and render its UI to match.
        .route("/whoami", get(whoami))
        .route("/hosts", get(hosts))
        .route("/fleet", get(fleet))
        // CRUD: create (POST, `write` cap), read one (GET), retire (DELETE,
        // `dangerous` cap). Mutating ops shell the same `bwoc` CLI the read
        // paths use and are capability-gated via `require_cap`, matching the
        // `/cli` tiering (so they can't be used to bypass it).
        .route("/agents", post(agent_create))
        .route("/agents/{id}", get(agent).delete(agent_retire))
        .route("/agents/{id}/status", get(agent_status))
        .route("/agents/{id}/inbox", get(agent_inbox))
        // chat = one-shot `bwoc run`; spawns the agent backend (costs tokens) →
        // `exec` cap, same as `run` over /cli.
        .route("/agents/{id}/chat", post(chat))
        // /cli = capability-gated generic verb proxy (allowlist + per-verb cap).
        .route("/cli", post(cli))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            verify_signed_request,
        ))
        // DoS bound: cap concurrent in-flight signed requests → cap concurrent
        // `bwoc` spawns. Outer-most on the signed stack, so a flood is shed
        // (503) *before* the crypto of signature verification runs. The permit
        // is held for the whole request incl. the subprocess, so N slow `bwoc
        // run`s in flight shed the N+1th. `HandleErrorLayer` maps load-shed's
        // `Overloaded` (a `BoxError`) into a 503 so axum's error type stays
        // `Infallible`.
        .layer(
            tower::ServiceBuilder::new()
                .layer(axum::error_handling::HandleErrorLayer::new(
                    |_: axum::BoxError| async { StatusCode::SERVICE_UNAVAILABLE },
                ))
                .layer(tower::load_shed::LoadShedLayer::new())
                .layer(tower::limit::GlobalConcurrencyLimitLayer::new(
                    state.max_concurrency,
                )),
        );

    let app = Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"ok": true})) }),
        )
        // /enroll is self-authenticating (signature checked against the body's
        // own key), so it sits outside the registry middleware.
        .route("/enroll", post(enroll))
        .merge(signed)
        .with_state(state);

    // CORS wraps the whole router so it also answers preflight for /enroll and
    // the signed routes. tower-http intercepts the OPTIONS preflight before the
    // signing middleware, so an unsigned preflight is never refused.
    match cors {
        Some(layer) => app.layer(layer),
        None => app,
    }
}

/// CORS layer for the browser (chrome-extension) client. `None` when no origin
/// is configured — native/CLI controllers need no CORS. The custom `X-BWOC-*`
/// signing headers make every request non-simple, so the browser preflights
/// with OPTIONS; this layer answers it and allows the signing headers back.
fn cors_layer(origin: Option<&str>) -> Option<tower_http::cors::CorsLayer> {
    use axum::http::{header, HeaderName, HeaderValue, Method};
    use tower_http::cors::{AllowOrigin, CorsLayer};

    let origin = origin?;
    let allow_origin = if origin == "*" {
        AllowOrigin::any()
    } else {
        AllowOrigin::exact(HeaderValue::from_str(origin).ok()?)
    };
    let allow_headers = [
        header::CONTENT_TYPE,
        HeaderName::from_static("x-bwoc-agent"),
        HeaderName::from_static("x-bwoc-nonce"),
        HeaderName::from_static("x-bwoc-timestamp"),
        HeaderName::from_static("x-bwoc-body-sha256"),
        HeaderName::from_static("x-bwoc-signature"),
    ];
    Some(
        CorsLayer::new()
            .allow_origin(allow_origin)
            .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
            .allow_headers(allow_headers)
            // No `allow_credentials` — auth is header-borne, there are no cookies.
            .max_age(std::time::Duration::from_secs(600)),
    )
}

/// The calling controller's identity, granted capabilities, and the daemon
/// version. Powers the extension's enrollment check + capability-aware UI.
async fn whoami(Extension(caller): Extension<auth::Caller>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "controller_id": caller.id,
        "caps": caller.caps,
        "bwocd_version": env!("CARGO_PKG_VERSION"),
    }))
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
            &state.workspaces.primary,
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
            Err(crate::enroll::RecordError::Full) => {
                eprintln!("bwocd: ENROLL refused {sender:?} — pending queue full");
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    "enrollment queue full — contact the operator",
                )
                    .into_response()
            }
            Err(crate::enroll::RecordError::InvalidId) => {
                (StatusCode::BAD_REQUEST, "invalid controller id").into_response()
            }
            Err(crate::enroll::RecordError::KeyMismatch) => {
                eprintln!("bwocd: ENROLL refused {sender:?} — id already pending with another key");
                (
                    StatusCode::CONFLICT,
                    "controller id already pending with a different key",
                )
                    .into_response()
            }
            Err(crate::enroll::RecordError::Io(e)) => {
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
            // Attach the verified caller (id + parsed caps) for handlers that
            // gate on capability (/whoami today; /cli mutations in P2).
            let caps = state
                .registry
                .scope_of(&sender)
                .map(auth::parse_caps)
                .unwrap_or_else(|| vec!["read".to_string()]);
            let caller = auth::Caller {
                id: sender.clone(),
                caps,
            };
            let mut req = Request::from_parts(parts, axum::body::Body::from(bytes));
            req.extensions_mut().insert(caller);
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
    let discovered = state.workspaces.discover();
    Json(serde_json::json!({
        "hostname": hostname,
        "workspace": state.workspaces.primary.display().to_string(),
        "workspaces": discovered.iter().map(|w| &w.name).collect::<Vec<_>>(),
        "multi_workspace": state.workspaces.is_multi(),
        "bwocd_version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn fleet(State(state): State<Arc<AppState>>) -> Response {
    // Aggregate `bwoc list` across every discovered workspace, tagging each agent
    // with its `workspace` (name) so the client can group them. A single failing
    // workspace is skipped (not fatal) so one broken registry can't blank the
    // whole fleet. In single-workspace mode this is just the one list.
    let discovered = state.workspaces.discover();
    let mut agents: Vec<serde_json::Value> = Vec::new();
    let mut any_ok = false;
    for ws in &discovered {
        match bwoc_json(&ws.path, &["list"]).await {
            Ok(mut v) => {
                any_ok = true;
                if let serde_json::Value::Array(items) = v["agents"].take() {
                    for mut a in items {
                        if let serde_json::Value::Object(map) = &mut a {
                            map.insert(
                                "workspace".into(),
                                serde_json::Value::String(ws.name.clone()),
                            );
                        }
                        agents.push(a);
                    }
                }
            }
            Err(e) => eprintln!("bwocd: fleet: skipping workspace {}: {e}", ws.name),
        }
    }
    // Only error out if EVERY workspace failed (nothing to show and a real fault).
    if !any_ok && !discovered.is_empty() {
        return upstream_error("all workspaces failed to list".into());
    }
    // Health stays a primary-workspace read — running `fleet health` per workspace
    // would be N heavy sweeps; the client's fleet view keys off the agent list.
    let health = bwoc_json(&state.workspaces.primary, &["fleet", "health"])
        .await
        .unwrap_or(serde_json::Value::Null);
    Json(serde_json::json!({
        "workspace": state.workspaces.primary.display().to_string(),
        "workspaces": discovered.iter().map(|w| &w.name).collect::<Vec<_>>(),
        "agents": agents,
        "health": health,
    }))
    .into_response()
}

async fn agent(State(state): State<Arc<AppState>>, AxPath(id): AxPath<String>) -> Response {
    if !valid_agent_id(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(ws) = state.workspaces.for_agent(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match bwoc_json(&ws, &["list"]).await {
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

#[derive(serde::Deserialize)]
struct CreateBody {
    /// Bare agent name (no `agent-` prefix) — becomes `agent-<name>`.
    name: String,
    backend: String,
    role: String,
    primary_model: String,
    #[serde(default)]
    scope: Option<String>,
    /// Target workspace by name (from the discovered set). Omit → primary. An
    /// unknown name is a 400 — in single-workspace mode only the primary's own
    /// name resolves.
    #[serde(default)]
    workspace: Option<String>,
}

/// Reject with 403 when `caller` lacks `cap` (`read` is implicit). Mirrors the
/// `/cli` capability gate so the dedicated mutating routes enforce the SAME
/// tiering — otherwise a read-only controller reaches exec/dangerous ops (create
/// / retire / run) that `/cli` refuses.
fn require_cap(caller: &auth::Caller, cap: &str) -> Option<Response> {
    if cap == "read" || caller.caps.iter().any(|c| c == cap) {
        None
    } else {
        Some(
            (
                StatusCode::FORBIDDEN,
                format!("this route needs the `{cap}` capability"),
            )
                .into_response(),
        )
    }
}

/// Create an agent: `bwoc new <name> --json --backend … --role … --primary-model
/// … --target agents/agent-<name>`. The template auto-resolves (CLI ships an
/// embedded fallback). Mutating + slow-ish (incarnation writes files).
async fn agent_create(
    State(state): State<Arc<AppState>>,
    Extension(caller): Extension<auth::Caller>,
    Json(body): Json<CreateBody>,
) -> Response {
    if let Some(r) = require_cap(&caller, "write") {
        return r;
    }
    let name = body.name.trim();
    if !valid_agent_name(name) {
        return (
            StatusCode::BAD_REQUEST,
            "name must be lowercase letters/digits/'-'",
        )
            .into_response();
    }
    if !valid_backend(&body.backend) {
        return (StatusCode::BAD_REQUEST, "invalid backend").into_response();
    }
    if body.role.trim().is_empty() || body.primary_model.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "role and primary_model are required",
        )
            .into_response();
    }
    let target = format!("agents/agent-{name}");
    // `bwoc new` requires the four verification gates at incarnation (it cannot
    // prompt over HTTP). Default them to the no-op `true`; the operator refines
    // them later (edit the agent, or recreate it bound to a project).
    // NB: bwoc_json already appends `--json` (+ `--workspace`); passing `--json`
    // here makes bwoc error "the argument '--json' cannot be used multiple times".
    let mut args: Vec<&str> = vec![
        "new",
        name,
        "--backend",
        &body.backend,
        "--role",
        body.role.trim(),
        "--primary-model",
        body.primary_model.trim(),
        "--lint-cmd",
        "true",
        "--format-cmd",
        "true",
        "--test-cmd",
        "true",
        "--build-cmd",
        "true",
        "--target",
        &target,
    ];
    let scope = body
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(s) = scope {
        args.push("--scope");
        args.push(s);
    }
    // Resolve the target workspace by name (from the discovered set) — never an
    // arbitrary path. Unknown name → 400; omitted → primary.
    let ws = match body.workspace.as_deref() {
        Some(name) => match state.workspaces.by_name(name) {
            Some(p) => p,
            None => return (StatusCode::BAD_REQUEST, "unknown workspace").into_response(),
        },
        None => state.workspaces.primary.clone(),
    };
    eprintln!("bwocd: CREATE agent-{name} backend={}", body.backend);
    match bwoc_json(&ws, &args).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => upstream_error(e),
    }
}

/// Retire an agent: `bwoc retire <id> --yes --json` (removes registry entry +
/// files). Destructive — `--yes` bypasses the CLI's confirm prompt.
async fn agent_retire(
    State(state): State<Arc<AppState>>,
    Extension(caller): Extension<auth::Caller>,
    AxPath(id): AxPath<String>,
) -> Response {
    // Destructive (removes registry entry + files) → `dangerous` cap.
    if let Some(r) = require_cap(&caller, "dangerous") {
        return r;
    }
    if !valid_agent_id(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(ws) = state.workspaces.for_agent(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    eprintln!("bwocd: RETIRE {id} by {}", caller.id);
    match bwoc_json(&ws, &["retire", &id, "--yes"]).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => upstream_error(e),
    }
}

/// One agent's health + identity: `bwoc status <id> --json`.
async fn agent_status(State(state): State<Arc<AppState>>, AxPath(id): AxPath<String>) -> Response {
    if !valid_agent_id(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(ws) = state.workspaces.for_agent(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match bwoc_json(&ws, &["status", &id]).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => upstream_error(e),
    }
}

async fn agent_inbox(State(state): State<Arc<AppState>>, AxPath(id): AxPath<String>) -> Response {
    if !valid_agent_id(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(ws) = state.workspaces.for_agent(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match bwoc_json(&ws, &["inbox", &id]).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => upstream_error(e),
    }
}

#[derive(serde::Deserialize)]
struct ChatBody {
    prompt: String,
    #[serde(default)]
    timeout: Option<u32>,
}

/// One-shot chat turn: `bwoc run <id> --task <prompt> --json`. Blocks while the
/// agent thinks, then returns its `{output, duration_ms, …}`.
async fn chat(
    State(state): State<Arc<AppState>>,
    Extension(caller): Extension<auth::Caller>,
    AxPath(id): AxPath<String>,
    Json(body): Json<ChatBody>,
) -> Response {
    // `bwoc run` spawns an LLM agent turn (spends tokens) → `exec` cap, same as
    // `run` over /cli.
    if let Some(r) = require_cap(&caller, "exec") {
        return r;
    }
    if !valid_agent_id(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if body.prompt.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "empty prompt").into_response();
    }
    let Some(ws) = state.workspaces.for_agent(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let timeout = body.timeout.unwrap_or(120).clamp(5, 600).to_string();
    // Incarnate fully: persona (AGENTS.md) is loaded by `bwoc run`; prepend the
    // agent's saved memory so the turn has its prior context too.
    let task = match crate::agentmem::load(&ws, &id) {
        Some(mem) => format!(
            "# Your saved memory — consult this before replying\n\n{mem}\n\n---\n\n{}",
            body.prompt
        ),
        None => body.prompt.clone(),
    };
    eprintln!(
        "bwocd: CHAT {id} ({} chars prompt, memory {})",
        body.prompt.len(),
        if task.len() > body.prompt.len() {
            "loaded"
        } else {
            "none"
        }
    );
    match bwoc_json(&ws, &["run", &id, "--task", &task, "--timeout", &timeout]).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => upstream_error(e),
    }
}

/// Agent ids are kebab-case (`agent-<name>`): reject anything that could
/// smuggle CLI flags or path segments into the shell-out. A leading `-` is
/// refused because the id is a positional CLI argument (`--help` would parse as
/// a flag — and in single-workspace mode `for_agent` doesn't vet the id).
fn valid_agent_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Bare agent name for `bwoc new` — same charset as an id, must not start with
/// `-` (would parse as a flag) and capped shorter (the `agent-` prefix is added).
fn valid_agent_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Known backend identifiers (a closed set — never interpolate an arbitrary
/// string into the CLI invocation).
fn valid_backend(b: &str) -> bool {
    matches!(
        b,
        "claude" | "codex" | "agy" | "kimi" | "ollama" | "openai" | "copilot" | "openrouter"
    )
}

fn upstream_error(e: String) -> Response {
    eprintln!("bwocd: upstream bwoc error: {e}");
    (StatusCode::BAD_GATEWAY, "bwoc CLI error").into_response()
}

// ── /cli — capability-gated generic verb proxy ───────────────────────────────
//
// A remote controller drives a bounded set of `bwoc` verbs through one signed
// endpoint (the other routes hardcode a handful; this generalizes them without
// bwocd reimplementing bwoc). It is deliberately narrow: an allowlist of verbs,
// each mapped to a required capability, with the workspace pinned by bwocd.

#[derive(serde::Deserialize)]
struct CliBody {
    argv: Vec<String>,
    /// Optional per-call server-side ceiling (seconds). Clamped to [5, 600] and
    /// enforced by bwocd (the child is killed on overrun) so an exec verb can't
    /// run unbounded. Defaults to 120s when omitted.
    #[serde(default)]
    timeout_secs: Option<u64>,
    /// Target workspace by name (from the discovered set). Omit → primary. Lets a
    /// multi-workspace client run a verb (send/run/start/…) against the agent's
    /// own workspace. Resolved by name only — never an arbitrary path.
    #[serde(default)]
    workspace: Option<String>,
}

/// The capability a `/cli` verb requires, or `None` if the verb is not proxied.
/// `read` is implicit for every enrolled controller; `write`/`exec`/`dangerous`
/// must be explicitly granted at enrollment. Matches on the leading NON-flag
/// tokens (the subcommand path) so trailing flags/values don't change the class.
fn required_cap_for(argv: &[String]) -> Option<&'static str> {
    let words: Vec<&str> = argv
        .iter()
        .map(|s| s.as_str())
        .take_while(|w| !w.starts_with('-'))
        .collect();
    match words.as_slice() {
        // read (any enrolled controller)
        ["list", ..] | ["status", ..] | ["receipts", ..] => Some("read"),
        ["fleet", "health", ..] | ["fleet", "status", ..] => Some("read"),
        ["team", "list", ..] => Some("read"),
        ["task", "list", ..] => Some("read"),
        ["memory", "list", ..] | ["memory", "show", ..] | ["memory", "search", ..] => Some("read"),
        ["outbox", "list", ..] => Some("read"),
        // write
        ["send", ..] => Some("write"), // incl. broadcast (send --all / --team)
        ["task", "add", ..] | ["task", "claim", ..] | ["task", "complete", ..] => Some("write"),
        ["outbox", "flush", ..] => Some("write"),
        // exec — spawn/kill an agent daemon on the host
        ["run", ..] | ["start", ..] | ["stop", ..] => Some("exec"),
        // Creation/retirement have dedicated routes; dangerous verbs and anything
        // unrecognized are refused here.
        _ => None,
    }
}

/// Audit-log form of a `/cli` argv: the leading verb words only (at most two,
/// e.g. `task add`, `send agent-x`) plus the arg count. Never the full argv —
/// trailing args carry message/task text that doesn't belong in the journal.
/// Debug-formatted so a crafted word can't forge a log line.
fn cli_log_summary(argv: &[String]) -> String {
    let words: Vec<&str> = argv
        .iter()
        .map(String::as_str)
        .take_while(|w| !w.starts_with('-'))
        .take(2)
        .collect();
    format!("{:?} +{} arg(s)", words.join(" "), argv.len() - words.len())
}

/// Reject args that would let a caller escape bwocd's confinement: bwocd sets
/// `--workspace` itself, so a caller-supplied one (which would retarget a
/// different workspace, or duplicate-error) is forbidden.
fn arg_is_forbidden(a: &str) -> bool {
    a == "--workspace" || a.starts_with("--workspace=") || a == "-C"
}

async fn cli(
    State(state): State<Arc<AppState>>,
    Extension(caller): Extension<auth::Caller>,
    Json(body): Json<CliBody>,
) -> Response {
    if body.argv.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty argv").into_response();
    }
    if body.argv.iter().any(|a| arg_is_forbidden(a)) {
        return (
            StatusCode::BAD_REQUEST,
            "forbidden arg: --workspace / -C are set by bwocd",
        )
            .into_response();
    }
    let cap = match required_cap_for(&body.argv) {
        Some(c) => c,
        None => return (StatusCode::FORBIDDEN, "verb not allowed via /cli").into_response(),
    };
    // `read` is implicit; other tiers must be explicitly granted to this caller.
    if cap != "read" && !caller.caps.iter().any(|c| c == cap) {
        return (
            StatusCode::FORBIDDEN,
            format!("this verb needs the `{cap}` capability"),
        )
            .into_response();
    }
    eprintln!(
        "bwocd: CLI [{}] tier={cap} by {}",
        cli_log_summary(&body.argv),
        caller.id
    );
    // Resolve the target workspace by name (validated against the discovered set)
    // — unknown name → 400, never an arbitrary path.
    let ws = match body.workspace.as_deref() {
        Some(name) => match state.workspaces.by_name(name) {
            Some(p) => p,
            None => return (StatusCode::BAD_REQUEST, "unknown workspace").into_response(),
        },
        None => state.workspaces.primary.clone(),
    };
    let timeout = body.timeout_secs.unwrap_or(120).clamp(5, 600);
    match bwoc_exec(&ws, &body.argv, timeout).await {
        Ok(out) => Json(serde_json::json!({
            "ok": out.exit_code == 0,
            "exit_code": out.exit_code,
            "stdout": out.stdout,
            "stderr": out.stderr,
            "tier": cap,
        }))
        .into_response(),
        Err(e) => upstream_error(e),
    }
}

#[cfg(test)]
mod cli_tests {
    use super::{arg_is_forbidden, cli_log_summary, require_cap, required_cap_for, valid_agent_id};
    use crate::auth::Caller;

    fn caller(caps: &[&str]) -> Caller {
        Caller {
            id: "ctrl-1".into(),
            caps: caps.iter().copied().map(String::from).collect(),
        }
    }

    #[test]
    fn require_cap_enforces_tiering_for_mutating_routes() {
        // require_cap returns Some(rejection) when denied, None when allowed.
        let ro = caller(&["read"]);
        // read-only controller is refused the mutating-route caps (the A1 gap).
        assert!(require_cap(&ro, "write").is_some());
        assert!(require_cap(&ro, "exec").is_some()); // e.g. /chat = bwoc run
        assert!(require_cap(&ro, "dangerous").is_some()); // e.g. DELETE /agents
                                                          // read is implicit for everyone.
        assert!(require_cap(&ro, "read").is_none());
        // granted caps pass; a lower grant doesn't imply a higher one.
        assert!(require_cap(&caller(&["read", "write"]), "write").is_none());
        assert!(require_cap(&caller(&["read", "write"]), "dangerous").is_some());
        assert!(require_cap(&caller(&["read", "exec"]), "exec").is_none());
        assert!(require_cap(&caller(&["read", "dangerous"]), "dangerous").is_none());
    }

    fn v(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn read_verbs_need_read() {
        assert_eq!(required_cap_for(&v("list --json")), Some("read"));
        assert_eq!(
            required_cap_for(&v("status agent-anna --json")),
            Some("read")
        );
        assert_eq!(required_cap_for(&v("team list --json")), Some("read"));
        assert_eq!(
            required_cap_for(&v("task list tianting --json")),
            Some("read")
        );
        assert_eq!(required_cap_for(&v("memory list --json")), Some("read"));
        assert_eq!(required_cap_for(&v("outbox list")), Some("read"));
    }

    #[test]
    fn mutating_verbs_need_write_or_exec() {
        assert_eq!(required_cap_for(&v("send agent-anna hi")), Some("write"));
        // broadcast rides the send arm.
        assert_eq!(required_cap_for(&v("send --all hi")), Some("write"));
        assert_eq!(required_cap_for(&v("send --team t hi")), Some("write"));
        assert_eq!(
            required_cap_for(&v("task add tianting do-x")),
            Some("write")
        );
        assert_eq!(required_cap_for(&v("outbox flush")), Some("write"));
        assert_eq!(
            required_cap_for(&v("outbox flush --peer agent-x")),
            Some("write")
        );
        assert_eq!(
            required_cap_for(&v("run agent-anna --task x")),
            Some("exec")
        );
        // agent-daemon lifecycle spawns/kills a process on the host → exec.
        assert_eq!(required_cap_for(&v("start agent-anna --yes")), Some("exec"));
        assert_eq!(required_cap_for(&v("stop agent-anna")), Some("exec"));
    }

    #[test]
    fn unlisted_and_dangerous_verbs_are_refused() {
        assert_eq!(required_cap_for(&v("retire agent-x --yes")), None);
        assert_eq!(required_cap_for(&v("new foo")), None);
        assert_eq!(required_cap_for(&v("rm -rf /")), None);
        assert_eq!(required_cap_for(&v("")), None);
        // `task` alone (no sub-verb) is not a known read/write path.
        assert_eq!(required_cap_for(&v("task")), None);
    }

    #[test]
    fn agent_id_cannot_be_a_flag() {
        assert!(valid_agent_id("agent-anna"));
        assert!(!valid_agent_id("--help"));
        assert!(!valid_agent_id("-h"));
        assert!(!valid_agent_id(""));
        assert!(!valid_agent_id(&"a".repeat(65)));
    }

    #[test]
    fn cli_log_omits_message_text() {
        let line = cli_log_summary(&v("send agent-anna secret plans here"));
        assert!(line.contains("send agent-anna"), "{line}");
        assert!(!line.contains("secret"), "{line}");
        let line = cli_log_summary(&v("run agent-x --task leak-me"));
        assert!(!line.contains("leak-me"), "{line}");
        // Debug formatting escapes control chars → no forged log lines.
        let line = cli_log_summary(&["send\nbwocd: OK".to_string()]);
        assert!(!line.contains('\n'), "{line}");
    }

    #[test]
    fn workspace_override_is_forbidden() {
        assert!(arg_is_forbidden("--workspace"));
        assert!(arg_is_forbidden("--workspace=/etc"));
        assert!(arg_is_forbidden("-C"));
        assert!(!arg_is_forbidden("--json"));
        assert!(!arg_is_forbidden("--task"));
    }
}
