//! IPC commands exposed to the webview.
//!
//! Every command returns `Result<_, String>` so the frontend can display the
//! underlying hardware error verbatim (a `Result<T, E>` needs `E: Serialize`,
//! and hardware errors are strings anyway).
//!
//! Mutations return an [`ActionResult`] with a fresh snapshot: the UI never has
//! to round-trip again to learn what actually happened.

use std::process::Command;

use colorctl_core::about::{AboutInfo, AboutParams};
use colorctl_core::drivers::{self, DownloadedFile, DriverFile, DriverKind};
use colorctl_core::model::{Channel, ChannelConfig, FanHeader, Snapshot};
use colorctl_core::platform::{self, Elevation};
use colorctl_core::service::{ActionResult, Activity};
use colorctl_core::settings::{Settings, SettingsPatch};
use colorctl_core::{CURVE_POINTS, CurvePoint, FanCurve, FanProfile};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

/// Event name carrying telemetry pushes to the UI.
pub const TELEMETRY_EVENT: &str = "telemetry";

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut inner = state.lock()?;
    Ok(inner.service_mut()?.snapshot())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let inner = state.lock()?;
    Ok(inner.settings.clone())
}

/// Whether this process is elevated and whether a prompt can be raised.
#[tauri::command]
pub fn get_elevation() -> Elevation {
    platform::state()
}

