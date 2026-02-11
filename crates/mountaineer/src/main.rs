use gpui::*;
use gpui_component_assets::Assets;

mod network;
mod tray;

fn main() {
    env_logger::init();
    log::info!("Mountaineer starting");

    Application::new().with_assets(Assets).run(|cx: &mut App| {
        gpui_component::init(cx);
        tray::install(cx);

        log::info!("GPUI app running");
    });
}
