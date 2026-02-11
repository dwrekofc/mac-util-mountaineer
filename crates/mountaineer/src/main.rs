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

        log::info!("GPUI app running");
    });
}
