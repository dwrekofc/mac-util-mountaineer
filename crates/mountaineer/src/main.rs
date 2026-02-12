#![allow(unexpected_cfgs)]

use anyhow::Result;
use clap::Parser;

mod cli;
mod config;
mod discovery;
mod gui;
mod mount;
mod network;
mod tray;
mod watcher;
mod wol;

use cli::{Cli, Command};

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        None => {
            gui::run();
            Ok(())
        }
        Some(cmd) => run_cli(cmd),
    }
}

fn run_cli(command: Command) -> Result<()> {
    match command {
        Command::List => cmd_list(),
        Command::Favorites => cmd_favorites(),
        Command::Add { share, mac } => cmd_add(&share, mac),
        Command::Remove { share } => cmd_remove(&share),
        Command::Mount { share } => cmd_mount(share.as_deref()),
        Command::Unmount { share } => cmd_unmount(&share),
        Command::Status => cmd_status(),
        Command::Wake { share } => cmd_wake(&share),
        Command::Watch => cmd_watch(),
    }
}

fn cmd_list() -> Result<()> {
    let shares = discovery::discover_mounted_shares();
    if shares.is_empty() {
        println!("No SMB shares currently mounted.");
        return Ok(());
    }

    // Print header
    println!(
        "{:<14} {:<20} {:<24} {:<30} {}",
        "SHARE", "SERVER", "MOUNT POINT", "INTERFACE", "SMB VERSION"
    );

    for s in &shares {
        let iface_str = match (&s.interface, &s.interface_label) {
            (Some(iface), Some(label)) => format!("{} ({})", iface, label),
            (Some(iface), None) => iface.clone(),
            _ => "—".to_string(),
        };
        let smb_ver = s.smb_version.as_deref().unwrap_or("—");

        println!(
            "{:<14} {:<20} {:<24} {:<30} {}",
            s.share, s.server, s.mount_point, iface_str, smb_ver
        );
    }

    Ok(())
}

fn cmd_favorites() -> Result<()> {
    let cfg = config::load()?;
    if cfg.favorites.is_empty() {
        println!("No favorites configured. Use 'mountaineer add <SHARE>' to add one.");
        return Ok(());
    }

    let mounted = discovery::discover_mounted_shares();

    println!(
        "{:<14} {:<20} {:<24} {}",
        "SHARE", "SERVER", "MOUNT POINT", "STATUS"
    );

    for fav in &cfg.favorites {
        let is_mounted = mounted.iter().any(|m| {
            m.share.eq_ignore_ascii_case(&fav.share)
                && m.server.eq_ignore_ascii_case(&fav.server)
        });
        let status = if is_mounted { "● Mounted" } else { "○ Offline" };

        println!(
            "{:<14} {:<20} {:<24} {}",
            fav.share, fav.server, fav.mount_point, status
        );
    }

    Ok(())
}

fn cmd_add(share_name: &str, mac_override: Option<String>) -> Result<()> {
    let mut cfg = config::load()?;

    // Check if already in favorites
    if cfg
        .favorites
        .iter()
        .any(|f| f.share.eq_ignore_ascii_case(share_name))
    {
        anyhow::bail!("'{}' is already in favorites", share_name);
    }

    // Find the share in currently mounted shares
    let mounted = discovery::discover_mounted_shares();
    let found = mounted
        .iter()
        .find(|m| m.share.eq_ignore_ascii_case(share_name));

    let (server, mount_point) = match found {
        Some(m) => (m.server.clone(), m.mount_point.clone()),
        None => {
            anyhow::bail!(
                "'{}' is not currently mounted. Mount it first, then run 'mountaineer add {}'.",
                share_name,
                share_name
            );
        }
    };

    // Discover MAC address
    let mac_address = mac_override.or_else(|| {
        log::info!("Discovering MAC address for {}...", server);
        discovery::discover_mac_address(&server)
    });

    let fav = config::Favorite {
        server,
        share: share_name.to_string(),
        mount_point,
        mac_address: mac_address.clone(),
    };

    println!("Adding to favorites:");
    println!("  Share:       {}", fav.share);
    println!("  Server:      {}", fav.server);
    println!("  Mount point: {}", fav.mount_point);
    match &fav.mac_address {
        Some(mac) => println!("  MAC address: {}", mac),
        None => println!("  MAC address: (not discovered — WoL unavailable)"),
    }

    cfg.favorites.push(fav);
    config::save(&cfg)?;
    println!("Saved.");

    Ok(())
}

fn cmd_remove(share_name: &str) -> Result<()> {
    let mut cfg = config::load()?;
    let before = cfg.favorites.len();
    cfg.favorites
        .retain(|f| !f.share.eq_ignore_ascii_case(share_name));

    if cfg.favorites.len() == before {
        anyhow::bail!("'{}' not found in favorites", share_name);
    }

    config::save(&cfg)?;
    println!("Removed '{}' from favorites.", share_name);
    Ok(())
}

