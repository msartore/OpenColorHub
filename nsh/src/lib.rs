//! Rust side of the NSH port.
//!
//! The screens are TSX; this crate is the only code that touches hardware,
//! settings files and the `colorctl-core` service. TSX sees one class —
//! `Controller` — through `opencolorhub/native`. Everything it returns crosses
//! as JSON, parsed once in `app/infrastructure/native.ts`; the Rust types stay
//! the authority and no screen can reach a register.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use colorctl_core::about::{AboutInfo, AboutParams};
use colorctl_core::drivers::{self, CachedCatalogue, DriverFile};
use colorctl_core::gpu::GpuZoneState;
use colorctl_core::model::{Channel, ChannelConfig, FanHeader};
use colorctl_core::service::ActionResult;
use colorctl_core::settings::{BackendMode, Settings, SettingsPatch};
use colorctl_core::setups::{Setup, SetupKind, SetupStore};
use colorctl_core::{
    CURVE_POINTS, CurvePoint, FanCurve, FanProfile, PowerWatcher, RestoreReport, ResumeWatch,
    Service, WakeReport,
};
use colorctl_core::{autostart, pawnio_install, platform};
use nsh_core::prelude::*;

/// The crate's UI set: one class, `new Controller()`.
#[native_module("opencolorhub/native")]
fn native(registry: &mut Registry) {
    registry.class::<Controller>();
}

struct Inner {
    /// `None` when the probe failed; `error` says why.
    service: Option<Service>,
    error: Option<String>,
    settings: Settings,
    settings_path: PathBuf,
    settings_warning: Option<String>,
    data_dir: PathBuf,
    /// Watches the gap between ticks: a long one is a suspend, and the
    /// hardware forgets its state across it.
    watch: ResumeWatch,
}

fn app_dirs() -> (PathBuf, PathBuf) {
    // Tests and CI point this at a scratch directory so a headless run cannot
    // rewrite the user's real settings or presets.
    if let Some(root) = std::env::var_os("OPENCOLORHUB_DATA_DIR") {
        let root = PathBuf::from(root);
        return (root.join("config"), root.join("data"));
    }
    if let Some(root) = portable_root() {
        return (root.join("config"), root.join("data"));
    }

    // The same directories the Tauri shell used: the identifier under the
    // platform's config and data roots. Read from the environment rather than
    // NSH's reactive `data_dir()`, which needs an engine runtime this
    // constructor runs before.
    const ID: &str = "dev.msartore.opencolorhub";
    let home = || std::env::var_os("HOME").map(PathBuf::from);

    #[cfg(target_os = "windows")]
    {
        // Tauri's `app_config_dir` and `app_data_dir` both sit under
        // `%APPDATA%\<identifier>` on Windows; keeping the data there means an
        // existing `settings.json` and `rgb.state` carry over untouched.
        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(home)
            .unwrap_or_else(std::env::temp_dir);
        (roaming.join(ID), roaming.join(ID))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let config_root = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|home| home.join(".config")))
            .unwrap_or_else(std::env::temp_dir);
        let data_root = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|home| home.join(".local/share")))
            .unwrap_or_else(std::env::temp_dir);
        (config_root.join(ID), data_root.join(ID))
    }
}

/// Same marker as the Tauri build: a portable copy keeps config and data
/// beside the executable so the folder moves as one.
fn portable_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    dir.join("opencolorhub.portable").is_file().then_some(dir)
}

fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("could not encode the result: {error}"))
}

fn parse_header(id: &str) -> Result<FanHeader, String> {
    FanHeader::from_id(id).ok_or_else(|| format!("unknown fan header: {id}"))
}

fn parse_profile(id: &str) -> Result<FanProfile, String> {
    match id {
        "quiet" => Ok(FanProfile::Quiet),
        "standard" => Ok(FanProfile::Standard),
        "full" => Ok(FanProfile::Full),
        other => Err(format!("unknown fan profile: {other}")),
    }
}

fn parse_channel(id: &str) -> Result<Channel, String> {
    Channel::from_id(id).ok_or_else(|| format!("unknown lighting channel: {id}"))
}

fn parse_color(json: &str) -> Result<[u8; 3], String> {
    let parts: Vec<u8> =
        serde_json::from_str(json).map_err(|error| format!("invalid colour: {error}"))?;
    parts
        .try_into()
        .map_err(|_| "a colour needs exactly three bytes (r, g, b)".to_string())
}

