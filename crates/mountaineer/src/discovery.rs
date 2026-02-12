use std::collections::HashMap;
use std::process::Command;

/// A currently mounted SMB share with connection details.
#[derive(Debug, Clone)]
pub struct MountedShare {
    pub server: String,
    pub share: String,
    pub mount_point: String,
    pub interface: Option<String>,
    pub interface_label: Option<String>,
    pub smb_version: Option<String>,
}

/// Discover all currently mounted SMB shares with full connection details.
pub fn discover_mounted_shares() -> Vec<MountedShare> {
    let mounts = parse_mount_smbfs();
    if mounts.is_empty() {
        return Vec::new();
    }

    let statshares = parse_smbutil_statshares();
    let hw_ports = parse_hardware_ports();

    let mut result = Vec::new();
    for (server, share, mount_point) in mounts {
        let smb_version = statshares.get(&share).cloned();

        // Resolve server to IP, then find interface via route get
        let server_ip = resolve_hostname(&server);
        let (interface, interface_label) = if let Some(ip) = &server_ip {
            let iface = get_route_interface(ip);
            let label = iface.as_ref().and_then(|i| hw_ports.get(i)).cloned();
            (iface, label)
        } else {
            (None, None)
        };

        result.push(MountedShare {
            server,
            share,
            mount_point,
            interface,
            interface_label,
            smb_version,
        });
    }

    result
}

/// Parse `mount -t smbfs` output.
/// Returns Vec<(server, share, mount_point)>.
fn parse_mount_smbfs() -> Vec<(String, String, String)> {
    let output = match Command::new("mount").args(["-t", "smbfs"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut mounts = Vec::new();

    for line in stdout.lines() {
        // Format: //user@server/SHARE on /mount/point (smbfs, ...)
        // or:     //server/SHARE on /mount/point (smbfs, ...)
        if let Some((smb_part, rest)) = line.split_once(" on ") {
            if let Some((mount_point, _flags)) = rest.split_once(" (") {
                let path = smb_part.trim_start_matches("//");
                // Strip optional user@ prefix
                let path = if let Some((_user, after_at)) = path.split_once('@') {
                    after_at
                } else {
                    path
                };
                // Split server/share
                if let Some((server, share)) = path.split_once('/') {
                    mounts.push((
                        server.to_string(),
                        share.to_string(),
                        mount_point.to_string(),
                    ));
                }
            }
        }
    }

    mounts
}

/// Parse `smbutil statshares -a` output.
/// Returns map of share_name -> smb_version.
///
/// The format is:
/// ```text
/// ==== (header line)
/// SHARE                         ATTRIBUTE TYPE                VALUE
/// ==== (header line)
/// CORE-01
///                               SMB_VERSION                   SMB_3.0.2
/// ---- (separator)
/// VAULT-R1
///                               SMB_VERSION                   SMB_3.0.2
/// ```
fn parse_smbutil_statshares() -> HashMap<String, String> {
    let output = match Command::new("smbutil")
        .args(["statshares", "-a"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return HashMap::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result = HashMap::new();
    let mut current_share: Option<String> = None;

    for line in stdout.lines() {
        // Skip separator/header lines
        if line.starts_with("===") || line.starts_with("---") {
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Skip the header row
        if trimmed.starts_with("SHARE") && trimmed.contains("ATTRIBUTE") {
            continue;
        }

        // If line doesn't start with whitespace, it's a share name
        if !line.starts_with(' ') && !line.starts_with('\t') {
            current_share = Some(trimmed.to_string());
        } else {
            // Attribute line — split on multiple whitespace
            let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
            if parts.len() == 2 {
                let key = parts[0].trim();
                let value = parts[1].trim();
                if key == "SMB_VERSION" {
                    if let Some(ref share) = current_share {
                        // Clean up the version string (e.g., "SMB_3.0.2" -> "SMB 3.0.2")
                        let clean = value.replace('_', " ");
                        result.insert(share.clone(), clean);
                    }
                }
            }
        }
    }

    result
}

/// Resolve a hostname to an IP address using `dscacheutil -q host`.
fn resolve_hostname(hostname: &str) -> Option<String> {
    // If it's already an IP, return it
    if hostname.parse::<std::net::Ipv4Addr>().is_ok() {
        return Some(hostname.to_string());
    }

    let output = Command::new("dscacheutil")
        .args(["-q", "host", "-a", "name", hostname])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(ip) = trimmed.strip_prefix("ip_address:") {
            let ip = ip.trim();
            // Prefer IPv4
            if ip.parse::<std::net::Ipv4Addr>().is_ok() {
                return Some(ip.to_string());
            }
        }
    }

    None
}

/// Run `route get <ip>` and extract the interface name.
fn get_route_interface(ip: &str) -> Option<String> {
    let output = Command::new("route")
        .args(["get", ip])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(iface) = trimmed.strip_prefix("interface:") {
            return Some(iface.trim().to_string());
        }
    }

    None
}

/// Parse `networksetup -listallhardwareports` to build interface_name -> label map.
fn parse_hardware_ports() -> HashMap<String, String> {
    let output = match Command::new("networksetup")
        .args(["-listallhardwareports"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return HashMap::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result = HashMap::new();
    let mut current_port: Option<String> = None;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(port) = trimmed.strip_prefix("Hardware Port:") {
            current_port = Some(port.trim().to_string());
        } else if let Some(device) = trimmed.strip_prefix("Device:") {
            let device = device.trim().to_string();
            if let Some(port) = current_port.take() {
                result.insert(device, port);
            }
        }
    }

    result
}

/// Discover the MAC address for a server by checking the ARP table.
pub fn discover_mac_address(server: &str) -> Option<String> {
    // First resolve hostname to IP
    let ip = resolve_hostname(server)?;

    // Ping once to ensure ARP entry exists
    let _ = Command::new("ping")
        .args(["-c", "1", "-W", "1", &ip])
        .output();

    let output = Command::new("arp").args(["-a"]).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains(&format!("({})", ip)) {
            // Format: ? (192.168.1.1) at d0:11:e5:13:af:1f on en0 ifscope [ethernet]
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[2] == "at" {
                let mac = parts[3];
                // Filter out incomplete entries
                if mac != "(incomplete)" && mac.contains(':') {
                    return Some(mac.to_string());
                }
            }
        }
    }

    None
}

/// Check if a server is reachable via ping.
pub fn is_server_reachable(server: &str) -> bool {
    Command::new("ping")
        .args(["-c", "1", "-W", "1", server])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mount_smbfs_output_format() {
        // Just verify the function runs without panic
        let mounts = parse_mount_smbfs();
        for (server, share, mount_point) in &mounts {
            assert!(!server.is_empty());
            assert!(!share.is_empty());
            assert!(mount_point.starts_with('/'));
        }
    }

    #[test]
    fn hardware_ports_returns_map() {
        let ports = parse_hardware_ports();
        // Should have at least en0 on any Mac
        assert!(
            !ports.is_empty(),
            "expected at least one hardware port mapping"
        );
    }

    #[test]
    fn resolve_ip_passthrough() {
        assert_eq!(resolve_hostname("10.0.0.1"), Some("10.0.0.1".to_string()));
    }
}