fn cmd_mount(share_name: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let mounted = discovery::discover_mounted_shares();

    let favorites: Vec<&config::Favorite> = match share_name {
        Some(name) => {
            let fav = cfg
                .favorites
                .iter()
                .find(|f| f.share.eq_ignore_ascii_case(name))
                .ok_or_else(|| anyhow::anyhow!("'{}' not found in favorites", name))?;
            vec![fav]
        }
        None => cfg.favorites.iter().collect(),
    };

    if favorites.is_empty() {
        println!("No favorites to mount.");
        return Ok(());
    }

    for fav in favorites {
        let already_mounted = mounted.iter().any(|m| {
            m.share.eq_ignore_ascii_case(&fav.share)
                && m.server.eq_ignore_ascii_case(&fav.server)
        });

        if already_mounted {
            println!("{}: already mounted", fav.share);
            continue;
        }

        println!("{}: mounting...", fav.share);
        match mount::smb::mount_favorite(fav) {
            Ok(()) => println!("{}: mounted at {}", fav.share, fav.mount_point),
            Err(e) => eprintln!("{}: mount failed — {}", fav.share, e),
        }
    }

    Ok(())
}

fn cmd_unmount(share_name: &str) -> Result<()> {
    let cfg = config::load()?;

    // Find in favorites for mount point
    let fav = cfg
        .favorites
        .iter()
        .find(|f| f.share.eq_ignore_ascii_case(share_name));

    let mount_point = match fav {
        Some(f) => f.mount_point.clone(),
        None => {
            // Try to find it in currently mounted shares
            let mounted = discovery::discover_mounted_shares();
            let found = mounted
                .iter()
                .find(|m| m.share.eq_ignore_ascii_case(share_name));
            match found {
                Some(m) => m.mount_point.clone(),
                None => anyhow::bail!("'{}' is not mounted and not in favorites", share_name),
            }
        }
    };

    println!("{}: unmounting...", share_name);
    mount::smb::unmount(std::path::Path::new(&mount_point))?;
    println!("{}: unmounted", share_name);
    Ok(())
}

fn cmd_status() -> Result<()> {
    let cfg = config::load()?;
    if cfg.favorites.is_empty() {
        println!("No favorites configured.");
        return Ok(());
    }

    let mounted = discovery::discover_mounted_shares();

    println!(
        "{:<14} {:<12} {:<30} {}",
        "SHARE", "STATUS", "INTERFACE", "SMB VERSION"
    );

    for fav in &cfg.favorites {
        let mount_info = mounted.iter().find(|m| {
            m.share.eq_ignore_ascii_case(&fav.share)
                && m.server.eq_ignore_ascii_case(&fav.server)
        });

        match mount_info {
            Some(m) => {
                let iface_str = match (&m.interface, &m.interface_label) {
                    (Some(iface), Some(label)) => format!("{} ({})", iface, label),
                    (Some(iface), None) => iface.clone(),
                    _ => "—".to_string(),
                };
                let smb_ver = m.smb_version.as_deref().unwrap_or("—");
                println!(
                    "{:<14} {:<12} {:<30} {}",
                    fav.share, "● Connected", iface_str, smb_ver
                );
            }
            None => {
                let reachable = discovery::is_server_reachable(&fav.server);
                let status = if reachable {
                    "○ Reachable"
                } else {
                    "✕ Offline"
                };
                println!("{:<14} {:<12} {:<30} {}", fav.share, status, "—", "—");
            }
        }
    }

    Ok(())
}

fn cmd_wake(share_name: &str) -> Result<()> {
    let cfg = config::load()?;
    let fav = cfg
        .favorites
        .iter()
        .find(|f| f.share.eq_ignore_ascii_case(share_name))
        .ok_or_else(|| anyhow::anyhow!("'{}' not found in favorites", share_name))?;

    let mac = fav
        .mac_address
        .as_ref()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No MAC address stored for '{}'. Re-add with --mac flag.",
                share_name
            )
        })?;

    println!("Sending Wake-on-LAN to {} ({})", fav.server, mac);
    wol::send_wol(mac)?;

    println!("Waiting for server to respond...");
    for i in 1..=10 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if discovery::is_server_reachable(&fav.server) {
            println!("{} is online after ~{}s", fav.server, i * 2);
            return Ok(());
        }
        print!(".");
        use std::io::Write;
        std::io::stdout().flush()?;
    }

    println!("\n{} did not respond within 20s. It may still be waking up.", fav.server);
    Ok(())
}

fn cmd_watch() -> Result<()> {
    watcher::run()
}
