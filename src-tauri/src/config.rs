//! App-side configuration: the controller identity (this install's ed25519
//! keypair) and the remote host list.
//!
//! The config dir is resolved per-platform by the caller via Tauri's
//! `app_config_dir()` — crucially, on iOS the app sandbox root is NOT writable
//! (only Library/Documents/tmp are), so a hardcoded `~/.config` fails with
//! EPERM. A desktop install that still has the legacy `~/.config/...` files is
//! migrated transparently on first read. Mobile enrollment moves the seed into
//! Keychain/Keystore later — the `Controller` shape stays the same.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone)]
pub struct Controller {
    pub id: String,
    pub seed_hex: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HostEntry {
    pub name: String,
    /// bwocd base URL over the tailnet, e.g. https://anna.tailXXXX.ts.net:10130
    pub url: String,
}

#[derive(Deserialize, Default)]
struct HostsFile {
    #[serde(default)]
    host: Vec<HostEntry>,
}

/// The pre-`app_config_dir` location, kept only to migrate existing desktop
/// installs (the Mac's registered controller lives here).
fn legacy_file(name: &str) -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| {
        PathBuf::from(h)
            .join(".config/bwoc-control-center")
            .join(name)
    })
}

/// Copy a legacy `~/.config/...` file into the new dir if the new one is absent.
fn migrate_legacy(dir: &Path, name: &str) {
    let dest = dir.join(name);
    if dest.exists() {
        return;
    }
    if let Some(src) = legacy_file(name) {
        if src.is_file() {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::copy(&src, &dest);
        }
    }
}

/// Load the controller identity, generating one on first run. `dir` is the
/// app's writable config dir (Tauri `app_config_dir()`).
pub fn load_or_create_controller(dir: &Path) -> Result<Controller, String> {
    migrate_legacy(dir, "controller.toml");
    let path = dir.join("controller.toml");
    if path.is_file() {
        let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        return toml::from_str(&raw).map_err(|e| format!("bad controller.toml: {e}"));
    }

    use rand::RngCore;
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    let suffix = hex::encode(&seed[..3]);
    let ctrl = Controller {
        id: format!("ctrl-{suffix}"),
        seed_hex: hex::encode(seed),
    };

    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create config dir: {e}"))?;
    let body = toml::to_string_pretty(&ctrl).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| format!("cannot write controller: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(ctrl)
}

/// Built-in fleet hosts — used when no hosts.toml exists (notably on mobile,
/// whose sandbox has no config file to seed). Tailnet hostnames aren't secret;
/// a user hosts.toml still overrides these.
fn default_hosts() -> Vec<HostEntry> {
    vec![
        HostEntry {
            name: "anna".into(),
            url: "https://kla-macbook-spro-16.tail32ca95.ts.net:10130".into(),
        },
        HostEntry {
            name: "busaba".into(),
            url: "https://bemindlabs.tail32ca95.ts.net:10130".into(),
        },
    ]
}

/// Load the remote host list, falling back to the built-in defaults when
/// hosts.toml is absent or empty. `dir` is the app's writable config dir.
pub fn load_hosts(dir: &Path) -> Result<Vec<HostEntry>, String> {
    migrate_legacy(dir, "hosts.toml");
    let path = dir.join("hosts.toml");
    if !path.is_file() {
        return Ok(default_hosts());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let parsed: HostsFile = toml::from_str(&raw).map_err(|e| format!("bad hosts.toml: {e}"))?;
    if parsed.host.is_empty() {
        return Ok(default_hosts());
    }
    Ok(parsed.host)
}
