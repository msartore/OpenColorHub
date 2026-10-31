//! Tauri application wiring: setup, state, telemetry ticker, IPC handlers.

mod commands;
mod state;
mod tray;
mod update;
mod update_ipc;

use std::thread;
use std::time::Duration;

use colorctl_core::Service;
use colorctl_core::settings::{POLL_MAX_MS, POLL_MIN_MS, Settings};
use tauri::{Emitter, Manager};

use crate::state::{AppState, Inner};

pub fn run() {
    update::clean_previous();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();

            let (config_dir, data_dir) = state_dirs(&handle)?;

            std::fs::create_dir_all(&config_dir)
                .map_err(|e| format!("cannot create {}: {e}", config_dir.display()))?;
            std::fs::create_dir_all(&data_dir)
                .map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;

            let settings_path = config_dir.join("settings.json");
            let (settings, settings_warning) = Settings::load_or_default(&settings_path);
            if let Some(warning) = &settings_warning {
                eprintln!("opencolorhub: settings file problem: {warning} (using defaults)");
            }

            let mut service = Service::new(settings.clone(), data_dir.clone());
            let snapshot = service.snapshot();
            let board = snapshot.board.clone();
            eprintln!(
                "opencolorhub {}: board = {} [{} via {}]",
                env!("CARGO_PKG_VERSION"),
                board.display_name(),
                board.confidence.label(),
                board.source,
            );
            for note in &board.notes {
                eprintln!("opencolorhub: board note: {note}");
            }
            eprintln!(
                "opencolorhub {}: fan backend = {} ({}), led backend = {} ({})",
                env!("CARGO_PKG_VERSION"),
                snapshot.fan_device.name,
                if snapshot.fan_device.simulated {
                    "simulated"
                } else {
                    "hardware"
                },
                snapshot.led_device.name,
                if snapshot.led_device.simulated {
                    "simulated"
                } else {
                    "hardware"
                },
            );

            if settings.reapply_led_on_launch {
                match service.apply_led() {
                    Ok(result) => eprintln!("opencolorhub: {}", result.message),
                    Err(error) => eprintln!("opencolorhub: could not re-apply lighting: {error}"),
                }
            }

            app.manage(AppState::new(Inner {
                service,
                settings,
                settings_path,
                settings_warning,
                data_dir,
            }));

            tray::install(&handle)?;
            tray::show_unless_hidden(&handle);

            spawn_ticker(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_settings,
            commands::get_elevation,
            commands::request_elevation,
            commands::save_settings,
            commands::get_activity,
            commands::get_board,
            commands::get_about,
            commands::get_settings_warning,
            commands::set_fan_pwm,
            commands::set_fan_percent,
            commands::set_fan_curve,
            commands::apply_profile,
            commands::set_channel,
            commands::set_all_channels,
            commands::set_brightness,
            commands::turn_off_channel,
            commands::apply_led,
            commands::reload_state,
            commands::open_external,
            commands::get_drivers,
            commands::download_driver,
            commands::reveal_driver,
            commands::get_autostart,
            commands::set_autostart,
            update_ipc::update_check,
            update_ipc::update_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running opencolorhub");
}

/// Emits a snapshot on the configured interval.
///
/// A plain OS thread keeps this independent of the async runtime: the work is
/// blocking port I/O, which should never sit on a tokio worker.
fn spawn_ticker(app: tauri::AppHandle) {
    thread::spawn(move || {
        loop {
            let interval = {
                let state = app.state::<AppState>();
                match state.lock() {
                    Ok(inner) => inner.settings.poll_interval_ms,
                    Err(_) => 1000,
                }
            };
            thread::sleep(Duration::from_millis(
                interval.clamp(POLL_MIN_MS, POLL_MAX_MS),
            ));

            let snapshot = {
                let state = app.state::<AppState>();
                match state.lock() {
                    Ok(mut inner) => Some(inner.service.snapshot()),
                    Err(error) => {
                        eprintln!("opencolorhub: telemetry skipped: {error}");
                        None
                    }
                }
            };

            let Some(snapshot) = snapshot else {
                continue;
            };
            if let Err(error) = app.emit(commands::TELEMETRY_EVENT, snapshot) {
                eprintln!("opencolorhub: could not emit telemetry: {error}");
            }
        }
    });
}

/// Directory holding `opencolorhub.portable`, when this is a portable build.
fn portable_root() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    dir.join("opencolorhub.portable").is_file().then_some(dir)
}

///
/// Portable builds keep both beside the executable so the folder can be moved
/// or deleted as a unit; installed builds use the per-user AppData paths.
fn state_dirs(app: &tauri::AppHandle) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    if let Some(root) = portable_root() {
        return Ok((root.join("config"), root.join("data")));
    }
    let config = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("cannot resolve the config directory: {e}"))?;
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve the data directory: {e}"))?;
    Ok((config, data))
}
