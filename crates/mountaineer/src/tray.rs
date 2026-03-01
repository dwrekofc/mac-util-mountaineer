use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::config::{self, Backend};
use crate::engine::{self, RuntimeState, ShareStatus, SwitchResult};
use crate::logging;
use crate::network;

/// Shared state for the tray menu, updated by the background reconciliation loop.
struct TrayState {
    statuses: Vec<ShareStatus>,
    runtime_state: RuntimeState,
}

pub fn install(cx: &mut App) {
    // Load initial state
    let cfg = config::load().unwrap_or_default();
    let mut runtime_state = engine::load_runtime_state().unwrap_or_default();
    let statuses = engine::verify_all(&cfg, &mut runtime_state);

    let state = Arc::new(Mutex::new(TrayState {
        statuses,
        runtime_state,
    }));

    let menu = build_dynamic_menu(&state);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Mountaineer")
        .with_icon(make_icon())
        .build()
        .expect("failed to build tray icon");

    // Keep tray alive
    #[allow(clippy::arc_with_non_send_sync)]
    let tray = Arc::new(Mutex::new(tray));

    // Clone for the event handler
    let state_for_events = Arc::clone(&state);
    let tray_for_events = Arc::clone(&tray);

    // Menu event handling loop
    cx.spawn(async move |cx: &mut AsyncApp| {
        let receiver = MenuEvent::receiver();

        loop {
            while let Ok(event) = receiver.try_recv() {
                let id = event.id().0.as_str().to_string();
                handle_menu_event(&id, &state_for_events, &tray_for_events);

                // Check for quit
                if id == "quit" {
                    cx.update(|cx| cx.quit());
                    return;
                }
            }

            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
        }
    })
    .detach();

    // Start SCDynamicStore network change monitor (spec 11).
    // A bridge thread reads network events, debounces them (500ms per spec 11),
    // and sets a flag that the GPUI reconcile loop polls.
    let network_changed = Arc::new(AtomicBool::new(false));
    let network_changed_writer = Arc::clone(&network_changed);
    let network_rx = network::monitor::start();
    std::thread::Builder::new()
        .name("tray-network-bridge".into())
        .spawn(move || {
            loop {
                match network_rx.recv() {
                    Ok(event) => {
                        log::info!("Tray: network change detected: {:?}", event.changed_keys);
                        // Debounce: drain further events for 500ms (spec 11)
                        let debounce = Duration::from_millis(500);
                        while network_rx.recv_timeout(debounce).is_ok() {}
                        network_changed_writer.store(true, Ordering::Release);
                        log::info!("Tray: network debounce complete, flagging reconcile");
                    }
                    Err(_) => {
                        log::warn!("Tray: network monitor channel disconnected");
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn tray network bridge thread");

    // Background reconciliation loop
    let state_for_reconcile = Arc::clone(&state);
    let tray_for_reconcile = Arc::clone(&tray);

    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            // Load config and reconcile
            let cfg = config::load().unwrap_or_default();
            let check_interval = cfg.global.check_interval_secs;

            {
                let mut guard = state_for_reconcile.lock().unwrap();
                guard.statuses = engine::reconcile_all(&cfg, &mut guard.runtime_state);
                let _ = engine::save_runtime_state(&guard.runtime_state);
            }

            // Rebuild menu with updated state
            let new_menu = build_dynamic_menu(&state_for_reconcile);
            if let Ok(tray) = tray_for_reconcile.lock() {
                tray.set_menu(Some(Box::new(new_menu)));
            }

            // Poll at 500ms granularity so network events trigger reconcile within
            // ~500ms instead of waiting the full check_interval (spec 11).
            let poll_ms = 500u64;
            let total_wait_ms = check_interval * 1000;
            let mut waited_ms = 0u64;
            while waited_ms < total_wait_ms {
                if network_changed.swap(false, Ordering::AcqRel) {
                    log::info!("Tray: network change flag set, triggering immediate reconcile");
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(poll_ms))
                    .await;
                waited_ms += poll_ms;
            }
        }
    })
    .detach();
}

fn handle_menu_event(id: &str, state: &Arc<Mutex<TrayState>>, tray: &Arc<Mutex<TrayIcon>>) {
    match id {
        "open-shares" => {
            let _ = open_shares_folder();
        }
        "open-logs" => {
            let _ = open_log_file();
        }
        "toggle-auto-failback" => {
            toggle_config_bool(
                "auto_failback",
                |g| g.auto_failback,
                |g, v| g.auto_failback = v,
            );
            rebuild_menu(state, tray);
        }
        "toggle-lsof-recheck" => {
            toggle_config_bool(
                "lsof_recheck",
                |g| g.lsof_recheck,
                |g, v| g.lsof_recheck = v,
            );
            rebuild_menu(state, tray);
        }
        "quit" => {
            // Handled in the event loop
        }
        _ if id.starts_with("switch-") => {
            // Parse: switch-{share}-{backend}
            let parts: Vec<&str> = id
                .strip_prefix("switch-")
                .unwrap()
                .rsplitn(2, '-')
                .collect();
            if parts.len() == 2 {
                let backend_str = parts[0];
                let share_name = parts[1];

                let to = match backend_str {
                    "tb" => Backend::Tb,
                    "fallback" | "fb" => Backend::Fallback,
                    _ => return,
                };

                log::info!("Tray: switching {} to {}", share_name, to.short_label());
                handle_switch(share_name, to, state, tray);
            }
        }
        _ => {}
    }
}

fn handle_switch(
    share_name: &str,
    to: Backend,
    state: &Arc<Mutex<TrayState>>,
    tray: &Arc<Mutex<TrayIcon>>,
) {
    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            return;
        }
    };

    let share = match config::find_share(&cfg, share_name) {
        Some(s) => s.clone(),
        None => {
            log::error!("Share '{}' not found", share_name);
            return;
        }
    };

    let mut guard = state.lock().unwrap();

    // Determine current backend
    let current = guard
        .runtime_state
        .shares
        .get(&share_name.to_ascii_lowercase())
        .and_then(|e| e.active_backend);

    if current == Some(to) {
        log::info!("{} is already on {}", share_name, to.short_label());
        return;
    }

    let from = match current {
        Some(b) => b,
        None => {
            log::warn!("{}: no current backend, will do initial mount", share_name);
            // No active backend — trigger a reconcile to do initial mount through the engine
            guard.statuses = engine::reconcile_all(&cfg, &mut guard.runtime_state);
            let _ = engine::save_runtime_state(&guard.runtime_state);
            drop(guard);
            let new_menu = build_dynamic_menu(state);
            if let Ok(tray) = tray.lock() {
                tray.set_menu(Some(Box::new(new_menu)));
            }
            return;
        }
    };

    // Use the single-mount switch function
    match engine::switch_backend_single_mount(
        &cfg,
        &mut guard.runtime_state,
        &share,
        from,
        to,
        false,
    ) {
        SwitchResult::Success => {
            log::info!(
                "{}: switched {} -> {}",
                share_name,
                from.short_label(),
                to.short_label()
            );
            let _ = engine::save_runtime_state(&guard.runtime_state);

            // Refresh statuses
            drop(guard);
            let mut guard = state.lock().unwrap();
            guard.statuses = engine::verify_all(&cfg, &mut guard.runtime_state);

            // Update menu
            drop(guard);
            let new_menu = build_dynamic_menu(state);
            if let Ok(tray) = tray.lock() {
                tray.set_menu(Some(Box::new(new_menu)));
            }
        }
        SwitchResult::BusyOpenFiles => {
            log::warn!(
                "{}: cannot switch - open files detected. Close files and try again.",
                share_name
            );
            // Could show a notification here
        }
        SwitchResult::UnmountFailed(e) => {
            log::error!("{}: unmount failed: {}", share_name, e);
        }
        SwitchResult::MountFailed { error, rolled_back } => {
            log::error!(
                "{}: mount failed: {} (rolled back: {})",
                share_name,
                error,
                rolled_back
            );
        }
    }
}

