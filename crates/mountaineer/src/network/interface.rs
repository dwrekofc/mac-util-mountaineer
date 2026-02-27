use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use nix::ifaddrs::getifaddrs;
use system_configuration::network_configuration::{SCNetworkInterfaceType, get_interfaces};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InterfaceType {
    Ethernet,
    WiFi,
    Other,
}

impl std::fmt::Display for InterfaceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterfaceType::Ethernet => write!(f, "Ethernet"),
            InterfaceType::WiFi => write!(f, "WiFi"),
            InterfaceType::Other => write!(f, "Other"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub interface_type: InterfaceType,
    pub display_name: Option<String>,
    pub ipv4_addresses: Vec<Ipv4Addr>,
    pub ipv6_addresses: Vec<Ipv6Addr>,
}

impl NetworkInterface {
    /// Returns true if this interface has at least one IP address assigned.
    pub fn is_active(&self) -> bool {
        !self.ipv4_addresses.is_empty() || !self.ipv6_addresses.is_empty()
    }
}

impl std::fmt::Display for NetworkInterface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.interface_type)?;
        for ip in &self.ipv4_addresses {
            write!(f, " {}", ip)?;
        }
        Ok(())
    }
}

/// Enumerate all active network interfaces, classified by type, with IP addresses.
///
/// Uses macOS SystemConfiguration framework for type detection and nix getifaddrs
/// for IP address retrieval. Only returns interfaces that are Ethernet or WiFi
/// and have at least one IP address.
pub fn enumerate_interfaces() -> Vec<NetworkInterface> {
    // Step 1: Build a map of BSD name -> (InterfaceType, display_name) from SystemConfiguration
    let mut type_map: HashMap<String, (InterfaceType, Option<String>)> = HashMap::new();

    let sc_interfaces = get_interfaces();
    for iface in sc_interfaces.iter() {
        let bsd_name = match iface.bsd_name() {
            Some(name) => name.to_string(),
            None => continue,
        };

        let if_type = match iface.interface_type() {
            Some(SCNetworkInterfaceType::Ethernet) => InterfaceType::Ethernet,
            Some(SCNetworkInterfaceType::IEEE80211) => InterfaceType::WiFi,
            _ => InterfaceType::Other,
        };

        let display_name = iface.display_name().map(|s| s.to_string());
        type_map.insert(bsd_name, (if_type, display_name));
    }

    // Step 2: Collect IP addresses per interface name from getifaddrs
    let mut ipv4_map: HashMap<String, Vec<Ipv4Addr>> = HashMap::new();
    let mut ipv6_map: HashMap<String, Vec<Ipv6Addr>> = HashMap::new();

    if let Ok(addrs) = getifaddrs() {
        for addr in addrs {
            let name = addr.interface_name.clone();
            if let Some(storage) = addr.address {
                if let Some(sin) = storage.as_sockaddr_in() {
                    ipv4_map.entry(name).or_default().push(sin.ip());
                } else if let Some(sin6) = storage.as_sockaddr_in6() {
                    ipv6_map.entry(name).or_default().push(sin6.ip());
                }
            }
        }
    }

    // Step 3: Combine into NetworkInterface structs, filtering to Ethernet/WiFi with IPs
    let mut result: Vec<NetworkInterface> = Vec::new();

    for (name, (if_type, display_name)) in &type_map {
        if *if_type == InterfaceType::Other {
            continue;
        }

        let ipv4 = ipv4_map.remove(name).unwrap_or_default();
        let ipv6 = ipv6_map.remove(name).unwrap_or_default();

        if ipv4.is_empty() && ipv6.is_empty() {
            continue;
        }

        result.push(NetworkInterface {
            name: name.clone(),
            interface_type: *if_type,
            display_name: display_name.clone(),
            ipv4_addresses: ipv4,
            ipv6_addresses: ipv6,
        });
    }

    // Sort: Ethernet first, then WiFi; within each type, by name
    result.sort_by(|a, b| {
        a.interface_type
            .cmp_priority()
            .cmp(&b.interface_type.cmp_priority())
            .then_with(|| a.name.cmp(&b.name))
    });

    result
}

impl InterfaceType {
    /// Priority for sorting: lower = higher priority.
    pub(crate) fn cmp_priority(&self) -> u8 {
        match self {
            InterfaceType::Ethernet => 0,
            InterfaceType::WiFi => 1,
            InterfaceType::Other => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_returns_only_ethernet_and_wifi() {
        let interfaces = enumerate_interfaces();
        for iface in &interfaces {
            assert!(
                iface.interface_type == InterfaceType::Ethernet
                    || iface.interface_type == InterfaceType::WiFi,
                "unexpected type {:?} for {}",
                iface.interface_type,
                iface.name
            );
        }
    }

    #[test]
    fn enumerate_active_interfaces_have_ips() {
        let interfaces = enumerate_interfaces();
        for iface in &interfaces {
            assert!(
                iface.is_active(),
                "interface {} has no IPs but was returned",
                iface.name
            );
        }
    }

    #[test]
    fn enumerate_returns_at_least_one_interface() {
        // On any dev machine, we should have at least one active network interface
        let interfaces = enumerate_interfaces();
        assert!(
            !interfaces.is_empty(),
            "expected at least one active network interface"
        );
    }

    #[test]
    fn ethernet_sorted_before_wifi() {
        let interfaces = enumerate_interfaces();
        let mut seen_wifi = false;
        for iface in &interfaces {
            if iface.interface_type == InterfaceType::WiFi {
                seen_wifi = true;
            }
            if iface.interface_type == InterfaceType::Ethernet && seen_wifi {
                panic!("Ethernet interface {} appeared after WiFi", iface.name);
            }
        }
    }

    #[test]
    fn display_format_includes_type() {
        let iface = NetworkInterface {
            name: "en0".into(),
            interface_type: InterfaceType::WiFi,
            display_name: Some("Wi-Fi".into()),
            ipv4_addresses: vec!["192.168.1.100".parse().unwrap()],
            ipv6_addresses: vec![],
        };
        let s = format!("{}", iface);
        assert!(s.contains("WiFi"));
        assert!(s.contains("en0"));
        assert!(s.contains("192.168.1.100"));
    }
}
