use std::time::Duration;

use gpui::{App, AsyncApp};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIconBuilder, TrayIconEvent};

pub fn install(cx: &App) {
    let quit_item = MenuItem::new("Quit Mountaineer", true, None);
    let quit_id = quit_item.id().clone();

    let menu = Menu::with_items(&[
        &MenuItem::new("Mountaineer", false, None),
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .expect("failed to build tray menu");

    let _tray_icon = TrayIconBuilder::new()
        .with_title("⛰")
        .with_menu(Box::new(menu))
        .build()
        .expect("failed to build tray icon");

    // Leak the TrayIcon so it lives for the entire app lifetime.
    // There is only ever one tray icon and it must not be dropped.
    std::mem::forget(_tray_icon);

    start_event_polling(quit_id, cx);
}

fn start_event_polling(quit_id: tray_icon::menu::MenuId, cx: &App) {
    let menu_receiver = MenuEvent::receiver().clone();
    let tray_receiver = TrayIconEvent::receiver().clone();

    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;

            while let Ok(event) = menu_receiver.try_recv() {
                if event.id == quit_id {
                    log::info!("Quit menu item clicked");
                    let _ = cx.update(|cx: &mut App| cx.quit());
                    return;
                }
            }

            while let Ok(event) = tray_receiver.try_recv() {
                log::debug!("Tray icon event: {:?}", event);
            }
        }
    })
    .detach();
}
