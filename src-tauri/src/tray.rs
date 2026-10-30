//! System tray.
//!
//! The app is a background utility: at sign-in it should apply the saved state
//! and get out of the way. So the window starts hidden when the autostart entry
//! launches it (`--minimized`), closing the window hides it instead of quitting,
//! and the only real exit is the tray menu's **Quit**.
//!
//! Tauri's own tray API is used rather than the `tray-icon` + `winit` recipe
//! that floats around: Tauri already owns an event loop, and a second one would
//! fight the webview.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// Tray icon id, so the entry can be updated or removed later.
pub const ID: &str = "main";

/// Window label the tray controls.
const WINDOW: &str = "main";

/// True when the process was started by the autostart entry.
pub fn starts_hidden() -> bool {
    std::env::args().any(|arg| arg == "--minimized")
}

/// Brings the window back, whether it was hidden or merely minimised.
pub fn reveal(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn toggle(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };
    match window.is_visible() {
        Ok(true) => {
            let _ = window.hide();
        }
        _ => reveal(app),
    }
}

/// Creates the tray icon and makes the window close to the tray.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open OpenColorHub", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    let mut builder = TrayIconBuilder::with_id(ID)
        .tooltip("OpenColorHub — fan and lighting control")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => reveal(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left click toggles; the menu handles everything else.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    // Closing the window parks the app in the tray. Without this the tray would
    // be destroyed the first time somebody hits the title-bar X.
    if let Some(window) = app.get_webview_window(WINDOW) {
        let handle = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = handle.hide();
            }
        });
    }

    Ok(())
}

/// Shows the window unless this launch came from the autostart entry.
pub fn show_unless_hidden(app: &AppHandle) {
    if starts_hidden() {
        return;
    }
    reveal(app);
}
