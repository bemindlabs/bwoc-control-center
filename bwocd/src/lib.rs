//! bwocd library surface — split from the binary so integration tests can
//! drive the real router (middleware included) in-process.

pub mod agentmem;
pub mod auth;
pub mod bwoc;
pub mod enroll;
pub mod routes;

use std::path::PathBuf;

pub struct AppState {
    pub workspace: PathBuf,
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
    /// gate. 0 sheds every signed request (test-only).
    pub max_concurrency: usize,
}

/// Default ceiling on concurrent signed requests. Tailnet-only, small-fleet
/// posture: generous enough for real concurrent fleet work, low enough to bound
/// subprocess fan-out. Override with `--max-concurrency` / `BWOCD_MAX_CONCURRENCY`.
pub const DEFAULT_MAX_CONCURRENCY: usize = 16;
