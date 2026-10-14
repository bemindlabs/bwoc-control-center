//! bwocd library surface — split from the binary so integration tests can
//! drive the real router (middleware included) in-process.

pub mod agentmem;
pub mod auth;
pub mod bwoc;
pub mod enroll;
pub mod routes;
pub mod workspaces;

pub struct AppState {
    /// Workspace resolver: single (`--workspace`) or rooted multi-workspace
    /// (`--workspace-root`, discovered dynamically). Replaces the old single
    /// `workspace` path — use `workspaces.primary` for the enroll/pending store
    /// and `workspaces.for_agent(id)` / `by_name(name)` for targeted ops.
    pub workspaces: workspaces::Workspaces,
    pub registry: auth::ControllerRegistry,
    pub nonces: auth::NonceStore,
    /// CORS allow-origin for browser clients (the chrome extension). `None`
    /// disables CORS entirely (the default — native/CLI clients need none);
    /// `Some("chrome-extension://<id>")` allows exactly that origin;
    /// `Some("*")` (dev only, `--insecure-cors`) allows any.
    pub cors_origin: Option<String>,
    /// Max concurrent in-flight *signed* requests. Bounds concurrent `bwoc`
    /// subprocess spawns so a burst (many controllers, or one flooding a valid
    /// key) can't fork-bomb the host or grow unbounded memory. Excess is shed
    /// with 503 rather than queued. `/health` and `/enroll` are outside this
    /// gate. 0 sheds every signed request — the CLI refuses it, so only tests
    /// construct that.
    pub max_concurrency: usize,
}

/// Default ceiling on concurrent signed requests. Tailnet-only, small-fleet
/// posture: generous enough for real concurrent fleet work, low enough to bound
/// subprocess fan-out. Override with `--max-concurrency` / `BWOCD_MAX_CONCURRENCY`.
pub const DEFAULT_MAX_CONCURRENCY: usize = 16;
