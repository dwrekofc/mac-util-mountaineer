use std::collections::HashMap;
use std::fmt;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use gpui::Global;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::network::InterfaceType;

// ---------------------------------------------------------------------------
// DriveId
// ---------------------------------------------------------------------------

/// Unique identifier for a configured drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DriveId(pub Uuid);

impl DriveId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for DriveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// DriveConfig
// ---------------------------------------------------------------------------

/// User-defined configuration for a single SMB drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveConfig {
    pub id: DriveId,
    pub label: String,
    /// Hostname resolved via mDNS or user-provided.
    pub server_hostname: String,
    /// Direct Ethernet IP for the server (optional, for failover).
    pub server_ethernet_ip: Option<Ipv4Addr>,
    pub share_name: String,
    /// Username for SMB authentication (password stored in Keychain).
    pub username: String,
    /// Where to mount, e.g. /Volumes/MyShare.
    pub mount_point: PathBuf,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// DriveStatus
// ---------------------------------------------------------------------------

/// Runtime status of a managed drive.
#[derive(Debug, Clone, PartialEq)]
pub enum DriveStatus {
    /// Not mounted, no connection attempt in progress.
    Disconnected,
    /// Mount command issued, waiting for result.
    Mounting,
    /// Successfully mounted via the given interface and IP.
    Connected {
        via: InterfaceType,
        ip: Ipv4Addr,
    },
    /// Switching from one interface to another (unmount + remount in flight).
    Reconnecting {
        from: InterfaceType,
        to: InterfaceType,
    },
    /// Mount or unmount failed with an error message.
    Error(String),
}

impl fmt::Display for DriveStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveStatus::Disconnected => write!(f, "Disconnected"),
            DriveStatus::Mounting => write!(f, "Mounting…"),
            DriveStatus::Connected { via, ip } => write!(f, "Connected via {} ({})", via, ip),
            DriveStatus::Reconnecting { from, to } => {
                write!(f, "Reconnecting {} → {}", from, to)
            }
            DriveStatus::Error(msg) => write!(f, "Error: {}", msg),
        }
    }
}

// ---------------------------------------------------------------------------
// AdhocMount
// ---------------------------------------------------------------------------

/// A one-off mount discovered or manually triggered (not persisted in config).
#[derive(Debug, Clone)]
pub struct AdhocMount {
    pub host: String,
    pub share: String,
    pub mount_point: PathBuf,
    pub via: InterfaceType,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

/// Central application state, registered as a GPUI Global.
///
/// Access via `cx.global::<AppState>()` (read) or
/// `cx.global_mut::<AppState>()` (write).
pub struct AppState {
    /// Configured drives (from TOML config), keyed by DriveId.
    pub drives: HashMap<DriveId, DriveConfig>,
    /// Runtime status for each configured drive.
    pub drive_statuses: HashMap<DriveId, DriveStatus>,
    /// One-off mounts not persisted in config.
    pub adhoc_mounts: Vec<AdhocMount>,
    /// Temporary in-memory password store until Keychain integration (bd-r83).
    pub passwords: HashMap<DriveId, String>,
}

impl Global for AppState {}

impl AppState {
    pub fn new() -> Self {
        Self {
            drives: HashMap::new(),
            drive_statuses: HashMap::new(),
            adhoc_mounts: Vec::new(),
            passwords: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_id_uniqueness() {
        let a = DriveId::new();
        let b = DriveId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn drive_status_display() {
        assert_eq!(DriveStatus::Disconnected.to_string(), "Disconnected");
        assert_eq!(DriveStatus::Mounting.to_string(), "Mounting…");
        assert_eq!(
            DriveStatus::Connected {
                via: InterfaceType::Ethernet,
                ip: "10.0.0.1".parse().unwrap(),
            }
            .to_string(),
            "Connected via Ethernet (10.0.0.1)"
        );
        assert_eq!(
            DriveStatus::Reconnecting {
                from: InterfaceType::WiFi,
                to: InterfaceType::Ethernet,
            }
            .to_string(),
            "Reconnecting WiFi → Ethernet"
        );
        assert_eq!(
            DriveStatus::Error("timeout".into()).to_string(),
            "Error: timeout"
        );
    }

    #[test]
    fn app_state_new_is_empty() {
        let state = AppState::new();
        assert!(state.drives.is_empty());
        assert!(state.drive_statuses.is_empty());
        assert!(state.adhoc_mounts.is_empty());
    }

    #[test]
    fn drive_config_roundtrip_serde() {
        let config = DriveConfig {
            id: DriveId::new(),
            label: "NAS".into(),
            server_hostname: "nas.local".into(),
            server_ethernet_ip: Some("10.0.0.5".parse().unwrap()),
            share_name: "shared".into(),
            username: "alice".into(),
            mount_point: PathBuf::from("/Volumes/NAS"),
            enabled: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: DriveConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, config.id);
        assert_eq!(restored.label, "NAS");
        assert_eq!(restored.server_ethernet_ip, Some("10.0.0.5".parse().unwrap()));
    }
}
