use std::time::Duration;

use gpui::*;
use gpui_component_assets::Assets;

mod app_state;
mod network;
mod tray;

use app_state::AppState;

fn main() {
    env_logger::init();
    log::info!("Mountaineer starting");

    Application::new().with_assets(Assets).run(|cx: &mut App| {
        gpui_component::init(cx);

        // Central app state — accessible everywhere via cx.global::<AppState>()
        cx.set_global(AppState::new());

        tray::install(cx);
        start_network_monitor(cx);

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

                let _ = cx.update(|_cx: &mut App| {
                    let interfaces = network::enumerate_interfaces();
                    if interfaces.is_empty() {
                        log::warn!("No active network interfaces");
                    } else {
                        for iface in &interfaces {
                            let status = if iface.is_active() { "UP" } else { "DOWN" };
                            log::info!("  [{}] {}", status, iface);
                        }
                    }
                });
            }
        }
    })
    .detach();
}