fn build_dynamic_menu(state: &Arc<Mutex<TrayState>>) -> Menu {
    let menu = Menu::new();

    let title = MenuItem::with_id("title", "Mountaineer", false, None);
    let _ = menu.append(&title);
    let _ = menu.append(&PredefinedMenuItem::separator());

    // Add share status items
    let guard = state.lock().unwrap();
    let has_pending = guard
        .runtime_state
        .shares
        .values()
        .any(|e| e.tb_recovery_pending);

    for status in &guard.statuses {
        // Determine connection status text
        let (backend_label, connected) = match status.active_backend {
            Some(Backend::Tb) => ("TB", status.tb.ready),
            Some(Backend::Fallback) => ("Fallback", status.fallback.ready),
            None => ("None", false),
        };

        let status_text = if connected { "●" } else { "○" };

        // Check if TB recovery is pending for this share
        let tb_pending = guard
            .runtime_state
            .shares
            .get(&status.name.to_ascii_lowercase())
            .map(|e| e.tb_recovery_pending)
            .unwrap_or(false);

        let label = if tb_pending {
            format!(
                "{} {} [TB available!] {}",
                status_text, status.name, backend_label
            )
        } else {
            format!("{} {} ({})", status_text, status.name, backend_label)
        };

        // Create submenu for this share
        let submenu = Submenu::new(&label, true);

        // Add switch options
        let other_backend = match status.active_backend {
            Some(Backend::Tb) => Backend::Fallback,
            Some(Backend::Fallback) | None => Backend::Tb,
        };

        let other_reachable = match other_backend {
            Backend::Tb => status.tb.reachable,
            Backend::Fallback => status.fallback.reachable,
        };

        if other_reachable {
            let switch_label = if tb_pending && other_backend == Backend::Tb {
                "⚡ Switch to TB (available)".to_string()
            } else {
                format!("Switch to {}", other_backend.short_label())
            };
            let switch_item = MenuItem::with_id(
                format!("switch-{}-{}", status.name, other_backend.short_label()),
                &switch_label,
                true,
                None,
            );
            let _ = submenu.append(&switch_item);
        }

        // Show backend status
        let _ = submenu.append(&PredefinedMenuItem::separator());

        let tb_status = format!(
            "TB: {} {}",
            if status.tb.reachable {
                "reachable"
            } else {
                "offline"
            },
            if status.tb.ready { "(mounted)" } else { "" }
        );
        let tb_item =
            MenuItem::with_id(format!("info-tb-{}", status.name), &tb_status, false, None);
        let _ = submenu.append(&tb_item);

        let fb_status = format!(
            "Fallback: {} {}",
            if status.fallback.reachable {
                "reachable"
            } else {
                "offline"
            },
            if status.fallback.ready {
                "(mounted)"
            } else {
                ""
            }
        );
        let fb_item =
            MenuItem::with_id(format!("info-fb-{}", status.name), &fb_status, false, None);
        let _ = submenu.append(&fb_item);

        // Show last error if present (spec 18)
        if let Some(error) = &status.last_error {
            let _ = submenu.append(&PredefinedMenuItem::separator());
            let err_label = format!("! {}", error);
            let err_item =
                MenuItem::with_id(format!("info-err-{}", status.name), &err_label, false, None);
            let _ = submenu.append(&err_item);
        }

        let _ = menu.append(&submenu);
    }
    drop(guard);

    // Highlight if any share has TB pending
    if has_pending {
        let _ = menu.append(&PredefinedMenuItem::separator());
        let notice = MenuItem::with_id("notice-tb", "⚡ TB connections available", false, None);
        let _ = menu.append(&notice);
    }

    let _ = menu.append(&PredefinedMenuItem::separator());

    let open_shares = MenuItem::with_id("open-shares", "Open Shares Folder", true, None);
    let _ = menu.append(&open_shares);

    let open_logs = MenuItem::with_id("open-logs", "Open Logs", true, None);
    let _ = menu.append(&open_logs);

    let _ = menu.append(&PredefinedMenuItem::separator());

    // Config toggles with visual state indicators (spec 19)
    let cfg = config::load().unwrap_or_default();
    let failback_label = format!(
        "Auto Failback [{}]",
        if cfg.global.auto_failback {
            "on"
        } else {
            "off"
        }
    );
    let failback_toggle = MenuItem::with_id("toggle-auto-failback", &failback_label, true, None);
    let _ = menu.append(&failback_toggle);

    let lsof_label = format!(
        "Lsof Recheck [{}]",
        if cfg.global.lsof_recheck { "on" } else { "off" }
    );
    let lsof_toggle = MenuItem::with_id("toggle-lsof-recheck", &lsof_label, true, None);
    let _ = menu.append(&lsof_toggle);

    let _ = menu.append(&PredefinedMenuItem::separator());

    let quit = MenuItem::with_id("quit", "Quit Mountaineer", true, None);
    let _ = menu.append(&quit);

    menu
}

