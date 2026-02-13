use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
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

/// Snapshot of tray state: favorite statuses and addable mounted shares.
#[derive(Clone, Debug, PartialEq)]
struct TraySnapshot {
    favorites: Vec<FavoriteStatus>,
    /// Mounted shares not already in favorites: (share, server).
    addable: Vec<(String, String)>,
}

/// Build a snapshot of favorite statuses and addable shares in one pass.
fn snapshot() -> TraySnapshot {
    let cfg = config::load().unwrap_or_default();
    let mounted = discovery::discover_mounted_shares();

    let favorites = cfg
        .favorites
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
        .collect();

    let addable = mounted
        .into_iter()
        .filter(|m| {
            !cfg.favorites.iter().any(|f| {
                f.share.eq_ignore_ascii_case(&m.share)
                    && f.server.eq_ignore_ascii_case(&m.server)
            })
        })
        .map(|m| (m.share, m.server))
        .collect();

    TraySnapshot { favorites, addable }
}

/// Build the tray menu from a snapshot.
fn build_menu(snap: &TraySnapshot) -> Menu {
    let menu = Menu::new();

    // Title item (disabled)
    let title = MenuItem::with_id("title", "Mountaineer", false, None);
    let _ = menu.append(&title);
    let _ = menu.append(&PredefinedMenuItem::separator());

    if snap.favorites.is_empty() {
        let empty = MenuItem::with_id("empty", "No favorites configured", false, None);
        let _ = menu.append(&empty);
    } else {
        for status in &snap.favorites {
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

    let show_logs = MenuItem::with_id("show-logs", "Show Logs", true, None);
    let _ = menu.append(&show_logs);

    // Manage Favorites submenu
    if !snap.addable.is_empty() || !snap.favorites.is_empty() {
        let submenu = Submenu::new("Manage Favorites", true);
        for (share, server) in &snap.addable {
            let item = MenuItem::with_id(
                format!("fav-add:{}:{}", server, share),
                format!("Add {} ({})", share, server),
                true,
                None,
            );
            let _ = submenu.append(&item);
        }
        if !snap.addable.is_empty() && !snap.favorites.is_empty() {
            let _ = submenu.append(&PredefinedMenuItem::separator());
        }
        for status in &snap.favorites {
            let item = MenuItem::with_id(
                format!("fav-remove:{}:{}", status.server, status.share),
                format!("Remove {}", status.share),
                true,
                None,
            );
            let _ = submenu.append(&item);
        }
        let _ = menu.append(&submenu);
    }

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
    let snap = snapshot();
    let menu = build_menu(&snap);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Mountaineer")
        .with_icon(make_icon())
        .build()
        .expect("failed to build tray icon");

    // Start network monitor on background thread, get receiver
    let network_rx = crate::network::monitor::start();

    // Auto-mount shared state
    let mount_in_progress = Arc::new(AtomicBool::new(false));
    let (mount_done_tx, mount_done_rx) = std::sync::mpsc::channel::<usize>();

    // Single GPUI async task owns the tray icon (TrayIcon is !Send, must stay on main thread)
    cx.spawn(async move |cx: &mut AsyncApp| {
        let menu_receiver = MenuEvent::receiver();
        let mut prev_snap = TraySnapshot {
            favorites: Vec::new(),
            addable: Vec::new(),
        };
        let status_interval = Duration::from_secs(30);
        let mount_cooldown = Duration::from_secs(10);

        let start_time = Instant::now();
        let mut startup_mount_done = false;
        let mut last_status_check = Instant::now();
        let mut last_auto_mount = Instant::now() - mount_cooldown; // allow immediate first mount
        let mut last_iteration = Instant::now();

        loop {
            let now = Instant::now();

            // --- Wake detection ---
            // If gap between iterations > 5s, system was asleep.
            let gap = now.duration_since(last_iteration);
            let woke_up = gap > Duration::from_secs(5);
            if woke_up {
                log::info!(
                    "Wake detected ({}s gap) — verifying mount liveness",
                    gap.as_secs()
                );
                // Reset cooldown so mount triggers immediately
                last_auto_mount = now - mount_cooldown;
            }
            last_iteration = now;

            // --- Menu events ---
            while let Ok(event) = menu_receiver.try_recv() {
                let id = event.id().0.as_str().to_string();
                handle_menu_event(&id, &mount_in_progress, &mount_done_tx);

                if id == "quit" {
                    let _ = cx.update(|cx| cx.quit());
                    return;
                }

                // Refresh after non-mount actions (config changes, etc.)
                if !id.starts_with("mount-") {
                    cx.background_executor()
                        .timer(Duration::from_secs(3))
                        .await;
                    let snap = snapshot();
                    tray.set_menu(Some(Box::new(build_menu(&snap))));
                    prev_snap = snap;
                }
            }

            // --- Mount completion → refresh menu ---
            while let Ok(count) = mount_done_rx.try_recv() {
                if count > 0 {
                    log::info!("Auto-mount cycle completed: {} shares mounted", count);
                }
                let snap = snapshot();
                if snap != prev_snap {
                    tray.set_menu(Some(Box::new(build_menu(&snap))));
                    prev_snap = snap;
                }
            }

            // --- Network events ---
            let mut network_changed = false;
            while network_rx.try_recv().is_ok() {
                network_changed = true;
            }
            if network_changed {
                // Debounce: drain bursts
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                while network_rx.try_recv().is_ok() {}
            }

            // --- Startup auto-mount (once, after 5s delay) ---
            if !startup_mount_done && now.duration_since(start_time) >= Duration::from_secs(5) {
                startup_mount_done = true;
                log::info!("Startup auto-mount — mounting reachable favorites");
                if trigger_mount(false, &mount_in_progress, &mount_done_tx) {
                    last_auto_mount = now;
                }
            }

            // --- Wake auto-mount (verify liveness) ---
            if woke_up && startup_mount_done {
                if trigger_mount(true, &mount_in_progress, &mount_done_tx) {
                    last_auto_mount = now;
                }
            }

            // --- Periodic / network-triggered status refresh + auto-mount ---
            if network_changed || now.duration_since(last_status_check) >= status_interval {
                let new_snap = snapshot();
                if new_snap != prev_snap {
                    log::debug!("Status changed, refreshing tray menu");
                    tray.set_menu(Some(Box::new(build_menu(&new_snap))));
                    prev_snap = new_snap.clone();
                }
                last_status_check = now;

                // Auto-mount if any favorites are disconnected and cooldown elapsed
                let has_unmounted = new_snap
                    .favorites
                    .iter()
                    .any(|f| !f.connected);

                if has_unmounted && now.duration_since(last_auto_mount) >= mount_cooldown {
                    log::debug!("Unmounted favorites detected — triggering auto-mount");
                    if trigger_mount(false, &mount_in_progress, &mount_done_tx) {
                        last_auto_mount = now;
                    }
                }
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
fn handle_menu_event(
    id: &str,
    mount_in_progress: &Arc<AtomicBool>,
    mount_done_tx: &std::sync::mpsc::Sender<usize>,
) {
    match id {
        "mount-all" => {
            log::info!("Manual mount-all requested");
            trigger_mount(false, mount_in_progress, mount_done_tx);
        }
        "wake-all" => {
            std::thread::spawn(|| {
                if let Err(e) = wake_all_servers() {
                    log::error!("Wake all failed: {}", e);
                }
            });
        }
        "show-logs" => {
            if let Some(home) = dirs::home_dir() {
                let log_path = home.join("Library/Logs/mountaineer.log");
                let _ = std::process::Command::new("open")
                    .arg("-a")
                    .arg("Console")
                    .arg(&log_path)
                    .spawn();
            }
        }
        "quit" => {
            // Handled in the polling loop
        }
        id if id.starts_with("open-") => {
            let share_name = &id[5..];
            open_share_in_finder(share_name);
        }
        id if id.starts_with("fav-add:") => {
            if let Some((server, share)) = id["fav-add:".len()..].split_once(':') {
                let server = server.to_string();
                let share = share.to_string();
                std::thread::spawn(move || add_share_to_favorites(&server, &share));
            }
        }
        id if id.starts_with("fav-remove:") => {
            if let Some((server, share)) = id["fav-remove:".len()..].split_once(':') {
                let server = server.to_string();
                let share = share.to_string();
                std::thread::spawn(move || remove_share_from_favorites(&server, &share));
            }
        }
        _ => {}
    }
}

/// Add a mounted share to favorites (mirrors CLI cmd_add logic).
fn add_share_to_favorites(server: &str, share: &str) {
    let mut cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            return;
        }
    };

    // Already a favorite — nothing to do
    if cfg.favorites.iter().any(|f| {
        f.share.eq_ignore_ascii_case(share) && f.server.eq_ignore_ascii_case(server)
    }) {
        log::warn!("{} on {} is already a favorite", share, server);
        return;
    }

    // Find in currently mounted shares to get mount_point
    let mounted = discovery::discover_mounted_shares();
    let found = mounted.iter().find(|m| {
        m.share.eq_ignore_ascii_case(share) && m.server.eq_ignore_ascii_case(server)
    });

    let mount_point = match found {
        Some(m) => m.mount_point.clone(),
        None => {
            log::error!("{} on {} is no longer mounted", share, server);
            return;
        }
    };

    let mac_address = discovery::discover_mac_address(server);

    let fav = config::Favorite {
        server: server.to_string(),
        share: share.to_string(),
        mount_point,
        mac_address,
    };

    log::info!("Adding favorite: {} on {}", fav.share, fav.server);
    cfg.favorites.push(fav);
    if let Err(e) = config::save(&cfg) {
        log::error!("Failed to save config: {}", e);
    }
}

/// Remove a share from favorites (mirrors CLI cmd_remove logic).
fn remove_share_from_favorites(server: &str, share: &str) {
    let mut cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            return;
        }
    };

    let before = cfg.favorites.len();
    cfg.favorites.retain(|f| {
        !(f.share.eq_ignore_ascii_case(share) && f.server.eq_ignore_ascii_case(server))
    });

    if cfg.favorites.len() == before {
        log::warn!("{} on {} not found in favorites", share, server);
        return;
    }

    log::info!("Removed favorite: {} on {}", share, server);
    if let Err(e) = config::save(&cfg) {
        log::error!("Failed to save config: {}", e);
    }
}

/// Run a full auto-mount cycle: verify liveness, unmount stale, mount reachable.
///
/// When `verify_liveness` is true (e.g. after wake), also checks whether
/// "mounted" shares are actually alive and force-unmounts stale ones.
///
/// Returns the number of shares newly mounted.
fn auto_mount_cycle(verify_liveness: bool) -> usize {
    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("auto_mount_cycle: failed to load config: {}", e);
            return 0;
        }
    };

    if cfg.favorites.is_empty() {
        return 0;
    }

    let mounted = discovery::discover_mounted_shares();
    let mut newly_mounted = 0;

    for fav in &cfg.favorites {
        let appears_mounted = mounted.iter().any(|m| {
            m.share.eq_ignore_ascii_case(&fav.share)
                && m.server.eq_ignore_ascii_case(&fav.server)
        });

        let actually_mounted = if appears_mounted && verify_liveness {
            let mount_point = std::path::Path::new(&fav.mount_point);
            let alive = mount::smb::is_mount_alive(mount_point);
            if !alive {
                log::warn!(
                    "{}: stale mount detected at {} — force unmounting",
                    fav.share,
                    fav.mount_point,
                );
                if let Err(e) = mount::smb::unmount(mount_point) {
                    log::error!("{}: force unmount failed: {}", fav.share, e);
                    // Skip remount if we can't unmount the stale entry
                    continue;
                }
                false
            } else {
                true
            }
        } else {
            appears_mounted
        };

        if !actually_mounted {
            if discovery::is_smb_reachable(&fav.server) {
                log::info!("{}: SMB reachable — mounting...", fav.share);
                match mount::smb::mount_favorite(fav) {
                    Ok(()) => {
                        log::info!("{}: mounted at {}", fav.share, fav.mount_point);
                        newly_mounted += 1;
                    }
                    Err(e) => {
                        log::error!("{}: mount failed — {}", fav.share, e);
                    }
                }
            } else {
                log::debug!("{}: SMB unreachable (port 445) — skipping", fav.share);
            }
        }
    }

    newly_mounted
}

/// Spawn auto_mount_cycle on a background thread if not already running.
/// Returns true if a cycle was started, false if one was already in progress.
fn trigger_mount(
    verify_liveness: bool,
    in_progress: &Arc<AtomicBool>,
    done_tx: &std::sync::mpsc::Sender<usize>,
) -> bool {
    if in_progress.swap(true, Ordering::SeqCst) {
        return false; // Already in progress
    }
    let flag = in_progress.clone();
    let tx = done_tx.clone();
    std::thread::spawn(move || {
        let count = auto_mount_cycle(verify_liveness);
        flag.store(false, Ordering::Release);
        let _ = tx.send(count);
    });
    true
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
