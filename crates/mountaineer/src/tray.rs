use std::collections::HashSet;
use std::time::Duration;

use gpui::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

use crate::{config, discovery, mount, wol};

/// Per-favorite status used to build the tray menu.
#[derive(Clone, Debug, PartialEq)]
struct FavoriteStatus {
    share: String,
    server: String,
    mount_point: String,
    connected: bool,
    mac_address: Option<String>,
}

/// Build a snapshot of favorite statuses by checking mounts and reachability.
fn snapshot_statuses() -> Vec<FavoriteStatus> {
    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            return Vec::new();
        }
    };

    let mounted = discovery::discover_mounted_shares();

    cfg.favorites
        .iter()
        .map(|fav| {
            let connected = mounted.iter().any(|m| {
                m.share.eq_ignore_ascii_case(&fav.share)
                    && m.server.eq_ignore_ascii_case(&fav.server)
            });

            FavoriteStatus {
                share: fav.share.clone(),
                server: fav.server.clone(),
                mount_point: fav.mount_point.clone(),
                connected,
                mac_address: fav.mac_address.clone(),
            }
        })
        .collect()
}

/// Build the tray menu from current favorite statuses.
fn build_menu(statuses: &[FavoriteStatus]) -> Menu {
    let menu = Menu::new();

    // Title item (disabled)
    let title = MenuItem::with_id("title", "Mountaineer", false, None);
    let _ = menu.append(&title);
    let _ = menu.append(&PredefinedMenuItem::separator());

    if statuses.is_empty() {
        let empty = MenuItem::with_id("empty", "No favorites configured", false, None);
        let _ = menu.append(&empty);
    } else {
        for status in statuses {
            let icon = if status.connected { "●" } else { "○" };
            let state = if status.connected {
                "Connected"
            } else {
                "Offline"
            };
            let label = format!("{}  {}   {}", icon, status.share, state);
            let id = format!("open-{}", status.share);
            // Only clickable if connected (opens Finder at mount point)
            let item = MenuItem::with_id(id, label, status.connected, None);
            let _ = menu.append(&item);
        }
    }

    let _ = menu.append(&PredefinedMenuItem::separator());

    let mount_all = MenuItem::with_id("mount-all", "Mount All Favorites", true, None);
    let _ = menu.append(&mount_all);

    let wake_all = MenuItem::with_id("wake-all", "Wake All Servers", true, None);
    let _ = menu.append(&wake_all);

    let _ = menu.append(&PredefinedMenuItem::separator());

    let quit = MenuItem::with_id("quit", "Quit Mountaineer", true, None);
    let _ = menu.append(&quit);

    menu
}

/// Create a simple 16x16 mountain icon (white on transparent).
fn make_icon() -> Icon {
    let size = 16u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    // Draw a simple mountain triangle
    // Peak at (8, 2), base from (1, 14) to (15, 14)
    for y in 2..=14u32 {
        let progress = (y - 2) as f32 / 12.0;
        let half_width = (progress * 7.0) as u32;
        let cx = 8u32;
        let left = cx.saturating_sub(half_width);
        let right = (cx + half_width).min(size - 1);

        for x in left..=right {
            let idx = ((y * size + x) * 4) as usize;
            rgba[idx] = 255; // R
            rgba[idx + 1] = 255; // G
            rgba[idx + 2] = 255; // B
            rgba[idx + 3] = 255; // A
        }
    }

    Icon::from_rgba(rgba, size, size).expect("failed to create tray icon")
}