fn open_shares_folder() -> anyhow::Result<()> {
    let cfg = config::load().unwrap_or_default();
    let shares_root = config::shares_root_path(&cfg);
    std::fs::create_dir_all(&shares_root)?;
    std::process::Command::new("open")
        .arg(shares_root)
        .spawn()?;
    Ok(())
}

fn open_log_file() -> anyhow::Result<()> {
    let path = logging::log_path()?;
    std::process::Command::new("open").arg(path).spawn()?;
    Ok(())
}

/// Toggle a boolean config field, save atomically, and log the change (spec 19).
fn toggle_config_bool(
    name: &str,
    get: fn(&config::GlobalConfig) -> bool,
    set: fn(&mut config::GlobalConfig, bool),
) {
    match config::load() {
        Ok(mut cfg) => {
            let new_value = !get(&cfg.global);
            set(&mut cfg.global, new_value);
            if let Err(e) = config::save(&cfg) {
                log::error!("Failed to save config after toggling {}: {}", name, e);
                return;
            }
            log::info!(
                "Tray: toggled {} to {}",
                name,
                if new_value { "on" } else { "off" }
            );
        }
        Err(e) => {
            log::error!("Failed to load config for toggle {}: {}", name, e);
        }
    }
}

/// Rebuild the tray menu from current state.
fn rebuild_menu(state: &Arc<Mutex<TrayState>>, tray: &Arc<Mutex<TrayIcon>>) {
    let new_menu = build_dynamic_menu(state);
    if let Ok(tray) = tray.lock() {
        tray.set_menu(Some(Box::new(new_menu)));
    }
}

fn make_icon() -> Icon {
    let size = 16u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    for y in 2..=14u32 {
        let progress = (y - 2) as f32 / 12.0;
        let half_width = (progress * 7.0) as u32;
        let center = 8u32;
        let left = center.saturating_sub(half_width);
        let right = (center + half_width).min(size - 1);

        for x in left..=right {
            let idx = ((y * size + x) * 4) as usize;
            rgba[idx] = 255;
            rgba[idx + 1] = 255;
            rgba[idx + 2] = 255;
            rgba[idx + 3] = 255;
        }
    }

    Icon::from_rgba(rgba, size, size).expect("failed to create tray icon")
}