fn parse_level(percent: u32, label: &str) -> Result<u8, String> {
    u8::try_from(percent).map_err(|_| format!("{label} {percent}% is outside 0–100%"))
}

fn parse_setup_kind(id: &str) -> Result<SetupKind, String> {
    match id {
        "fans" => Ok(SetupKind::Fans),
        "lighting" => Ok(SetupKind::Lighting),
        other => Err(format!("unknown setup kind: {other}")),
    }
}

fn parse_gpu_zone(json: &str) -> Result<GpuZoneState, String> {
    serde_json::from_str(json).map_err(|error| format!("invalid GPU zone state: {error}"))
}

/// Replays the persisted live setup in a fresh process. Retries while a
/// re-enumerating controller settles, but a stored setup with no errors breaks
/// on the first attempt so a normal launch is not delayed.
fn restore_at_launch(service: &mut Service) -> RestoreReport {
    let mut report = RestoreReport::default();
    for attempt in 0..6 {
        report = service.restore_active();
        if report.errors.is_empty() {
            break;
        }
        if attempt + 1 < 6 {
            std::thread::sleep(Duration::from_millis(700));
        }
    }
    report
}

/// Re-applies fans and lighting after a resume, retrying while the USB
/// controller re-enumerates. Runs on a blocking thread; notes the outcome so
/// the Dashboard's activity list explains the gap.
fn reassert_after_wake(inner: &Arc<Mutex<Inner>>) -> Option<String> {
    let mut guard = inner.lock().ok()?;
    let service = guard.service.as_mut()?;

    let mut report = WakeReport::default();
    for attempt in 0..6 {
        report = service.reassert_after_wake();
        if report.errors.is_empty() {
            break;
        }
        if attempt + 1 < 6 {
            std::thread::sleep(Duration::from_millis(700));
        }
    }

    let message = if !report.errors.is_empty() {
        format!(
            "Could not re-apply after wake — {}",
            report.errors.join("; ")
        )
    } else if report.restored_something() {
        match (report.fans, report.lighting) {
            (0, _) => "Re-applied lighting after wake".to_string(),
            (fans, true) => format!("Re-applied {fans} fans and lighting after wake"),
            (fans, false) => format!("Re-applied {fans} fans after wake"),
        }
    } else {
        "Woke; nothing needed re-applying".to_string()
    };
    service.note("wake", message.clone());
    Some(message)
}

#[derive(serde::Serialize)]
struct SetupList {
    setups: Vec<Setup>,
    warning: Option<String>,
}

fn setup_list(store: SetupStore, kind: SetupKind) -> SetupList {
    SetupList {
        warning: store.warning.clone(),
        setups: store.of_kind(kind),
    }
}

#[derive(serde::Serialize)]
struct DriverRow {
    #[serde(flatten)]
    file: DriverFile,
    newer: bool,
}

#[derive(serde::Serialize)]
struct DriverList {
    board: String,
    product_id: u32,
    installed_bios: String,
    files: Vec<DriverRow>,
    fetched_at_ms: u64,
    cached: bool,
    warning: Option<String>,
}

fn driver_list(
    cache: CachedCatalogue,
    installed_bios: &str,
    cached: bool,
    warning: Option<String>,
) -> DriverList {
    let files = cache
        .files
        .into_iter()
        .map(|file| DriverRow {
            newer: drivers::is_newer_firmware(&file.title, installed_bios),
            file,
        })
        .collect();
    DriverList {
        board: cache.board,
        product_id: cache.product_id,
        installed_bios: installed_bios.to_string(),
        files,
        fetched_at_ms: cache.fetched_at_ms,
        cached,
        warning,
    }
}

#[derive(serde::Serialize)]
struct StoredDriverFile {
    file_name: String,
    path: String,
    bytes: u64,
}

