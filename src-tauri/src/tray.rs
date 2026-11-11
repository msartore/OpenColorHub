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

pub const ID: &str = "main";

const WINDOW: &str = "main";

/// Flag the autostart entry passes to keep the window out of the way.
const HIDDEN_FLAG: &str = "--minimized";

/// Takes the arguments rather than reading the process's own, because the
/// single-instance callback receives the *second* process's command line and
/// has to tell an icon click from the autostart entry.
pub fn wants_hidden<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == HIDDEN_FLAG)
}

pub fn starts_hidden() -> bool {
    wants_hidden(std::env::args())
}

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

    // Without this the tray is destroyed the first time somebody hits the title-bar X.
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

pub fn show_unless_hidden(app: &AppHandle) {
    if starts_hidden() {
        return;
    }
    reveal(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_autostart_flag() {
        assert!(wants_hidden(["opencolorhub.exe", "--minimized"]));
        assert!(!wants_hidden(["opencolorhub.exe"]));
        assert!(!wants_hidden(["C:/build/--minimized/OpenColorHub.exe"]));
        assert!(!wants_hidden(["opencolorhub.exe", "--minimised"]));
    }
}