/// Asks the OS for elevation and hands over to a fresh elevated instance.
///
/// Returns `"launched"`, `"declined"` (the user dismissed the prompt) or an
/// error. On success the caller must exit — two instances would fight over the
/// same state files, and the elevated one is the one that matters.
#[tauri::command]
pub fn request_elevation(app: AppHandle) -> Result<String, String> {
    match platform::request() {
        Ok(platform::Outcome::Launched) => {
            app.exit(0);
            Ok("launched".to_string())
        }
        Ok(platform::Outcome::Declined) => Ok("declined".to_string()),
        Ok(platform::Outcome::Unsupported) => {
            Err("this build cannot raise a consent prompt".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let (settings, rebuilt) = {
        let mut inner = state.lock()?;
        let mut settings = inner.settings.clone();
        settings.apply_patch(patch);
        let rebuilt = inner.service_mut()?.update_settings(settings.clone());
        inner.settings = settings.clone();
        inner.persist_settings()?;
        (settings, rebuilt)
    };

    if rebuilt {
        // Backend changed: push a fresh snapshot so the UI leaves stale data behind.
        let snapshot = {
            let mut inner = state.lock()?;
            inner.service_mut()?.snapshot()
        };
        let _ = app.emit(TELEMETRY_EVENT, snapshot);
    }

    Ok(settings)
}

/// Motherboard identity, detected from SMBIOS at startup.
#[tauri::command]
pub fn get_board(state: State<'_, AppState>) -> Result<colorctl_core::BoardInfo, String> {
    let inner = state.lock()?;
    Ok(inner.service()?.board().clone())
}

#[tauri::command]
pub fn get_activity(state: State<'_, AppState>) -> Result<Vec<Activity>, String> {
    let inner = state.lock()?;
    Ok(inner.service()?.activity().to_vec())
}

#[tauri::command]
pub fn get_about(app: AppHandle, state: State<'_, AppState>) -> Result<AboutInfo, String> {
    let mut inner = state.lock()?;
    let package = app.package_info();
    let state_path = inner
        .service_mut()?
        .state_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "—".to_string());

    Ok(AboutInfo::build(AboutParams {
        name: package.name.clone(),
        version: package.version.to_string(),
        tagline: "Fan and lighting control for Colorful motherboards".to_string(),
        board: inner.service()?.board().clone(),
        tauri_version: Some(tauri::VERSION.to_string()),
        data_dir: inner.data_dir.display().to_string(),
        settings_path: inner.settings_path.display().to_string(),
        state_path,
        config_override: inner
            .settings
            .state_file
            .as_ref()
            .map(|p| p.display().to_string()),
    }))
}

#[tauri::command]
pub fn set_fan_pwm(
    state: State<'_, AppState>,
    header: FanHeader,
    pwm: u8,
) -> Result<ActionResult, String> {
    let mut inner = state.lock()?;
    inner
        .service_mut()?
        .set_fan_pwm(header, pwm)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_fan_percent(
    state: State<'_, AppState>,
    header: FanHeader,
    percent: u8,
) -> Result<ActionResult, String> {
    let mut inner = state.lock()?;
    inner
        .service_mut()?
        .set_fan_percent(header, percent)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_fan_curve(
    state: State<'_, AppState>,
    header: FanHeader,
    points: Vec<CurvePoint>,
) -> Result<ActionResult, String> {
    if points.len() != CURVE_POINTS {
        return Err(format!(
            "a curve needs exactly {CURVE_POINTS} points, got {}",
            points.len()
        ));
    }
    let mut curve: FanCurve = [CurvePoint::new(0, 0); CURVE_POINTS];
    curve.copy_from_slice(&points);

    let mut inner = state.lock()?;
    inner
        .service_mut()?
        .set_fan_curve(header, &curve)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_profile(
    state: State<'_, AppState>,
    headers: Vec<FanHeader>,
    profile: FanProfile,
) -> Result<ActionResult, String> {
    let mut inner = state.lock()?;
    inner
        .service_mut()?
        .apply_profile(&headers, profile)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_channel(
    state: State<'_, AppState>,
    channel: Channel,
    color: [u8; 3],
    brightness: u8,
    enabled: bool,
) -> Result<ActionResult, String> {
    let mut inner = state.lock()?;
    let config = if enabled {
        ChannelConfig::rgb(color, brightness)
    } else {
        ChannelConfig::off()
    };
    inner
        .service_mut()?
        .set_channel(channel, config, true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_all_channels(
    state: State<'_, AppState>,
    color: [u8; 3],
    brightness: u8,
    enabled: bool,
) -> Result<ActionResult, String> {
    let mut inner = state.lock()?;
    inner
        .service_mut()?
        .set_all_channels(color, brightness, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_brightness(state: State<'_, AppState>, percent: u8) -> Result<ActionResult, String> {
    let mut inner = state.lock()?;
    inner
        .service_mut()?
        .set_global_brightness(percent)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn turn_off_channel(
    state: State<'_, AppState>,
    channel: Channel,
) -> Result<ActionResult, String> {
    let mut inner = state.lock()?;
    inner
        .service_mut()?
        .turn_off_channel(channel)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_led(state: State<'_, AppState>) -> Result<ActionResult, String> {
    let mut inner = state.lock()?;
    inner.service_mut()?.apply_led().map_err(|e| e.to_string())
}

/// Opens a link in the user's browser.
///
/// Only `http(s)` and `mailto` are allowed, and the URL is passed as a single
/// argv entry (never through a shell string) so it cannot be used to inject
/// extra commands.
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    let allowed = trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("mailto:");
    if !allowed {
        return Err(format!("refusing to open unsupported URL: {trimmed}"));
    }

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", trimmed]);
        c
    };

    #[cfg(unix)]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(trimmed);
        c
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open {trimmed}: {e}"))
}

/// Exposed for the frontend so it can display why the UI has no hardware.
#[tauri::command]
pub fn get_settings_warning(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let inner = state.lock()?;
    Ok(inner.settings_warning.clone())
}

/// Re-reads the lighting state from disk (used by Settings → reload).
#[tauri::command]
pub fn reload_state(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut inner = state.lock()?;
    inner.service_mut()?.reload_led_state();
    Ok(inner.service_mut()?.snapshot())
}

/// Whether the OS will launch this app at sign-in.
#[tauri::command]
pub fn get_autostart() -> colorctl_core::autostart::Autostart {
    colorctl_core::autostart::status()
}

/// Registers or removes the logon entry. Requires elevation on Windows.
#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<colorctl_core::autostart::Autostart, String> {
    colorctl_core::autostart::set_enabled(enabled).map_err(|e| e.to_string())
}

/// Catalogue row as the UI sees it: the file plus whether it is newer than what
/// is installed.
#[derive(serde::Serialize)]
pub struct DriverRow {
    #[serde(flatten)]
    pub file: DriverFile,
    /// True when this is firmware newer than the running BIOS.
    pub newer: bool,
}

#[derive(serde::Serialize)]
pub struct DriverList {
    pub board: String,
    pub product_id: u32,
    /// BIOS version reported by SMBIOS, for context in the UI.
    pub installed_bios: String,
    pub files: Vec<DriverRow>,
}

#[derive(Clone, serde::Serialize)]
struct DriverProgress {
    file: String,
    received: u64,
    total: Option<u64>,
}

/// Lists the vendor's downloads for the detected board.
///
/// Two HTTP round-trips, so it runs off the UI thread. The board name and BIOS
/// version are copied out of the guarded state before awaiting: a `MutexGuard`
/// must not cross an await point.
#[tauri::command]
pub async fn get_drivers(state: State<'_, AppState>) -> Result<DriverList, String> {
    let (board, bios) = {
        let inner = state.lock()?;
        let identity = inner.service()?.board().identity.clone();
        match identity {
            Some(identity) => (identity.name, identity.bios_version),
            None => (String::new(), String::new()),
        }
    };
    if board.trim().is_empty() {
        return Err("this machine did not report a motherboard model".to_string());
    }

    let lookup = board.clone();
    let catalogue = tauri::async_runtime::spawn_blocking(move || drivers::fetch_catalogue(&lookup))
        .await
        .map_err(|e| format!("driver lookup panicked: {e}"))?
        .map_err(|e| e.to_string())?;

    let files = catalogue
        .files
        .into_iter()
        .map(|file| DriverRow {
            newer: file.kind == DriverKind::Bios && drivers::is_newer_firmware(&file.title, &bios),
            file,
        })
        .collect();

    Ok(DriverList {
        board: catalogue.board,
        product_id: catalogue.product_id,
        installed_bios: bios,
        files,
    })
}

/// Downloads one catalogue entry into the app's data directory.
///
/// Emits `drivers://progress` while the transfer runs. Nothing is executed: the
/// file is written, hashed and reported back.
#[tauri::command]
pub async fn download_driver(
    app: AppHandle,
    state: State<'_, AppState>,
    file: DriverFile,
) -> Result<DownloadedFile, String> {
    let dir = state.data_dir().join("drivers");
    let name = file.file_name.clone();
    let handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        drivers::download_with_progress(&file, &dir, |received, total| {
            let _ = handle.emit(
                "drivers://progress",
                DriverProgress {
                    file: name.clone(),
                    received,
                    total,
                },
            );
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("download panicked: {e}"))?
}

/// Opens the folder a downloaded file landed in, selecting the file itself.
#[tauri::command]
pub fn reveal_driver(path: String) -> Result<(), String> {
    let target = std::path::PathBuf::from(&path);
    if !target.exists() {
        return Err(format!("{path} does not exist"));
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(format!("/select,{}", target.display()))
            .spawn()
            .map_err(|e| format!("could not open Explorer: {e}"))?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let dir = target.parent().unwrap_or(&target);
        Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("could not open the file manager: {e}"))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("revealing files is not supported on this platform".to_string())
}

/// Saved setups plus any problem reading the file, so the UI can say so.
#[derive(serde::Serialize)]
pub struct SetupList {
    pub setups: Vec<colorctl_core::Setup>,
    pub warning: Option<String>,
}

fn setup_store(state: &State<'_, AppState>) -> Result<colorctl_core::SetupStore, String> {
    Ok(colorctl_core::SetupStore::load(
        state.data_dir().join("setups.json"),
    ))
}

fn setup_list(store: colorctl_core::SetupStore) -> SetupList {
    SetupList {
        warning: store.warning.clone(),
        setups: store.all().to_vec(),
    }
}

#[tauri::command]
pub fn list_setups(state: State<'_, AppState>) -> Result<SetupList, String> {
    Ok(setup_list(setup_store(&state)?))
}

/// Saves the current fan and lighting state under `name`.
///
/// Passing an `id` overwrites that setup instead of creating a new one, which is
/// what the "Update" action uses.
#[tauri::command]
pub fn save_setup(
    state: State<'_, AppState>,
    name: String,
    id: Option<String>,
) -> Result<SetupList, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a setup needs a name".to_string());
    }

    let mut store = setup_store(&state)?;
    let mut inner = state.lock()?;
    let mut setup = colorctl_core::Setup::capture(inner.service_mut()?, name);
    if let Some(id) = id {
        setup.id = id;
    }
    drop(inner);

    store.upsert(setup).map_err(|e| e.to_string())?;
    Ok(setup_list(store))
}

#[tauri::command]
pub fn apply_setup(state: State<'_, AppState>, id: String) -> Result<ActionResult, String> {
    let store = setup_store(&state)?;
    let setup = store
        .all()
        .iter()
        .find(|entry| entry.id == id)
        .cloned()
        .ok_or_else(|| "that setup no longer exists".to_string())?;

    let mut inner = state.lock()?;
    let message = setup
        .apply(inner.service_mut()?)
        .map_err(|e| e.to_string())?;
    let snapshot = inner.service_mut()?.snapshot();
    Ok(ActionResult { snapshot, message })
}

#[tauri::command]
pub fn delete_setup(state: State<'_, AppState>, id: String) -> Result<SetupList, String> {
    let mut store = setup_store(&state)?;
    store.remove(&id).map_err(|e| e.to_string())?;
    Ok(setup_list(store))
}

/// Whether fan control can reach PawnIO right now.
#[tauri::command]
pub fn pawnio_status() -> colorctl_core::PawnioStatus {
    colorctl_core::platform::pawnio_status()
}

#[derive(Clone, serde::Serialize)]
struct PawnioProgress {
    received: u64,
    total: Option<u64>,
}

/// Downloads PawnIO's official installer, verifies it, runs it, then re-probes.
///
/// The installer is shown with its own UI: this does not install a kernel driver
/// silently, it starts the vendor's installer and waits for the user to finish.
#[tauri::command]
pub async fn install_pawnio(app: AppHandle) -> Result<colorctl_core::PawnioStatus, String> {
    let dir = {
        let state = app.state::<AppState>();
        state.data_dir().join("tools")
    };

    let handle = app.clone();
    let installer = tauri::async_runtime::spawn_blocking(move || {
        colorctl_core::pawnio_install::download(&dir, |received, total| {
            let _ = handle.emit("pawnio://progress", PawnioProgress { received, total });
        })
    })
    .await
    .map_err(|e| format!("download panicked: {e}"))?
    .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        colorctl_core::pawnio_install::run_installer(&installer)
    })
    .await
    .map_err(|e| format!("installer panicked: {e}"))?
    .map_err(|e| e.to_string())?;

    // The probe verdict is cached in the backend, so rebuild before reporting.
    let state = app.state::<AppState>();
    let status = {
        let mut inner = state.lock()?;
        if let Ok(service) = inner.service_mut() {
            service.reload_backends();
        }
        colorctl_core::platform::pawnio_status()
    };
    Ok(status)
}