/// The one object TSX holds. Its Rust value lives on the engine thread; the
/// service is shared with a blocking thread only through this `Arc<Mutex<_>>`.
#[class]
pub struct Controller {
    inner: Arc<Mutex<Inner>>,
    /// OS power notifications. The telemetry loop checks this on every tick so
    /// a resume need not be inferred from a wall-clock gap alone.
    power: PowerWatcher,
    minimized: bool,
    stopped: Arc<AtomicBool>,
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

#[class]
impl Controller {
    /// Probes the hardware once and loads `settings.json`.
    ///
    /// The probe is synchronous and happens as the bundle loads, as the
    /// Tauri build did before its first paint. `OPENCOLORHUB_SIM=1` forces the
    /// simulator, which is how `nsh test` and CI run without hardware.
    pub fn new() -> Self {
        let (config_dir, data_dir) = app_dirs();
        let _ = std::fs::create_dir_all(&config_dir);
        let _ = std::fs::create_dir_all(&data_dir);

        let settings_path = config_dir.join("settings.json");
        let (mut settings, warning) = Settings::load_or_default(&settings_path);
        if let Some(warning) = &warning {
            eprintln!("opencolorhub: settings file problem: {warning} (using defaults)");
        }
        if std::env::var_os("OPENCOLORHUB_SIM").is_some() {
            settings.backend = BackendMode::Simulated;
        }

        let probe_settings = settings.clone();
        let probe_data = data_dir.clone();
        let built = catch_unwind(AssertUnwindSafe(|| {
            Service::new(probe_settings, probe_data)
        }));

        let (service, error) = match built {
            Ok(mut service) => {
                // The live setup is restored on every start, independently of
                // the lighting toggle and autostart. The toggle stays as a
                // fallback for installs that have no active-setup file yet.
                let report = if settings.restore_setup_on_startup {
                    restore_at_launch(&mut service)
                } else {
                    RestoreReport::default()
                };
                if report.found {
                    eprintln!(
                        "opencolorhub: restored saved setup — {} fans, lighting: {}",
                        report.fans, report.lighting
                    );
                }
                // Upgrades from the old shell have `rgb.state` but no active
                // setup yet. Push that blob too so lighting comes back on the
                // first launch after the update.
                let has_saved_lighting = service
                    .state_path()
                    .is_some_and(|path| path.is_file());
                if !report.lighting
                    && (settings.reapply_led_on_launch || has_saved_lighting)
                    && let Err(error) = service.apply_led()
                {
                    eprintln!("opencolorhub: could not re-apply lighting: {error}");
                }
                (Some(service), None)
            }
            Err(_) => (
                None,
                Some("the hardware probe failed — check that the driver is installed".to_string()),
            ),
        };

        Self {
            inner: Arc::new(Mutex::new(Inner {
                service,
                error,
                settings,
                settings_path,
                settings_warning: warning,
                data_dir,
                watch: ResumeWatch::new(Duration::from_millis(1000)),
            })),
            power: PowerWatcher::new(),
            minimized: std::env::args().any(|arg| arg == "--minimized"),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The launch came from the autostart entry; the window stays hidden.
    #[getter]
    pub fn minimized(&self) -> bool {
        self.minimized
    }

    /// The current telemetry snapshot, as JSON.
    pub fn snapshot_json(&self) -> Result<String, String> {
        let mut inner = self.lock()?;
        let service = inner.service_mut()?;
        to_json(&service.snapshot())
    }

    /// The stored preferences, as JSON.
    pub fn settings_json(&self) -> Result<String, String> {
        let inner = self.lock()?;
        to_json(&inner.settings)
    }

    /// A settings file that could not be read, when there was one.
    #[getter]
    pub fn settings_warning(&self) -> Option<String> {
        self.lock()
            .ok()
            .and_then(|inner| inner.settings_warning.clone())
    }

    /// Whether this process is elevated, and whether it could ask.
    pub fn elevation_json(&self) -> Result<String, String> {
        to_json(&platform::state())
    }

    /// Asks the OS for an elevated relaunch. `"launched"` means the caller
    /// must quit: the elevated copy takes over.
    pub fn request_elevation(&self) -> Result<String, String> {
        match platform::request() {
            Ok(platform::Outcome::Launched) => Ok("launched".to_string()),
            Ok(platform::Outcome::Declined) => Ok("declined".to_string()),
            Ok(platform::Outcome::Unsupported) => {
                Err("this build cannot raise a consent prompt".to_string())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    /// The logon entry the OS holds for this user.
    pub fn autostart_json(&self) -> Result<String, String> {
        to_json(&autostart::status())
    }

    /// Creates or removes the logon entry; needs elevation on Windows.
    pub fn set_autostart(&self, enabled: bool) -> Result<String, String> {
        let state = autostart::set_enabled(enabled).map_err(|error| error.to_string())?;
        to_json(&state)
    }

    /// Applies a `SettingsPatch` (JSON) and writes the file atomically.
    pub fn save_settings_json(&self, patch_json: String) -> Result<String, String> {
        let patch: SettingsPatch = serde_json::from_str(&patch_json)
            .map_err(|error| format!("invalid settings patch: {error}"))?;
        let mut inner = self.lock()?;
        let mut settings = inner.settings.clone();
        settings.apply_patch(patch);
        let rebuilt = inner.service_mut()?.update_settings(settings.clone());
        inner.settings = settings.clone();
        settings
            .save(&inner.settings_path)
            .map_err(|error| format!("could not save settings: {error}"))?;
        let _ = rebuilt;
        to_json(&settings)
    }

    /// Build facts, board identity and the paths the app uses, as JSON.
    pub fn about_json(&self) -> Result<String, String> {
        let mut inner = self.lock()?;
        let state_path = inner
            .service_mut()?
            .state_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "—".to_string());
        let config_override = inner
            .settings
            .state_file
            .as_ref()
            .map(|path| path.display().to_string());
        let inner = inner;
        let about = AboutInfo::build(AboutParams {
            name: "OpenColorHub".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            tagline: "Fan and lighting control for Colorful motherboards".to_string(),
            board: inner.service_ref()?.board().clone(),
            tauri_version: None,
            data_dir: inner.data_dir.display().to_string(),
            settings_path: inner.settings_path.display().to_string(),
            state_path,
            config_override,
        });
        to_json(&about)
    }

    /// The activity ring buffer, oldest first, as JSON.
    pub fn activity_json(&self) -> Result<String, String> {
        let inner = self.lock()?;
        to_json(inner.service_ref()?.activity())
    }

    /// Re-reads the saved lighting state and returns a fresh snapshot.
    pub fn reload_state_json(&self) -> Result<String, String> {
        let mut inner = self.lock()?;
        let service = inner.service_mut()?;
        service.reload_led_state();
        to_json(&service.snapshot())
    }

    /// Manual duty. The core applies the pump floor; the TSX preview mirrors it.
    pub fn set_fan_percent(&self, header: String, percent: u32) -> Result<String, String> {
        let header = parse_header(&header)?;
        let percent =
            u8::try_from(percent).map_err(|_| format!("duty {percent}% is outside 0–100%"))?;
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .set_fan_percent(header, percent)
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Four ascending points, as JSON `[{ temp_c, percent }, ...]`.
    pub fn set_fan_curve(&self, header: String, points_json: String) -> Result<String, String> {
        let header = parse_header(&header)?;
        let points: Vec<CurvePoint> = serde_json::from_str(&points_json)
            .map_err(|error| format!("invalid curve: {error}"))?;
        if points.len() != CURVE_POINTS {
            return Err(format!(
                "a curve needs exactly {CURVE_POINTS} points, got {}",
                points.len()
            ));
        }
        let mut curve: FanCurve = [CurvePoint::new(0, 0); CURVE_POINTS];
        curve.copy_from_slice(&points);
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .set_fan_curve(header, &curve)
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Applies a built-in profile to the headers given, as JSON ids.
    pub fn apply_profile(&self, headers_json: String, profile: String) -> Result<String, String> {
        let ids: Vec<String> = serde_json::from_str(&headers_json)
            .map_err(|error| format!("invalid headers: {error}"))?;
        let headers: Vec<FanHeader> = ids
            .iter()
            .map(|id| parse_header(id))
            .collect::<Result<_, _>>()?;
        let profile = parse_profile(&profile)?;
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .apply_profile(&headers, profile)
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// One lighting channel: colour as JSON `[r,g,b]`, brightness, enabled.
    pub fn set_channel(
        &self,
        channel: String,
        color_json: String,
        brightness: u32,
        enabled: bool,
    ) -> Result<String, String> {
        let channel = parse_channel(&channel)?;
        let color = parse_color(&color_json)?;
        let brightness = parse_level(brightness, "brightness")?;
        let config = if enabled {
            ChannelConfig::rgb(color, brightness)
        } else {
            ChannelConfig::off()
        };
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .set_channel(channel, config, true)
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// The same colour and brightness on every channel.
    pub fn set_all_channels(
        &self,
        color_json: String,
        brightness: u32,
        enabled: bool,
    ) -> Result<String, String> {
        let color = parse_color(&color_json)?;
        let brightness = parse_level(brightness, "brightness")?;
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .set_all_channels(color, brightness, enabled)
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Scales every lit channel, keeping the stored colours.
    pub fn set_brightness(&self, percent: u32) -> Result<String, String> {
        let percent = parse_level(percent, "brightness")?;
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .set_global_brightness(percent)
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Turns one channel off without touching the others.
    pub fn turn_off_channel(&self, channel: String) -> Result<String, String> {
        let channel = parse_channel(&channel)?;
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .turn_off_channel(channel)
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Pushes the staged frame to the controller.
    pub fn apply_led(&self) -> Result<String, String> {
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .apply_led()
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Stages or writes one GPU zone. `state_json` is a serde `GpuZoneState`.
    pub fn set_gpu_zone(
        &self,
        device: String,
        zone: String,
        state_json: String,
        apply: bool,
    ) -> Result<String, String> {
        let state = parse_gpu_zone(&state_json)?;
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .set_gpu_zone(device, zone, state, apply)
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Pushes every staged GPU zone to its controller.
    pub fn apply_gpu(&self) -> Result<String, String> {
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .apply_gpu()
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Mirrors the first lit motherboard channel to every GPU zone.
    pub fn sync_gpu_from_board(&self) -> Result<String, String> {
        let mut inner = self.lock()?;
        let result = inner
            .service_mut()?
            .sync_gpu_from_board()
            .map_err(|error| error.to_string())?;
        to_json(&result)
    }

    /// Presets of one half — `"fans"` or `"lighting"` — newest first.
    pub fn list_setups(&self, kind: String) -> Result<String, String> {
        let kind = parse_setup_kind(&kind)?;
        let store = SetupStore::load(self.setups_path()?);
        to_json(&setup_list(store, kind))
    }

    /// Captures what is running now as a preset. An `id` replaces that preset.
    pub fn save_setup(
        &self,
        kind: String,
        name: String,
        id: Option<String>,
    ) -> Result<String, String> {
        let kind = parse_setup_kind(&kind)?;
        let name = name.trim();
        if name.is_empty() {
            return Err("a preset needs a name".to_string());
        }

        let path = self.setups_path()?;
        let mut inner = self.lock()?;
        let mut setup = Setup::capture(inner.service_mut()?, kind, name);
        if let Some(id) = id {
            setup.id = id;
        }
        drop(inner);

        let mut store = SetupStore::load(&path);
        store.upsert(setup).map_err(|error| error.to_string())?;
        to_json(&setup_list(store, kind))
    }

    /// Replays a preset into the service; only its own half is written.
    pub fn apply_setup(&self, id: String) -> Result<String, String> {
        let store = SetupStore::load(self.setups_path()?);
        let setup = store
            .all()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| "that preset no longer exists".to_string())?;

        let mut inner = self.lock()?;
        let message = setup
            .apply(inner.service_mut()?)
            .map_err(|error| error.to_string())?;
        let snapshot = inner.service_mut()?.snapshot();
        to_json(&ActionResult { snapshot, message })
    }

    /// Removes a preset; the list comes back for the open dialog.
    pub fn delete_setup(&self, kind: String, id: String) -> Result<String, String> {
        let kind = parse_setup_kind(&kind)?;
        let mut store = SetupStore::load(self.setups_path()?);
        store.remove(&id).map_err(|error| error.to_string())?;
        to_json(&setup_list(store, kind))
    }

    /// The vendor catalogue for the detected board, cached first unless asked
    /// to refresh. A failed refresh returns the cache with a warning, never an
    /// empty screen. `OPENCOLORHUB_DRIVERS_FIXTURE` serves a file instead, which
    /// is how headless tests stay off the network.
    pub async fn drivers_json(this: Instance<Self>, refresh: bool) -> Result<String, String> {
        if let Some(path) = std::env::var_os("OPENCOLORHUB_DRIVERS_FIXTURE") {
            return std::fs::read_to_string(&path)
                .map_err(|error| format!("could not read the driver fixture: {error}"));
        }

        let Some((board, bios, data_dir)) = this.with(|controller| {
            let inner = controller.lock().ok()?;
            let identity = inner.service_ref().ok()?.board().identity.clone()?;
            Some((identity.name, identity.bios_version, inner.data_dir.clone()))
        }) else {
            return Err("this machine did not report a motherboard model".to_string());
        };
        if board.trim().is_empty() {
            return Err("this machine did not report a motherboard model".to_string());
        }

        let cache_path = data_dir.join("drivers-cache.json");
        if !refresh && let Some(cache) = drivers::load_cache_for(&cache_path, &board) {
            return to_json(&driver_list(cache, &bios, true, None));
        }

        let lookup = board.clone();
        let path = cache_path.clone();
        let fetched = blocking(move || drivers::fetch_and_cache(&lookup, &path))
            .await
            .map_err(|error| format!("driver lookup panicked: {error}"))?;

        match fetched {
            Ok(cache) => to_json(&driver_list(cache, &bios, false, None)),
            Err(error) => match drivers::load_cache_for(&cache_path, &board) {
                Some(cache) => to_json(&driver_list(cache, &bios, true, Some(error.to_string()))),
                None => Err(error.to_string()),
            },
        }
    }

    /// Downloads one catalogue file, posting `{file, received, total}` while it
    /// runs. Nothing is executed; the bytes are written, hashed and reported.
    pub async fn download_driver(
        this: Instance<Self>,
        file_json: String,
        listener: Callback<String>,
    ) -> Result<String, String> {
        let file: DriverFile = serde_json::from_str(&file_json)
            .map_err(|error| format!("invalid driver file: {error}"))?;
        let Some(dir) = this.with(|controller| {
            controller
                .lock()
                .ok()
                .map(|inner| inner.data_dir.join("drivers"))
        }) else {
            return Err("the state lock is poisoned — restart OpenColorHub".to_string());
        };

        let (sender, receiver) = std::sync::mpsc::channel::<String>();
        let name = file.file_name.clone();
        let job = blocking(move || {
            drivers::download_with_progress(&file, &dir, |received, total| {
                let payload = serde_json::json!({
                    "file": name,
                    "received": received,
                    "total": total,
                });
                let _ = sender.send(payload.to_string());
            })
        });

        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        spawn(async move {
            while !flag.load(Ordering::SeqCst) {
                timer(Duration::from_millis(120)).await;
                while let Ok(message) = receiver.try_recv() {
                    listener.post(message);
                }
            }
        });

        let result = job
            .await
            .map_err(|error| format!("download panicked: {error}"))?;
        done.store(true, Ordering::SeqCst);
        match result {
            Ok(saved) => to_json(&saved),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Files already in the download folder, sorted by name.
    pub fn downloaded_drivers_json(&self) -> Result<String, String> {
        let dir = self.lock()?.data_dir.join("drivers");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return to_json(&Vec::<StoredDriverFile>::new());
        };

        let mut files: Vec<StoredDriverFile> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let meta = entry.metadata().ok()?;
                if !meta.is_file() {
                    return None;
                }
                let file_name = path.file_name()?.to_string_lossy().to_string();
                if file_name.ends_with(".part") {
                    return None;
                }
                Some(StoredDriverFile {
                    file_name,
                    path: path.to_string_lossy().to_string(),
                    bytes: meta.len(),
                })
            })
            .collect();
        files.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        to_json(&files)
    }

    /// Shows a downloaded file in the system's file manager.
    pub async fn reveal_driver(this: Instance<Self>, path: String) -> Result<(), String> {
        let _ = this;
        reveal_path(path).await
    }

    /// Whether fan control can reach PawnIO's driver.
    pub fn pawnio_status_json(&self) -> Result<String, String> {
        to_json(&platform::pawnio_status())
    }

    /// Downloads the pinned, verified installer and runs it with its own UI,
    /// posting `{received, total}` while the download runs. Then re-probes the
    /// backends so the caller sees the new status.
    pub async fn install_pawnio_json(
        this: Instance<Self>,
        listener: Callback<String>,
    ) -> Result<String, String> {
        let Some(dir) = this.with(|controller| {
            controller
                .lock()
                .ok()
                .map(|inner| inner.data_dir.join("tools"))
        }) else {
            return Err("the state lock is poisoned — restart OpenColorHub".to_string());
        };

        let (sender, receiver) = std::sync::mpsc::channel::<String>();
        let job = blocking(move || {
            pawnio_install::download(&dir, |received, total| {
                let payload = serde_json::json!({ "received": received, "total": total });
                let _ = sender.send(payload.to_string());
            })
        });

        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        spawn(async move {
            while !flag.load(Ordering::SeqCst) {
                timer(Duration::from_millis(120)).await;
                while let Ok(message) = receiver.try_recv() {
                    listener.post(message);
                }
            }
        });

        let installer = job
            .await
            .map_err(|error| format!("download panicked: {error}"))?
            .map_err(|error| error.to_string())?;
        done.store(true, Ordering::SeqCst);

        blocking(move || pawnio_install::run_installer(&installer))
            .await
            .map_err(|error| format!("installer panicked: {error}"))?
            .map_err(|error| error.to_string())?;

        this.with(|controller| {
            if let Ok(mut inner) = controller.lock()
                && let Ok(service) = inner.service_mut()
            {
                service.reload_backends();
            }
        });
        to_json(&platform::pawnio_status())
    }

    /// Ticks the service off the engine thread and posts a JSON snapshot.
    ///
    /// An async member runs as a task, so `timer` and `blocking` work here.
    /// The loop ends when [`Self::stop_telemetry`] is called (or the app
    /// quits); its promise settles then.
    pub async fn start_telemetry(this: Instance<Self>, listener: Callback<String>) {
        let inner = this.with(|controller| controller.inner.clone());
        let stopped = this.with(|controller| controller.stopped.clone());
        let power = this.with(|controller| controller.power.clone());
        stopped.store(false, Ordering::SeqCst);
        while !stopped.load(Ordering::SeqCst) {
            let interval = inner
                .lock()
                .map(|inner| inner.settings.poll_interval_ms)
                .unwrap_or(1000)
                .clamp(250, 10_000);
            // Mark immediately before the sleep, so the gap the next tick
            // measures is the sleep itself and not the work after it.
            if let Ok(mut inner) = inner.lock() {
                inner.watch.set_interval(Duration::from_millis(interval));
                inner.watch.mark();
            }
            timer(Duration::from_millis(interval)).await;
            if stopped.load(Ordering::SeqCst) {
                break;
            }

            // A tick well past its interval means the machine was asleep. The
            // Super I/O and the USB controller come back without the settings
            // this session wrote, so re-assert them before the next snapshot.
            let slept = inner
                .lock()
                .ok()
                .and_then(|mut inner| inner.watch.observe_now());
            // An OS power notification is authoritative; the wall-clock gap is
            // still there as a fallback if the notification was unavailable.
            let notified = power.take_resume();
            if notified && let Ok(mut inner) = inner.lock() {
                // Re-sync after observing the gap so the next tick does not
                // re-assert a second time for the same wake.
                inner.watch.mark();
            }
            if slept.is_some() || notified {
                let job = inner.clone();
                let report = blocking(move || reassert_after_wake(&job)).await;
                if let Ok(Some(message)) = report {
                    let seconds = slept.map(|gap| gap.as_secs()).unwrap_or(0);
                    let source = if notified { "OS resume" } else { "clock gap" };
                    eprintln!("opencolorhub: woke ({source}, {seconds}s): {message}");
                }
            }

            let job = inner.clone();
            let ticked = blocking(move || {
                let mut inner = job.lock().ok()?;
                inner.service.as_mut().map(|service| service.tick())
            })
            .await;
            let Ok(Some(snapshot)) = ticked else {
                continue;
            };
            if let Ok(json) = serde_json::to_string(&snapshot) {
                listener.post(json);
            }
        }
    }

    /// Ends the telemetry loop.
    pub fn stop_telemetry(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    /// Hands every fan this session took over back to the chip's SmartFan.
    pub fn shutdown(&self) {
        self.stop_telemetry();
        if let Ok(mut inner) = self.inner.lock()
            && let Some(service) = inner.service.as_mut()
        {
            service.shutdown();
        }
    }
}

impl Controller {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>, String> {
        self.inner
            .lock()
            .map_err(|_| "internal state lock poisoned — restart OpenColorHub".to_string())
    }

    fn setups_path(&self) -> Result<PathBuf, String> {
        Ok(self.lock()?.data_dir.join("setups.json"))
    }
}

impl Inner {
    fn service_ref(&self) -> Result<&Service, String> {
        self.service.as_ref().ok_or_else(|| {
            self.error
                .clone()
                .unwrap_or_else(|| "backend unavailable".to_string())
        })
    }

    fn service_mut(&mut self) -> Result<&mut Service, String> {
        let error = self.error.clone();
        self.service
            .as_mut()
            .ok_or_else(|| error.unwrap_or_else(|| "backend unavailable".to_string()))
    }
}
