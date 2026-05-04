//! On-disk layout for per-tunnel state.
//!
//! Centralizes every filename and directory innisfree writes inside its
//! per-service config dir. The previous design scattered string literals
//! (`"ip"`, `"known_hosts"`, `"client_id_ed25519"`) across `manager.rs`,
//! `ssh.rs`, and `wg/mod.rs`, with no compile-time guarantee the writer
//! and reader of a given file agreed on its name. Routing every path
//! through [`TunnelStateDir`] removes that bug surface and gives the
//! eventual XDG migration (move from `~/.config/innisfree/` to
//! `$XDG_STATE_HOME/innisfree/`) a single edit site.

use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

/// Handle to the on-disk state directory for a single innisfree tunnel.
///
/// Each tunnel gets its own dir under [`base_dir`], typically
/// `~/.config/innisfree/<service>/`. Use [`Self::for_service`] to create
/// the dir (called from `TunnelManager::new`) or [`Self::open`] to
/// reference an already-existing one (called from read-only subcommands
/// like `innisfree ip` and `innisfree ssh`).
#[derive(Clone, Debug)]
pub struct TunnelStateDir {
    path: PathBuf,
    /// Service name, retained because [`Self::wg_conf`] uses it in the
    /// filename (`<service>.conf`).
    name: String,
}

impl TunnelStateDir {
    /// Create-or-open the state dir. Use this from the constructor of
    /// long-lived owners (`TunnelManager::new`); idempotent if the dir
    /// already exists.
    pub fn for_service(service_name: &str) -> Result<Self> {
        let path = base_dir()?.join(service_name);
        std::fs::create_dir_all(&path)
            .with_context(|| format!("creating state dir {}", path.display()))?;
        Ok(Self {
            path,
            name: service_name.to_string(),
        })
    }

    /// Open an existing state dir without creating it. Returns an error
    /// if no such dir exists — i.e. the tunnel was never brought up. Use
    /// this for read-only paths like `innisfree ip` and `innisfree ssh`,
    /// where auto-creating an empty dir would just defer the inevitable
    /// "tunnel isn't ready" error and leave a stray directory behind.
    pub fn open(service_name: &str) -> Result<Self> {
        let path = base_dir()?.join(service_name);
        if !path.is_dir() {
            return Err(anyhow!(
                "no state for service '{service_name}' at {} — was the tunnel ever brought up?",
                path.display()
            ));
        }
        Ok(Self {
            path,
            name: service_name.to_string(),
        })
    }

    /// Marker file holding the public IPv4 address of the cloud node.
    /// Written by [`crate::manager::TunnelManager::up`] only after the
    /// tunnel is verified live, so its presence doubles as the readiness
    /// signal for `innisfree ip` (and anything polling it).
    pub fn ip_marker(&self) -> PathBuf {
        self.path.join("ip")
    }

    /// SSH `known_hosts` file pinning the remote sshd's host key.
    pub fn known_hosts(&self) -> PathBuf {
        self.path.join("known_hosts")
    }

    /// Client-side SSH private key path. The matching public key lives
    /// at the same location with `.pub` appended. (No `server_key`
    /// counterpart: the remote sshd's keypair never lives on disk
    /// locally — it travels via cloud-init.)
    pub fn client_key(&self) -> PathBuf {
        self.path.join("client_id_ed25519")
    }

    /// Rendered `wg0.conf` path for the local end of the tunnel. The
    /// filename includes the service name so multiple parallel tunnels
    /// don't collide if a future change ever flattens the layout.
    pub fn wg_conf(&self) -> PathBuf {
        self.path.join(format!("{}.conf", self.name))
    }
}

/// Recursively delete the state dir for `service_name`. Idempotent —
/// returns `Ok(())` if the dir doesn't exist. Used by the `clean`
/// subcommand and by `TunnelManager::clean` during teardown.
pub fn remove_state_for_service(service_name: &str) -> Result<()> {
    let path = base_dir()?.join(service_name);
    if path.is_dir() {
        std::fs::remove_dir_all(&path).with_context(|| format!("removing {}", path.display()))?;
    }
    Ok(())
}

/// Resolve the parent dir under which all per-tunnel state lives.
/// Currently `~/.config/innisfree/`; an `XDG_STATE_HOME` migration
/// would change just this function.
fn base_dir() -> Result<PathBuf> {
    Ok(home::home_dir()
        .ok_or_else(|| anyhow!("could not find home directory"))?
        .join(".config")
        .join("innisfree"))
}
