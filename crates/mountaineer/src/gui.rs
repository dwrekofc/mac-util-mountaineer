use gpui::*;

pub fn run() {
    Application::new().run(|cx: &mut App| {
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

        crate::tray::install(cx);
        log::info!("Mountaineer menu bar app running");
    });
}
