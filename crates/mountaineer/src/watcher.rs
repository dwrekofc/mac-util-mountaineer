use anyhow::Result;
use std::time::Duration;

use crate::{config, discovery, mount};

/// Run the watch loop: auto-mount favorites and remount on network changes.
pub fn run() -> Result<()> {
    let poll_interval = Duration::from_secs(30);

    println!(
        "[{}] Starting watch mode ({}s poll interval)",
        timestamp(),
        poll_interval.as_secs()
    );

    // Start network change monitor
    let network_rx = crate::network::monitor::start();

    // Initial mount cycle
    mount_cycle()?;

    loop {
        // Wait for either a network event or poll timeout
        match network_rx.recv_timeout(poll_interval) {
            Ok(event) => {
                log::debug!("Network change: {:?}", event.changed_keys);
                // Debounce: drain additional events
                std::thread::sleep(Duration::from_millis(500));
                while network_rx.try_recv().is_ok() {}

                println!("[{}] Network change detected — checking favorites...", timestamp());
                mount_cycle()?;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                mount_cycle()?;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("[{}] Network monitor disconnected, exiting", timestamp());
                break;
            }
        }
    }

    Ok(())
}

fn mount_cycle() -> Result<()> {
    let cfg = config::load()?;
    if cfg.favorites.is_empty() {
        return Ok(());
    }

    let mounted = discovery::discover_mounted_shares();

    for fav in &cfg.favorites {
        let already_mounted = mounted.iter().any(|m| {
            m.share.eq_ignore_ascii_case(&fav.share)
                && m.server.eq_ignore_ascii_case(&fav.server)
        });

        if already_mounted {
            // Find connection info for logging
            if let Some(m) = mounted.iter().find(|m| m.share.eq_ignore_ascii_case(&fav.share)) {
                let iface = match (&m.interface, &m.interface_label) {
                    (Some(i), Some(l)) => format!("{} ({})", i, l),
                    (Some(i), None) => i.clone(),
                    _ => "unknown".to_string(),
                };
                log::debug!("{}: mounted on {}", fav.share, iface);
            }
            continue;
        }

        // Not mounted — check if server is reachable
        if discovery::is_server_reachable(&fav.server) {
            println!("[{}] {}: server back online — mounting...", timestamp(), fav.share);
            match mount::smb::mount_favorite(fav) {
                Ok(()) => {
                    println!("[{}] {}: mounted at {}", timestamp(), fav.share, fav.mount_point);
                }
                Err(e) => {
                    eprintln!("[{}] {}: mount failed — {}", timestamp(), fav.share, e);
                }
            }
        } else {
            log::debug!("{}: offline — server unreachable", fav.share);
        }
    }

    Ok(())
}

fn timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}