/// Install the tray icon and start background event/status loops.
pub fn install(cx: &mut App) {
    let statuses = snapshot_statuses();
    let menu = build_menu(&statuses);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Mountaineer")
        .with_icon(make_icon())
        .build()
        .expect("failed to build tray icon");

    // Start network monitor on background thread, get receiver
    let network_rx = crate::network::monitor::start();

    // Single GPUI async task owns the tray icon (TrayIcon is !Send, must stay on main thread)
    cx.spawn(async move |cx: &mut AsyncApp| {
        let menu_receiver = MenuEvent::receiver();
        let mut prev_statuses: Vec<FavoriteStatus> = Vec::new();
        let mut last_status_check = std::time::Instant::now();
        let status_interval = Duration::from_secs(30);

        loop {
            // Check menu events (non-blocking)
            while let Ok(event) = menu_receiver.try_recv() {
                let id = event.id().0.as_str().to_string();
                handle_menu_event(&id);

                if id == "quit" {
                    let _ = cx.update(|cx| cx.quit());
                    return;
                }

                // Refresh after action (small delay for mount to complete)
                cx.background_executor()
                    .timer(Duration::from_secs(3))
                    .await;
                let statuses = snapshot_statuses();
                tray.set_menu(Some(Box::new(build_menu(&statuses))));
                prev_statuses = statuses;
            }

            // Check network events (non-blocking)
            let mut network_changed = false;
            while network_rx.try_recv().is_ok() {
                network_changed = true;
            }

            // Periodic status check or network-triggered refresh
            let now = std::time::Instant::now();
            if network_changed || now.duration_since(last_status_check) >= status_interval {
                let new_statuses = snapshot_statuses();
                if new_statuses != prev_statuses {
                    log::debug!("Status changed, refreshing tray menu");
                    tray.set_menu(Some(Box::new(build_menu(&new_statuses))));
                    prev_statuses = new_statuses;
                }
                last_status_check = now;
            }

            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
        }
    })
    .detach();

    log::info!("Tray icon installed");
}

/// Handle a menu event by ID.
fn handle_menu_event(id: &str) {
    match id {
        "mount-all" => {
            std::thread::spawn(|| {
                if let Err(e) = mount_all_favorites() {
                    log::error!("Mount all failed: {}", e);
                }
            });
        }
        "wake-all" => {
            std::thread::spawn(|| {
                if let Err(e) = wake_all_servers() {
                    log::error!("Wake all failed: {}", e);
                }
            });
        }
        "quit" => {
            // Handled in the polling loop
        }
        id if id.starts_with("open-") => {
            let share_name = &id[5..];
            open_share_in_finder(share_name);
        }
        _ => {}
    }
}

/// Mount all unmounted favorites.
fn mount_all_favorites() -> anyhow::Result<()> {
    let cfg = config::load()?;
    let mounted = discovery::discover_mounted_shares();

    for fav in &cfg.favorites {
        let already = mounted.iter().any(|m| {
            m.share.eq_ignore_ascii_case(&fav.share)
                && m.server.eq_ignore_ascii_case(&fav.server)
        });

        if !already {
            log::info!("Mounting {}...", fav.share);
            if let Err(e) = mount::smb::mount_favorite(fav) {
                log::error!("Failed to mount {}: {}", fav.share, e);
            }
        }
    }

    Ok(())
}

/// Send Wake-on-LAN to all offline servers with known MAC addresses.
fn wake_all_servers() -> anyhow::Result<()> {
    let cfg = config::load()?;

    let unique_servers: HashSet<(String, String)> = cfg
        .favorites
        .iter()
        .filter_map(|f| {
            f.mac_address
                .as_ref()
                .map(|mac| (f.server.clone(), mac.clone()))
        })
        .collect();

    for (server, mac) in unique_servers {
        if !discovery::is_server_reachable(&server) {
            log::info!("Sending WoL to {} ({})", server, mac);
            if let Err(e) = wol::send_wol(&mac) {
                log::error!("WoL failed for {}: {}", server, e);
            }
        }
    }

    Ok(())
}

/// Open a connected share's mount point in Finder.
fn open_share_in_finder(share_name: &str) {
    let cfg = match config::load() {
        Ok(c) => c,
        Err(_) => return,
    };

    if let Some(fav) = cfg
        .favorites
        .iter()
        .find(|f| f.share.eq_ignore_ascii_case(share_name))
    {
        let _ = std::process::Command::new("open")
            .arg(&fav.mount_point)
            .spawn();
    }
}
