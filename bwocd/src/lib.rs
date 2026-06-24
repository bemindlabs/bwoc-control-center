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
}
