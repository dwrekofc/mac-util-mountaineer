use std::path::PathBuf;
use std::time::Duration;

use gpui::*;
use gpui_component_assets::Assets;

mod app_state;
mod mount;
mod network;
mod tray;

use app_state::{AppState, DriveConfig, DriveId};

fn main() {
    env_logger::init();
    log::info!("Mountaineer starting");

    Application::new().with_assets(Assets).run(|cx: &mut App| {
        gpui_component::init(cx);

        // Central app state — accessible everywhere via cx.global::<AppState>()
        cx.set_global(AppState::new());

        // Hardcoded test drive for development — remove once config loading is implemented (bd-157).
        add_test_drive(cx);

        tray::install(cx);
        start_network_monitor(cx);

        // Initial reconcile on startup — runs after the event loop starts.
        cx.defer(|cx: &mut App| {
            log::info!("Initial reconcile on startup");
            let interfaces = network::enumerate_interfaces();
            let state = cx.global_mut::<AppState>();
            mount::manager::reconcile_all(state, &interfaces);
        });

        log::info!("GPUI app running");
    });
}

/// Start the network change monitor and bridge events to the GPUI main thread.
///
/// Spawns a background SCDynamicStore listener (via `network::monitor::start()`)
/// and a GPUI async task that polls the receiver. On each network change event,
/// re-enumerates interfaces and logs transitions.
fn start_network_monitor(cx: &App) {
    let network_rx = network::monitor::start();

    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;

            while let Ok(event) = network_rx.try_recv() {
                log::info!("Network change detected: {:?}", event.changed_keys);

                let _ = cx.update(|cx: &mut App| {
                    let interfaces = network::enumerate_interfaces();
                    if interfaces.is_empty() {
                        log::warn!("No active network interfaces");
                    } else {
                        for iface in &interfaces {
                            let status = if iface.is_active() { "UP" } else { "DOWN" };
                            log::info!("  [{}] {}", status, iface);
                        }
                    }

                    let state = cx.global_mut::<AppState>();
                    mount::manager::reconcile_all(state, &interfaces);
                });
            }
        }
    })
    .detach();
}

/// Hardcoded test drive for development — remove once config loading is implemented (bd-157).
///
/// Set the SMB password via the `SMB_PASSWORD` environment variable:
///   SMB_PASSWORD=yourpassword cargo run
fn add_test_drive(cx: &mut App) {
    let password = std::env::var("SMB_PASSWORD").unwrap_or_default();
    if password.is_empty() {
        log::warn!("SMB_PASSWORD not set — test drive mount will fail on auth");
    }

    let drive = DriveConfig {
        id: DriveId::new(),
        label: "Mac Mini CORE-01".into(),
        server_hostname: "macmini.local".into(),
        server_ethernet_ip: Some("192.168.50.146".parse().unwrap()),
        share_name: "CORE-01".into(),
        username: "dskinnell".into(),
        mount_point: PathBuf::from("/Volumes/CORE-01"),
        enabled: true,
    };

    let id = drive.id;
    let state = cx.global_mut::<AppState>();
    state.drives.insert(id, drive);
    state.passwords.insert(id, password);

    log::info!("Added test drive: Mac Mini CORE-01");
}
