//! Tauri application wiring: setup, state, telemetry ticker, IPC handlers.

mod commands;
mod state;
mod tray;
mod update;
mod update_ipc;

use std::thread;
use std::time::Duration;

use colorctl_core::Service;
use colorctl_core::settings::{POLL_DEFAULT_MS, POLL_MAX_MS, POLL_MIN_MS, Settings};
use colorctl_core::{ResumeWatch, WakeReport};
use tauri::{Emitter, Manager};

use crate::state::{AppState, Inner, ServiceSlot};

/// Emitted once the hardware probe finishes, so the UI can drop its boot overlay.
pub const BACKEND_READY_EVENT: &str = "backend://ready";

pub fn run() {
    let started = std::time::Instant::now();
    update::clean_previous();

    tauri::Builder::default()
        .setup(move |app| {
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

            // Publish the state and show the window *before* the hardware probe.
            // Probing costs a few hundred milliseconds, and none of it is needed
            // to paint the shell: the UI shows a boot overlay until the
            // `backend://ready` event arrives.
            let probe_settings = settings.clone();
            let probe_data = data_dir.clone();
            let reapply_led = settings.reapply_led_on_launch;

            app.manage(AppState::new(Inner {
                service: ServiceSlot::Loading,
                settings,
                settings_path,
                settings_warning,
                data_dir,
            }));

            tray::install(&handle)?;
            tray::show_unless_hidden(&handle);
            eprintln!("opencolorhub: window shown at {:?}", started.elapsed());

            let ready = handle.clone();
            thread::spawn(move || {
                // A probe that panics must not leave the UI waiting forever.
                let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Service::new(probe_settings, probe_data)
                }));
                let mut service = match built {
                    Ok(service) => service,
                    Err(_) => {
                        let state = ready.state::<AppState>();
                        if let Ok(mut inner) = state.lock() {
                            inner.service = ServiceSlot::Failed(
                                "the hardware probe failed — see the app log".to_string(),
                            );
                        }
                        let _ = ready.emit(BACKEND_READY_EVENT, ());
                        return;
                    }
                };
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

                if reapply_led {
                    match service.apply_led() {
                        Ok(result) => eprintln!("opencolorhub: {}", result.message),
                        Err(error) => {
                            eprintln!("opencolorhub: could not re-apply lighting: {error}")
                        }
                    }
                }

                {
                    let state = ready.state::<AppState>();
                    match state.lock() {
                        Ok(mut inner) => inner.service = ServiceSlot::Ready(Box::new(service)),
                        Err(error) => {
                            eprintln!("opencolorhub: could not publish the backend: {error}");
                            return;
                        }
                    }
                }
                eprintln!("opencolorhub: hardware ready at {:?}", started.elapsed());

                if let Err(error) = ready.emit(BACKEND_READY_EVENT, ()) {
                    eprintln!("opencolorhub: could not announce readiness: {error}");
                }
            });

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
            commands::downloaded_drivers,
            commands::reveal_driver,
            commands::pawnio_status,
            commands::install_pawnio,
            commands::list_setups,
            commands::save_setup,
            commands::apply_setup,
            commands::delete_setup,
            commands::get_autostart,
            commands::set_autostart,
            update_ipc::update_check,
            update_ipc::update_install,
        ])
        .build(tauri::generate_context!())
        .expect("error while building opencolorhub")
        .run(|handle, event| {
            // Hand every fan this session took over back to the chip's own
            // SmartFan control. Only `ExitRequested` runs while the driver is
            // still open, so that is where the handshake belongs.
            if matches!(event, tauri::RunEvent::ExitRequested { .. })
                && let Ok(mut inner) = handle.state::<AppState>().lock()
                && let Ok(service) = inner.service_mut()
            {
                service.shutdown();
            }
        });
}

/// Attempts at re-applying after a wake, and the pause between them.
///
/// The resume re-enumerates the USB controller, so the first frames after
/// waking can fail while Windows reinstalls the interface; the light is usually
/// back within a second or two.
const WAKE_ATTEMPTS: usize = 6;
const WAKE_RETRY_DELAY: Duration = Duration::from_millis(700);

/// Emits a snapshot on the configured interval.
///
/// A plain OS thread keeps this independent of the async runtime: the work is
/// blocking port I/O, which should never sit on a tokio worker.
fn spawn_ticker(app: tauri::AppHandle) {
    thread::spawn(move || {
        let mut watch = ResumeWatch::new(Duration::from_millis(POLL_DEFAULT_MS));
        loop {
            let interval = {
                let state = app.state::<AppState>();
                match state.lock() {
                    Ok(inner) => inner.settings.poll_interval_ms,
                    Err(_) => POLL_DEFAULT_MS,
                }
            };
            let interval = Duration::from_millis(interval.clamp(POLL_MIN_MS, POLL_MAX_MS));
            watch.set_interval(interval);
            thread::sleep(interval);

            // The hardware does not keep its state across a suspend, and neither
            // does the USB handle; the gap between ticks is the only signal the
            // process gets that it has been asleep.
            if let Some(slept) = watch.observe_now() {
                reapply_after_wake(&app, slept);
            }

            let snapshot = {
                let state = app.state::<AppState>();
                match state.lock() {
                    Ok(mut inner) => match inner.service_mut() {
                        Ok(service) => Some(service.snapshot()),
                        Err(_) => None,
                    },
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

/// Re-applies the lighting and fan settings the machine forgot while asleep.
///
/// Locks the state for one attempt at a time, so an IPC command can still get in
/// between retries instead of waiting out the whole sequence.
fn reapply_after_wake(app: &tauri::AppHandle, slept: Duration) {
    let mut report = WakeReport::default();
    for attempt in 0..WAKE_ATTEMPTS {
        match reassert_once(app) {
            Ok(outcome) => report = outcome,
            Err(error) => {
                eprintln!("opencolorhub: could not re-apply after waking: {error}");
                return;
            }
        }
        if report.errors.is_empty() {
            break;
        }
        if attempt + 1 < WAKE_ATTEMPTS {
            thread::sleep(WAKE_RETRY_DELAY);
        }
    }

    if !report.restored_something() && report.errors.is_empty() {
        return;
    }

    let message = if report.errors.is_empty() {
        match (report.fans, report.lighting) {
            (0, _) => "Re-applied lighting after wake".to_string(),
            (fans, true) => format!("Re-applied {fans} fans and lighting after wake"),
            (fans, false) => format!("Re-applied {fans} fans after wake"),
        }
    } else {
        format!(
            "Could not re-apply after wake — {}",
            report.errors.join("; ")
        )
    };

    eprintln!("opencolorhub: woke after {}s: {message}", slept.as_secs());
    let state = app.state::<AppState>();
    if let Ok(mut inner) = state.lock()
        && let Ok(service) = inner.service_mut()
    {
        service.note("wake", message);
    }
}

fn reassert_once(app: &tauri::AppHandle) -> Result<WakeReport, String> {
    let state = app.state::<AppState>();
    let mut inner = state.lock()?;
    Ok(inner.service_mut()?.reassert_after_wake())
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
