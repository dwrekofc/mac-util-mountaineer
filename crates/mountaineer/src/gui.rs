use gpui::*;
use gpui_component_assets::Assets;

pub fn run() {
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        // Override GPUI's Regular activation policy → Accessory (no dock icon)
        #[cfg(target_os = "macos")]
        unsafe {
            use objc::msg_send;
            use objc::sel;
            use objc::sel_impl;
            let ns_app: *mut objc::runtime::Object =
                msg_send![objc::class!(NSApplication), sharedApplication];
            // NSApplicationActivationPolicyAccessory = 1
            let _: () = msg_send![ns_app, setActivationPolicy: 1i64];
        }

        gpui_component::init(cx);
        crate::tray::install(cx);
        log::info!("Mountaineer menu bar app running");
    });
}
