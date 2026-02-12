use std::net::Ipv4Addr;

use crate::app_state::{AppState, DriveConfig, DriveId, DriveStatus};
use crate::mount::smb::{self, MountParams};
use crate::network::{InterfaceType, NetworkInterface};

// ---------------------------------------------------------------------------
// ReconcileAction
// ---------------------------------------------------------------------------

/// The reconciler's decision for a single drive.
#[derive(Debug, Clone, PartialEq)]
pub enum ReconcileAction {
    /// Nothing to do — already optimal or operation in flight.
    NoOp,
    /// Mount the drive on the given interface.
    Mount {
        server: String,
        interface_type: InterfaceType,
        interface_ip: Ipv4Addr,
    },
    /// Unmount the currently mounted drive.
    Unmount,
    /// Unmount and remount on a better/different interface.
    Remount {
        server: String,
        from: InterfaceType,
        to: InterfaceType,
        interface_ip: Ipv4Addr,
    },
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Pick the best available network interface (Ethernet preferred over WiFi).
///
/// The input `interfaces` is expected to be sorted Ethernet-first (as returned
/// by `enumerate_interfaces()`). Returns the first interface with an IPv4 address.
fn best_interface(interfaces: &[NetworkInterface]) -> Option<&NetworkInterface> {
    interfaces.iter().find(|i| !i.ipv4_addresses.is_empty())
}

/// Determine the server address to use for a given interface type.
///
/// If the drive has a `server_ethernet_ip` and we're connecting via Ethernet,
/// use the direct IP. Otherwise fall back to the hostname (mDNS or user-provided).
fn server_address(config: &DriveConfig, iface_type: InterfaceType) -> String {
    if iface_type == InterfaceType::Ethernet {
        if let Some(ip) = config.server_ethernet_ip {
            return ip.to_string();
        }
    }
    config.server_hostname.clone()
}

/// Check if an interface of the given type is currently available with IPv4.
fn has_interface_type(interfaces: &[NetworkInterface], iface_type: InterfaceType) -> bool {
    interfaces
        .iter()
        .any(|i| i.interface_type == iface_type && !i.ipv4_addresses.is_empty())
}

// ---------------------------------------------------------------------------
// plan_reconcile — pure decision logic
// ---------------------------------------------------------------------------

/// Determine what action to take for a single drive given its current status
/// and the available network interfaces.
///
/// This function is pure — it performs no I/O and makes no state changes.
/// Call `reconcile_drive` to plan AND execute.
pub fn plan_reconcile(
    config: &DriveConfig,
    status: &DriveStatus,
    interfaces: &[NetworkInterface],
) -> ReconcileAction {
    // Disabled drives should be unmounted if currently connected.
    if !config.enabled {
        return match status {
            DriveStatus::Connected { .. } | DriveStatus::Mounting => ReconcileAction::Unmount,
            _ => ReconcileAction::NoOp,
        };
    }

    // Find the best available interface.
    let best = match best_interface(interfaces) {
        Some(iface) => iface,
        None => {
            // No usable interfaces — unmount if connected, otherwise nothing to do.
            return match status {
                DriveStatus::Connected { .. } => ReconcileAction::Unmount,
                _ => ReconcileAction::NoOp,
            };
        }
    };

    let best_type = best.interface_type;
    // Safe to index [0]: best_interface() guarantees non-empty ipv4_addresses.
    let best_ip = best.ipv4_addresses[0];
    let server = server_address(config, best_type);

    match status {
        // Not connected — mount on the best interface.
        DriveStatus::Disconnected | DriveStatus::Error(_) => ReconcileAction::Mount {
            server,
            interface_type: best_type,
            interface_ip: best_ip,
        },

        DriveStatus::Connected { via, .. } => {
            if best_type.cmp_priority() < via.cmp_priority() {
                // A higher-priority interface came up (e.g., Ethernet while on WiFi).
                ReconcileAction::Remount {
                    server,
                    from: *via,
                    to: best_type,
                    interface_ip: best_ip,
                }
            } else if !has_interface_type(interfaces, *via) {
                // Current interface went down — fail over to whatever's available.
                ReconcileAction::Remount {
                    server,
                    from: *via,
                    to: best_type,
                    interface_ip: best_ip,
                }
            } else {
                // Already on the best (or equivalent) interface.
                ReconcileAction::NoOp
            }
        }

        // An operation is already in flight — don't interfere.
        DriveStatus::Mounting | DriveStatus::Reconnecting { .. } => ReconcileAction::NoOp,
    }
}

// ---------------------------------------------------------------------------
// reconcile_drive — plan + execute
// ---------------------------------------------------------------------------

/// Reconcile a single drive: decide what to do, then do it.
///
/// Returns the new `DriveStatus` after executing the action.
/// The `password` parameter will come from Keychain once that module is implemented.
pub fn reconcile_drive(
    config: &DriveConfig,
    status: &DriveStatus,
    interfaces: &[NetworkInterface],
    password: &str,
) -> DriveStatus {
    let action = plan_reconcile(config, status, interfaces);

    match action {
        ReconcileAction::NoOp => status.clone(),

        ReconcileAction::Mount {
            server,
            interface_type,
            interface_ip,
        } => {
            log::info!(
                "[{}] Mounting via {} (server: {}) at {}",
                config.label,
                interface_type,
                server,
                config.mount_point.display(),
            );

            let params = MountParams {
                server: &server,
                share: &config.share_name,
                username: &config.username,
                password,
                mount_point: &config.mount_point,
            };

            match smb::mount(&params) {
                Ok(()) => {
                    log::info!("[{}] Mount succeeded via {}", config.label, interface_type);
                    DriveStatus::Connected {
                        via: interface_type,
                        ip: interface_ip,
                    }
                }
                Err(e) => {
                    log::error!("[{}] Mount failed: {}", config.label, e);
                    DriveStatus::Error(e.to_string())
                }
            }
        }

        ReconcileAction::Unmount => {
            log::info!(
                "[{}] Unmounting {}",
                config.label,
                config.mount_point.display(),
            );

            match smb::unmount(&config.mount_point) {
                Ok(()) => DriveStatus::Disconnected,
                Err(e) => {
                    log::error!("[{}] Unmount failed: {}", config.label, e);
                    DriveStatus::Error(e.to_string())
                }
            }
        }

        ReconcileAction::Remount {
            server,
            from,
            to,
            interface_ip,
        } => {
            log::info!(
                "[{}] Remounting: {} → {} (server: {})",
                config.label,
                from,
                to,
                server,
            );

            // Step 1: Unmount from current interface.
            if let Err(e) = smb::unmount(&config.mount_point) {
                log::error!("[{}] Unmount during remount failed: {}", config.label, e);
                return DriveStatus::Error(e.to_string());
            }

            // Step 2: Mount on the new interface.
            let params = MountParams {
                server: &server,
                share: &config.share_name,
                username: &config.username,
                password,
                mount_point: &config.mount_point,
            };

            match smb::mount(&params) {
                Ok(()) => {
                    log::info!("[{}] Remount succeeded via {}", config.label, to);
                    DriveStatus::Connected {
                        via: to,
                        ip: interface_ip,
                    }
                }
                Err(e) => {
                    log::error!("[{}] Remount failed: {}", config.label, e);
                    DriveStatus::Error(e.to_string())
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// reconcile_all — iterate all drives
// ---------------------------------------------------------------------------

/// Reconcile all enabled drives against the current network interfaces.
///
/// For each configured drive, determines the optimal action and executes it,
/// updating `AppState.drive_statuses` with the result.
pub fn reconcile_all(state: &mut AppState, interfaces: &[NetworkInterface]) {
    let drive_ids: Vec<DriveId> = state.drives.keys().copied().collect();

    for id in drive_ids {
        let config = match state.drives.get(&id) {
            Some(c) if c.enabled => c.clone(),
            _ => continue,
        };

        let current_status = state
            .drive_statuses
            .get(&id)
            .cloned()
            .unwrap_or(DriveStatus::Disconnected);

        // Use in-memory password store until Keychain integration (bd-r83).
        let empty = String::new();
        let password = state.passwords.get(&id).unwrap_or(&empty);

        let new_status = reconcile_drive(&config, &current_status, interfaces, password);

        if new_status != current_status {
            log::info!("[{}] {} → {}", config.label, current_status, new_status);
            state.drive_statuses.insert(id, new_status);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::DriveId;
    use std::path::PathBuf;

    fn test_config() -> DriveConfig {
        DriveConfig {
            id: DriveId::new(),
            label: "TestNAS".into(),
            server_hostname: "nas.local".into(),
            server_ethernet_ip: Some("10.0.0.5".parse().unwrap()),
            share_name: "shared".into(),
            username: "alice".into(),
            mount_point: PathBuf::from("/Volumes/TestNAS"),
            enabled: true,
        }
    }

    fn ethernet_interface() -> NetworkInterface {
        NetworkInterface {
            name: "en5".into(),
            interface_type: InterfaceType::Ethernet,
            display_name: Some("USB 10/100/1000 LAN".into()),
            ipv4_addresses: vec!["10.0.0.100".parse().unwrap()],
            ipv6_addresses: vec![],
        }
    }

    fn wifi_interface() -> NetworkInterface {
        NetworkInterface {
            name: "en0".into(),
            interface_type: InterfaceType::WiFi,
            display_name: Some("Wi-Fi".into()),
            ipv4_addresses: vec!["192.168.1.100".parse().unwrap()],
            ipv6_addresses: vec![],
        }
    }

    // --- Mount decisions ---

    #[test]
    fn disconnected_with_ethernet_mounts_via_ethernet_ip() {
        let config = test_config();
        let interfaces = vec![ethernet_interface(), wifi_interface()];
        let action = plan_reconcile(&config, &DriveStatus::Disconnected, &interfaces);
        assert_eq!(
            action,
            ReconcileAction::Mount {
                server: "10.0.0.5".into(),
                interface_type: InterfaceType::Ethernet,
                interface_ip: "10.0.0.100".parse().unwrap(),
            }
        );
    }

    #[test]
    fn disconnected_with_only_wifi_mounts_via_hostname() {
        let config = test_config();
        let interfaces = vec![wifi_interface()];
        let action = plan_reconcile(&config, &DriveStatus::Disconnected, &interfaces);
        assert_eq!(
            action,
            ReconcileAction::Mount {
                server: "nas.local".into(),
                interface_type: InterfaceType::WiFi,
                interface_ip: "192.168.1.100".parse().unwrap(),
            }
        );
    }

    #[test]
    fn disconnected_with_no_interfaces_is_noop() {
        let config = test_config();
        let action = plan_reconcile(&config, &DriveStatus::Disconnected, &[]);
        assert_eq!(action, ReconcileAction::NoOp);
    }

    #[test]
    fn error_state_retries_mount() {
        let config = test_config();
        let status = DriveStatus::Error("timeout".into());
        let interfaces = vec![wifi_interface()];
        let action = plan_reconcile(&config, &status, &interfaces);
        assert_eq!(
            action,
            ReconcileAction::Mount {
                server: "nas.local".into(),
                interface_type: InterfaceType::WiFi,
                interface_ip: "192.168.1.100".parse().unwrap(),
            }
        );
    }

    // --- Failback (upgrade to better interface) ---

    #[test]
    fn connected_via_wifi_with_ethernet_available_remounts() {
        let config = test_config();
        let status = DriveStatus::Connected {
            via: InterfaceType::WiFi,
            ip: "192.168.1.100".parse().unwrap(),
        };
        let interfaces = vec![ethernet_interface(), wifi_interface()];
        let action = plan_reconcile(&config, &status, &interfaces);
        assert_eq!(
            action,
            ReconcileAction::Remount {
                server: "10.0.0.5".into(),
                from: InterfaceType::WiFi,
                to: InterfaceType::Ethernet,
                interface_ip: "10.0.0.100".parse().unwrap(),
            }
        );
    }

    // --- Failover (current interface went down) ---

    #[test]
    fn connected_via_ethernet_when_ethernet_drops_fails_over_to_wifi() {
        let config = test_config();
        let status = DriveStatus::Connected {
            via: InterfaceType::Ethernet,
            ip: "10.0.0.100".parse().unwrap(),
        };
        let interfaces = vec![wifi_interface()]; // ethernet gone
        let action = plan_reconcile(&config, &status, &interfaces);
        assert_eq!(
            action,
            ReconcileAction::Remount {
                server: "nas.local".into(),
                from: InterfaceType::Ethernet,
                to: InterfaceType::WiFi,
                interface_ip: "192.168.1.100".parse().unwrap(),
            }
        );
    }

    // --- Unmount decisions ---

    #[test]
    fn connected_with_no_interfaces_unmounts() {
        let config = test_config();
        let status = DriveStatus::Connected {
            via: InterfaceType::Ethernet,
            ip: "10.0.0.100".parse().unwrap(),
        };
        let action = plan_reconcile(&config, &status, &[]);
        assert_eq!(action, ReconcileAction::Unmount);
    }

    #[test]
    fn disabled_drive_connected_unmounts() {
        let mut config = test_config();
        config.enabled = false;
        let status = DriveStatus::Connected {
            via: InterfaceType::Ethernet,
            ip: "10.0.0.100".parse().unwrap(),
        };
        let interfaces = vec![ethernet_interface()];
        let action = plan_reconcile(&config, &status, &interfaces);
        assert_eq!(action, ReconcileAction::Unmount);
    }

    // --- No-op decisions ---

    #[test]
    fn connected_via_ethernet_already_optimal_is_noop() {
        let config = test_config();
        let status = DriveStatus::Connected {
            via: InterfaceType::Ethernet,
            ip: "10.0.0.100".parse().unwrap(),
        };
        let interfaces = vec![ethernet_interface(), wifi_interface()];
        let action = plan_reconcile(&config, &status, &interfaces);
        assert_eq!(action, ReconcileAction::NoOp);
    }

    #[test]
    fn connected_via_wifi_only_wifi_available_is_noop() {
        let config = test_config();
        let status = DriveStatus::Connected {
            via: InterfaceType::WiFi,
            ip: "192.168.1.100".parse().unwrap(),
        };
        let interfaces = vec![wifi_interface()];
        let action = plan_reconcile(&config, &status, &interfaces);
        assert_eq!(action, ReconcileAction::NoOp);
    }

    #[test]
    fn disabled_drive_disconnected_is_noop() {
        let mut config = test_config();
        config.enabled = false;
        let action =
            plan_reconcile(&config, &DriveStatus::Disconnected, &[ethernet_interface()]);
        assert_eq!(action, ReconcileAction::NoOp);
    }

    #[test]
    fn mounting_in_flight_is_noop() {
        let config = test_config();
        let interfaces = vec![ethernet_interface()];
        let action = plan_reconcile(&config, &DriveStatus::Mounting, &interfaces);
        assert_eq!(action, ReconcileAction::NoOp);
    }

    #[test]
    fn reconnecting_in_flight_is_noop() {
        let config = test_config();
        let status = DriveStatus::Reconnecting {
            from: InterfaceType::WiFi,
            to: InterfaceType::Ethernet,
        };
        let interfaces = vec![ethernet_interface()];
        let action = plan_reconcile(&config, &status, &interfaces);
        assert_eq!(action, ReconcileAction::NoOp);
    }

    // --- Server address selection ---

    #[test]
    fn no_ethernet_ip_uses_hostname_on_ethernet() {
        let mut config = test_config();
        config.server_ethernet_ip = None;
        let interfaces = vec![ethernet_interface()];
        let action = plan_reconcile(&config, &DriveStatus::Disconnected, &interfaces);
        assert_eq!(
            action,
            ReconcileAction::Mount {
                server: "nas.local".into(),
                interface_type: InterfaceType::Ethernet,
                interface_ip: "10.0.0.100".parse().unwrap(),
            }
        );
    }

    #[test]
    fn wifi_always_uses_hostname() {
        let config = test_config();
        let interfaces = vec![wifi_interface()];
        let action = plan_reconcile(&config, &DriveStatus::Disconnected, &interfaces);
        assert_eq!(
            action,
            ReconcileAction::Mount {
                server: "nas.local".into(),
                interface_type: InterfaceType::WiFi,
                interface_ip: "192.168.1.100".parse().unwrap(),
            }
        );
    }

    // --- Helper functions ---

    #[test]
    fn best_interface_prefers_ethernet() {
        let interfaces = vec![ethernet_interface(), wifi_interface()];
        let best = best_interface(&interfaces).unwrap();
        assert_eq!(best.interface_type, InterfaceType::Ethernet);
    }

    #[test]
    fn best_interface_skips_no_ipv4() {
        let mut eth = ethernet_interface();
        eth.ipv4_addresses.clear();
        let interfaces = vec![eth, wifi_interface()];
        let best = best_interface(&interfaces).unwrap();
        assert_eq!(best.interface_type, InterfaceType::WiFi);
    }

    #[test]
    fn best_interface_returns_none_when_empty() {
        assert!(best_interface(&[]).is_none());
    }

    #[test]
    fn server_address_ethernet_with_ip() {
        let config = test_config();
        assert_eq!(
            server_address(&config, InterfaceType::Ethernet),
            "10.0.0.5"
        );
    }

    #[test]
    fn server_address_wifi_uses_hostname() {
        let config = test_config();
        assert_eq!(
            server_address(&config, InterfaceType::WiFi),
            "nas.local"
        );
    }

    #[test]
    fn server_address_ethernet_no_ip_falls_back() {
        let mut config = test_config();
        config.server_ethernet_ip = None;
        assert_eq!(
            server_address(&config, InterfaceType::Ethernet),
            "nas.local"
        );
    }
}
