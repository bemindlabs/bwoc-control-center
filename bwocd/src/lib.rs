//! bwocd library surface — split from the binary so integration tests can
//! drive the real router (middleware included) in-process.

pub mod auth;
pub mod bwoc;
pub mod routes;

use std::path::PathBuf;

pub struct AppState {
    pub workspace: PathBuf,
    pub registry: auth::ControllerRegistry,
    pub nonces: auth::NonceStore,
}
