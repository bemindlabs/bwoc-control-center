//! App-side configuration: the controller identity (this install's ed25519
//! keypair) and the remote host list.
//!
//! Lives in `~/.config/bwoc-control-center/` for phase 1 (file mode 0600 on
//! the key). Mobile enrollment moves the seed into Keychain/Keystore — the
//! `Controller` shape stays the same.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

fn config_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("no HOME")?;
    Ok(PathBuf::from(home).join(".config/bwoc-control-center"))
}

/// Load the controller identity, generating one on first run.
pub fn load_or_create_controller() -> Result<Controller, String> {
    let dir = config_dir()?;
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

    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body = toml::to_string_pretty(&ctrl).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(ctrl)
}

/// Load the remote host list (empty when hosts.toml does not exist yet).
pub fn load_hosts() -> Result<Vec<HostEntry>, String> {
    let path = config_dir()?.join("hosts.toml");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let parsed: HostsFile = toml::from_str(&raw).map_err(|e| format!("bad hosts.toml: {e}"))?;
    Ok(parsed.host)
}
