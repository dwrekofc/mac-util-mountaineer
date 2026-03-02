pub fn run() {
    // Initialize NSApplication and set activation policy to Accessory (no dock icon) — spec 01.
    // Call finishLaunching since we use a manual event loop instead of NSApplication.run().
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::{class, msg_send, sel, sel_impl};
        let ns_app: *mut objc::runtime::Object =
            msg_send![class!(NSApplication), sharedApplication];
        // NSApplicationActivationPolicyAccessory = 1
        let _: () = msg_send![ns_app, setActivationPolicy: 1i64];
        let _: () = msg_send![ns_app, finishLaunching];
    }

    log::info!("Mountaineer menu bar app running");

    // install() enters the main-thread event loop and never returns.
    // It manually pumps the AppKit event queue alongside tray menu event handling.
    crate::tray::install();
}
