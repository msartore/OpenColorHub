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
use colorctl_core::model::{Channel, ChannelConfig, FanHeader};
use colorctl_core::settings::{BackendMode, Settings, SettingsPatch};
use colorctl_core::{CURVE_POINTS, CurvePoint, FanCurve, FanProfile, Service};
use colorctl_core::{autostart, platform};
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
}

fn app_dirs() -> (PathBuf, PathBuf) {
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

/// The one object TSX holds. Its Rust value lives on the engine thread; the
/// service is shared with a blocking thread only through this `Arc<Mutex<_>>`.
#[class]
pub struct Controller {
    inner: Arc<Mutex<Inner>>,
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
                if settings.reapply_led_on_launch
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
            })),
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

    /// Ticks the service off the engine thread and posts a JSON snapshot.
    ///
    /// An async member runs as a task, so `timer` and `blocking` work here.
    /// The loop ends when [`Self::stop_telemetry`] is called (or the app
    /// quits); its promise settles then.
    pub async fn start_telemetry(this: Instance<Self>, listener: Callback<String>) {
        let inner = this.with(|controller| controller.inner.clone());
        let stopped = this.with(|controller| controller.stopped.clone());
        stopped.store(false, Ordering::SeqCst);
        while !stopped.load(Ordering::SeqCst) {
            let interval = inner
                .lock()
                .map(|inner| inner.settings.poll_interval_ms)
                .unwrap_or(1000)
                .clamp(250, 10_000);
            timer(Duration::from_millis(interval)).await;
            if stopped.load(Ordering::SeqCst) {
                break;
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
