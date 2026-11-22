//! The façade both frontends (CLI and Tauri GUI) drive.
//!
//! A `Service` owns the backends, the user settings and a little bit of derived
//! state (smoothed readings, last error). Every mutating call returns a fresh
//! [`Snapshot`] so a caller can never act on stale data, and every call is
//! safe to make when hardware is missing — it returns an error instead of
//! panicking, which is what the GUI renders in its banner.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::active::{self, ActiveSetup, RestoreReport};
use crate::backend::{self, FanBackend, LedBackend};
use crate::board::{self, BoardInfo};
use crate::curve::{self, FanCurve, FanProfile};
use crate::error::{Error, Result};
use crate::gpu::{GpuLedBackend, GpuLedState, GpuStatus, GpuZoneState};
use crate::model::{
    Capabilities, Channel, ChannelConfig, DeviceInfo, FanHeader, FanMode, FanSample, FanStatus,
    Snapshot, Telemetry, now_ms,
};
use crate::settings::{BackendMode, Settings};
use crate::state::RgbState;

/// Smoothing factor for the EMA (0..1; higher = more responsive).
const EMA_ALPHA: f32 = 0.45;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    pub snapshot: Snapshot,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WakeReport {
    /// Channels whose duty or curve was written again.
    pub fans: usize,
    /// Whether the current lighting frame was pushed to the controller.
    pub lighting: bool,
    /// One line per half that could not be restored.
    pub errors: Vec<String>,
}

impl WakeReport {
    pub fn restored_something(&self) -> bool {
        self.fans > 0 || self.lighting
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub at_ms: u64,
    pub kind: String,
    pub message: String,
}

/// Below this duty a fan reporting 0 rpm proves nothing.
const STALL_MIN_DUTY: u8 = 25;
/// Tach counts below this are not rotation: a stopped fan can still read a few
/// counts from bus noise, and smoothing delays the fall to zero.
const STALL_RPM: u16 = 50;
/// How long a fan may report nothing before it is called stalled.
const STALL_ALARM_MS: u64 = 5_000;

/// Whether a tick should re-push the frame because the controller went away.
fn should_recover(led_control: bool, lighting_in_use: bool, device_present: bool) -> bool {
    led_control && lighting_in_use && !device_present
}

pub struct Service {
    fan: Box<dyn FanBackend>,
    led: Box<dyn LedBackend>,
    /// Optional GPU RGB transport (AMD ADL / NVIDIA NVAPI / Intel IGCL).
    gpu: GpuLedBackend,
    capabilities: Capabilities,
    /// SMBIOS identity of the host board, detected once at startup.
    board: BoardInfo,
    settings: Settings,
    data_dir: PathBuf,
    /// The live setup is written here after every successful hardware write so
    /// the next process — a restart, an update, an elevation relaunch — can put
    /// the machine back the way the user left it.
    active_path: PathBuf,
    smoothed: Option<Telemetry>,
    /// Bit per header index, set once that header reports a tach. Latched so a
    /// fan stopped by a 0% curve or a 0 rpm reading does not lose its card.
    seen_fans: u8,
    /// When each header first reported 0 rpm while being asked to spin.
    stall_since: [Option<u64>; crate::model::FAN_HEADERS.len()],
    stall_alerted: [bool; crate::model::FAN_HEADERS.len()],
    released: bool,
    /// Whether this session pushed (or tried to push) a lighting frame; a
    /// resume re-pushes it, but lighting a session never touched is left on the
    /// BIOS's own pattern.
    lighting_in_use: bool,
    last_read_error: Option<String>,
    activity: Vec<Activity>,
}

impl Service {
    pub fn new(settings: Settings, data_dir: impl Into<PathBuf>) -> Self {
        let board = board::detect();
        Self::with_board(settings, data_dir, board)
    }

    /// Same as [`Service::new`] with an injected board identity (tests, demo
    /// mode, or a caller that already detected the board).
    pub fn with_board(settings: Settings, data_dir: impl Into<PathBuf>, board: BoardInfo) -> Self {
        let data_dir = data_dir.into();
        let mut set = backend::build(
            settings.backend,
            settings.state_file.as_deref(),
            &data_dir,
            &board,
        );

        // Keep the live setup beside the CCTL state file, so a CLI write and a
        // GUI launch that share `rgb.state` share this too.
        let active_path = set
            .led
            .state_path()
            .map(|path| path.with_file_name("active-setup.json"))
            .unwrap_or_else(|| data_dir.join("active-setup.json"));
        set.capabilities.notes.insert(
            0,
            format!(
                "Board: {} ({}, {} series)",
                board.display_name(),
                board.confidence.label(),
                board.series.as_deref().unwrap_or("unknown")
            ),
        );
        if board.confidence != crate::board::BoardConfidence::Verified {
            for note in &board.notes {
                set.capabilities.notes.push(note.clone());
            }
        }

        let gpu_state_path = settings
            .state_file
            .as_deref()
            .map(|path| path.with_file_name("gpu-led.state"))
            .unwrap_or_else(|| data_dir.join("gpu-led.state"));
        let mut gpu =
            GpuLedBackend::detect(gpu_state_path, settings.backend == BackendMode::Simulated);
        gpu.state_mut().enabled = settings.gpu_led_enabled;

        Self {
            fan: set.fan,
            led: set.led,
            gpu,
            capabilities: set.capabilities,
            board,
            settings,
            active_path,
            data_dir,
            smoothed: None,
            seen_fans: 0,
            stall_since: [None; crate::model::FAN_HEADERS.len()],
            stall_alerted: [false; crate::model::FAN_HEADERS.len()],
            released: false,
            lighting_in_use: false,
            last_read_error: None,
            activity: Vec::new(),
        }
    }

    /// Raw Super I/O dump, when the active backend can produce one.
    pub fn register_dump(&self) -> Option<Result<Vec<crate::model::RegisterRead>>> {
        self.fan.register_dump()
    }

    /// Hands the fans back to the chip's own control before the process exits.
    ///
    /// Explicit rather than `Drop`: the CLI writes a duty and exits on purpose,
    /// and releasing on drop would immediately undo what it was asked to do.
    /// The desktop app calls this from its exit event. A killed process cannot
    /// run it, and the UI does not pretend otherwise.
    pub fn shutdown(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        if self.settings.release_fans_on_exit {
            self.fan.release();
            self.record("fan", "Fans handed back to firmware on exit");
        }
    }

    /// Duties and curves the fan backends are actually driving. The active
    /// setup stores only these, never the zero-duty placeholder of an untouched
    /// header.
    pub fn programmed_fans(&self) -> Vec<crate::setups::FanSetting> {
        self.fan.programmed()
    }

    /// Saves the live setup after a successful write. Best effort: a full disk
    /// must never turn a hardware write that already succeeded into an error.
    fn persist_active(&mut self) {
        let setup = ActiveSetup::capture(self);
        if let Err(error) = setup.persist(&self.active_path) {
            let message = format!("Could not save the active setup: {error}");
            // A failing disk must not turn a successful write into an error, and
            // repeating the same line on every change would flood the log.
            if self
                .activity
                .last()
                .is_none_or(|entry| entry.message != message)
            {
                self.record("settings", message);
            }
        }
    }

    /// Path of the persisted live setup.
    pub fn active_path(&self) -> &Path {
        &self.active_path
    }

    /// Replays the stored setup in a fresh process. Fans and lighting are
    /// independent: a failure in one half is reported but does not stop the
    /// other, and every successful write re-persists the file.
    pub fn restore_active(&mut self) -> RestoreReport {
        let Some(setup) = ActiveSetup::load(&self.active_path) else {
            return RestoreReport::default();
        };
        let report = active::restore(&setup, self);
        if report.restored_something() {
            let message = match (report.fans, report.lighting) {
                (0, _) => "Restored saved lighting at launch".to_string(),
                (fans, true) => {
                    format!("Restored saved setup at launch — {fans} fans and lighting")
                }
                (fans, false) => format!("Restored saved setup at launch — {fans} fans"),
            };
            self.record("restore", message);
        }
        if !report.errors.is_empty() {
            self.record(
                "restore",
                format!(
                    "Could not fully restore the saved setup: {}",
                    report.errors.join("; ")
                ),
            );
        }
        report
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn board(&self) -> &BoardInfo {
        &self.board
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn state_path(&self) -> Option<PathBuf> {
        self.led.state_path()
    }

    pub fn led_state(&self) -> &RgbState {
        self.led.state()
    }

    pub fn activity(&self) -> &[Activity] {
        &self.activity
    }

    pub fn update_settings(&mut self, settings: Settings) -> bool {
        let needs_rebuild = settings.backend != self.settings.backend
            || settings.state_file != self.settings.state_file
            || settings.gpu_led_enabled != self.settings.gpu_led_enabled;
        // A GPU toggle does not need a fresh hardware probe: keep the detected
        // transports and their state, only flip the master enable.
        if settings.gpu_led_enabled != self.settings.gpu_led_enabled {
            self.gpu.state_mut().enabled = settings.gpu_led_enabled;
        }
        self.settings = settings;
        if needs_rebuild {
            let set = backend::build(
                self.settings.backend,
                self.settings.state_file.as_deref(),
                &self.data_dir,
                &self.board,
            );
            self.active_path = set
                .led
                .state_path()
                .map(|path| path.with_file_name("active-setup.json"))
                .unwrap_or_else(|| self.data_dir.join("active-setup.json"));
            self.fan = set.fan;
            self.led = set.led;
            self.gpu = GpuLedBackend::detect(
                self.settings
                    .state_file
                    .as_deref()
                    .map(|path| path.with_file_name("gpu-led.state"))
                    .unwrap_or_else(|| self.data_dir.join("gpu-led.state")),
                self.settings.backend == BackendMode::Simulated,
            );
            self.gpu.state_mut().enabled = self.settings.gpu_led_enabled;
            self.capabilities = set.capabilities;
            self.smoothed = None;
            self.last_read_error = None;
            self.record("settings", "Backend rebuilt from settings");
        }
        needs_rebuild
    }

    /// Used after installing a driver: the previous probe already cached its
    /// verdict, so the only way to pick the new device up is to rebuild.
    pub fn reload_backends(&mut self) -> bool {
        let set = backend::build(
            self.settings.backend,
            self.settings.state_file.as_deref(),
            &self.data_dir,
            &self.board,
        );
        self.active_path = set
            .led
            .state_path()
            .map(|path| path.with_file_name("active-setup.json"))
            .unwrap_or_else(|| self.data_dir.join("active-setup.json"));
        self.fan = set.fan;
        self.led = set.led;
        self.gpu = GpuLedBackend::detect(
            self.settings
                .state_file
                .as_deref()
                .map(|path| path.with_file_name("gpu-led.state"))
                .unwrap_or_else(|| self.data_dir.join("gpu-led.state")),
            self.settings.backend == BackendMode::Simulated,
        );
        self.gpu.state_mut().enabled = self.settings.gpu_led_enabled;
        self.capabilities = set.capabilities;
        self.smoothed = None;
        self.last_read_error = None;
        self.record("settings", "Hardware re-probed");
        true
    }

    fn record(&mut self, kind: &str, message: impl Into<String>) {
        self.activity.push(Activity {
            at_ms: now_ms(),
            kind: kind.to_string(),
            message: message.into(),
        });
        if self.activity.len() > 40 {
            self.activity.remove(0);
        }
    }

    pub fn read_telemetry(&mut self) -> Result<Telemetry> {
        let fresh = self.fan.read()?;
        let telemetry = if self.settings.smoothing {
            match self.smoothed.take() {
                Some(prev) => Self::blend(prev, fresh),
                None => fresh,
            }
        } else {
            fresh
        };
        self.smoothed = Some(telemetry.clone());
        self.last_read_error = None;
        Ok(telemetry)
    }

    fn blend(prev: Telemetry, fresh: Telemetry) -> Telemetry {
        let temperature_c = match (prev.temperature_c, fresh.temperature_c) {
            (Some(a), Some(b)) => Some(a + (b - a) * EMA_ALPHA),
            (None, b) => b,
            (a, None) => a,
        };
        let cpu_temp_c = match (prev.cpu_temp_c, fresh.cpu_temp_c) {
            (Some(a), Some(b)) => Some(a + (b - a) * EMA_ALPHA),
            (None, b) => b,
            (a, None) => a,
        };

        let fans = fresh
            .fans
            .into_iter()
            .map(|f| {
                let rpm = prev
                    .fans
                    .iter()
                    .find(|p| p.header == f.header)
                    .map(|p| {
                        (p.rpm as f32 + (f.rpm as f32 - p.rpm as f32) * EMA_ALPHA).round() as u16
                    })
                    .unwrap_or(f.rpm);
                FanSample { rpm, ..f }
            })
            .collect();

        Telemetry {
            temperature_source: fresh.temperature_source.clone(),
            temperature_register: fresh.temperature_register.clone(),
            board_temp_c: temperature_c,
            temperature_c,
            cpu_temp_c,
            cpu_temp_source: fresh.cpu_temp_source.clone(),
            fans,
        }
    }

    /// Flags fans that are being asked to spin but report nothing.
    ///
    /// A fan that is commanded 30% and reports 0 rpm for five seconds is either
    /// unplugged, seized, or was never spun up — all of which need a human, and
    /// none of which the duty register can reveal.
    fn check_stalls(&mut self, fans: &mut [FanStatus]) {
        let now = now_ms();
        for fan in fans.iter_mut() {
            let index = fan.header.index();
            let suspect = fan.detected && fan.rpm < STALL_RPM && fan.percent >= STALL_MIN_DUTY;
            if !suspect {
                self.stall_since[index] = None;
                self.stall_alerted[index] = false;
                fan.stalled = false;
                continue;
            }
            let since = *self.stall_since[index].get_or_insert(now);
            if now.saturating_sub(since) >= STALL_ALARM_MS {
                fan.stalled = true;
                if !self.stall_alerted[index] {
                    self.stall_alerted[index] = true;
                    self.record(
                        "fan",
                        format!(
                            "{} reports 0 rpm at {}% — it may be unplugged or seized",
                            fan.label, fan.percent
                        ),
                    );
                }
            }
        }
    }

    fn latch_detection(&mut self, fans: &mut [FanStatus]) {
        let mut seen = self.seen_fans;
        for fan in fans.iter_mut() {
            let bit = 1u8 << fan.header.index();
            if fan.rpm > 0 {
                seen |= bit;
            }
            fan.detected = seen & bit != 0;
        }
        self.seen_fans = seen;
    }

    /// Full UI snapshot. Never fails: read errors are reported inside it.
    pub fn snapshot(&mut self) -> Snapshot {
        let telemetry = match self.read_telemetry() {
            Ok(t) => Some(t),
            Err(e) => {
                self.last_read_error = Some(e.to_string());
                None
            }
        };

        let temp = telemetry.as_ref().and_then(|t| t.temperature_c);
        let temp_source = telemetry
            .as_ref()
            .and_then(|t| t.temperature_source.clone());
        let temp_register = telemetry
            .as_ref()
            .and_then(|t| t.temperature_register.clone());
        let board_temp = telemetry.as_ref().and_then(|t| t.board_temp_c).or(temp);
        let cpu_temp = telemetry.as_ref().and_then(|t| t.cpu_temp_c);
        let cpu_temp_source = telemetry
            .as_ref()
            .and_then(|t| t.cpu_temp_source.clone());
        let samples = telemetry.map(|t| t.fans).unwrap_or_default();

        let mut fans: Vec<FanStatus> = samples
            .iter()
            .map(|s| {
                let curve_percent = s
                    .curve
                    .as_ref()
                    .map(|c| curve::eval(c, temp.unwrap_or(0.0)));
                FanStatus {
                    header: s.header,
                    label: s.header.label().to_string(),
                    description: s.header.description().to_string(),
                    bank: s.bank,
                    pump: self.settings.is_pump(s.header),
                    rpm: s.rpm,
                    pwm: s.pwm,
                    percent: s.percent(),
                    mode: s.mode,
                    curve: s.curve,
                    curve_percent,
                    detected: false,
                    stalled: false,
                }
            })
            .collect();

        // Guarantee the UI always has a card per header, even when the backend
        // returned fewer samples (degraded/unavailable backends).
        for header in FanHeader::all() {
            if !fans.iter().any(|f| f.header == *header) {
                fans.push(FanStatus {
                    header: *header,
                    label: header.label().to_string(),
                    description: header.description().to_string(),
                    bank: crate::chipmap::bank(*header),
                    pump: self.settings.is_pump(*header),
                    rpm: 0,
                    pwm: 0,
                    percent: 0,
                    mode: FanMode::Unknown,
                    curve: None,
                    curve_percent: None,
                    detected: false,
                    stalled: false,
                });
            }
        }
        fans.sort_by_key(|f| f.header.index());
        self.latch_detection(&mut fans);
        self.check_stalls(&mut fans);

        // Averaged over the headers that exist: empty sockets would otherwise
        // drag the figure toward zero.
        let present: Vec<&FanStatus> = fans.iter().filter(|f| f.detected).collect();
        let pool: Vec<&FanStatus> = if present.is_empty() {
            fans.iter().collect()
        } else {
            present
        };
        let avg_percent = if pool.is_empty() {
            0
        } else {
            (pool.iter().map(|f| f.percent as u32).sum::<u32>() / pool.len() as u32) as u8
        };

        Snapshot {
            timestamp_ms: now_ms(),
            temperature_c: temp,
            temperature_source: temp_source,
            temperature_register: temp_register,
            board_temp_c: board_temp,
            cpu_temp_c: cpu_temp,
            cpu_temp_source: cpu_temp_source,
            fans,
            channels: self.led.state().statuses(),
            fan_device: self.fan_info(),
            led_device: self.led_info(),
            gpu_led: Some(self.gpu.status()),
            capabilities: self.capabilities.clone(),
            avg_percent,
            read_error: self.last_read_error.clone(),
            board: self.board.clone(),
        }
    }

    fn fan_info(&self) -> DeviceInfo {
        let mut info = self.fan.info();
        if let Some(err) = &self.last_read_error {
            info.error = Some(err.clone());
        }
        info
    }

    fn led_info(&self) -> DeviceInfo {
        self.led.info()
    }

    pub fn set_fan_pwm(&mut self, header: FanHeader, pwm: u8) -> Result<ActionResult> {
        let percent = crate::units::pwm_to_pct(pwm);
        let clamped = self
            .settings
            .clamp_duty(self.settings.is_pump(header), percent);
        let effective = if clamped != percent {
            crate::units::pct(clamped)
        } else {
            pwm
        };

        self.fan.set_pwm(header, effective)?;
        self.persist_active();
        let message = format!(
            "{} set to PWM {} (~{}%)",
            header.label(),
            effective,
            crate::units::pwm_to_pct(effective)
        );
        self.record("fan", message.clone());
        Ok(ActionResult {
            snapshot: self.snapshot(),
            message,
        })
    }

    pub fn set_fan_percent(&mut self, header: FanHeader, percent: u8) -> Result<ActionResult> {
        let clamped = self
            .settings
            .clamp_duty(self.settings.is_pump(header), percent.min(100));
        self.set_fan_pwm(header, crate::units::pct(clamped))
    }

    pub fn set_fan_curve(&mut self, header: FanHeader, curve: &FanCurve) -> Result<ActionResult> {
        curve::validate(curve)?;
        if curve.iter().any(|p| p.percent < 60) && self.settings.is_pump(header) {
            return Err(Error::invalid(
                "AIO pump curves must stay at or above 60% — refusing to stall the pump",
            ));
        }
        self.fan.set_curve(header, curve)?;
        self.persist_active();
        let message = format!("{} curve set to {}", header.label(), curve::format(curve));
        self.record("fan", message.clone());
        Ok(ActionResult {
            snapshot: self.snapshot(),
            message,
        })
    }

    pub fn apply_profile(
        &mut self,
        headers: &[FanHeader],
        profile: FanProfile,
    ) -> Result<ActionResult> {
        if !self.fan.supports_curves() {
            return Err(Error::unavailable(
                "The current backend cannot program fan curves",
            ));
        }
        let mut applied = 0;
        let mut last_error = None;
        for header in headers {
            let curve = profile.points_for(self.settings.is_pump(*header));
            match self.fan.set_curve(*header, &curve) {
                Ok(()) => applied += 1,
                Err(e) => last_error = Some(e),
            }
        }
        if applied == 0 {
            return Err(last_error.unwrap_or_else(|| {
                Error::backend("No headers were programmed (empty selection)")
            }));
        }
        self.persist_active();
        let message = format!(
            "Applied '{}' profile to {} header{}",
            profile.label(),
            applied,
            if applied == 1 { "" } else { "s" }
        );
        self.record("fan", message.clone());
        Ok(ActionResult {
            snapshot: self.snapshot(),
            message,
        })
    }

    /// Stages one channel in memory; no HID write.
    pub fn stage_channel(&mut self, channel: Channel, config: ChannelConfig) {
        self.led.state_mut().set(channel, config);
    }

    pub fn set_channel(
        &mut self,
        channel: Channel,
        config: ChannelConfig,
        apply: bool,
    ) -> Result<ActionResult> {
        self.stage_channel(channel, config);
        if !apply {
            return Ok(ActionResult {
                snapshot: self.snapshot(),
                message: format!("{} staged", channel.label()),
            });
        }
        let mut outcome = self.apply_led()?;
        let state = if config.enabled {
            format!(
                "{} set to {} at {}%",
                channel.label(),
                config.hex(),
                config.brightness
            )
        } else {
            format!("{} turned off", channel.label())
        };
        self.record("led", state.clone());
        outcome.message = state;
        Ok(outcome)
    }

    pub fn set_all_channels(
        &mut self,
        color: [u8; 3],
        brightness: u8,
        enabled: bool,
    ) -> Result<ActionResult> {
        {
            let state = self.led.state_mut();
            for ch in Channel::all() {
                let cfg = if enabled {
                    ChannelConfig::rgb(color, brightness)
                } else {
                    ChannelConfig::off()
                };
                state.set(*ch, cfg);
            }
        }
        let mut outcome = self.apply_led()?;
        outcome.message = if enabled {
            format!(
                "All channels set to {} at {brightness}%",
                ChannelConfig::rgb(color, brightness).hex()
            )
        } else {
            "All lighting turned off".to_string()
        };
        self.record("led", outcome.message.clone());
        Ok(outcome)
    }

    pub fn set_global_brightness(&mut self, brightness: u8) -> Result<ActionResult> {
        self.led.state_mut().set_global_brightness(brightness);
        let mut outcome = self.apply_led()?;
        outcome.message = format!("Brightness set to {brightness}%");
        self.record("led", outcome.message.clone());
        Ok(outcome)
    }

    pub fn turn_off_channel(&mut self, channel: Channel) -> Result<ActionResult> {
        self.led.state_mut().turn_off(channel);
        let mut outcome = self.apply_led()?;
        outcome.message = format!("{} turned off", channel.label());
        self.record("led", outcome.message.clone());
        Ok(outcome)
    }

    /// Pushes the lighting state to the controller, which persists it.
    pub fn apply_led(&mut self) -> Result<ActionResult> {
        if !self.capabilities.led_control {
            return Err(Error::unavailable(
                "LED control is unavailable with the current backend",
            ));
        }
        self.led.apply()?;
        self.lighting_in_use = true;
        self.persist_active();
        Ok(ActionResult {
            snapshot: self.snapshot(),
            message: "Lighting applied".to_string(),
        })
    }

    // -----------------------------------------------------------------------
    // GPU RGB
    // -----------------------------------------------------------------------

    pub fn gpu_state(&self) -> &GpuLedState {
        self.gpu.state()
    }

    pub fn gpu_status(&self) -> GpuStatus {
        self.gpu.status()
    }

    /// Stages one GPU zone in memory. `apply` pushes it to the card at once.
    pub fn set_gpu_zone(
        &mut self,
        device: String,
        zone: String,
        state: GpuZoneState,
        apply: bool,
    ) -> Result<ActionResult> {
        if !self.settings.gpu_led_enabled {
            return Err(Error::unavailable(
                "GPU LED control is disabled in settings",
            ));
        }
        if !self.gpu.status().devices.iter().any(|d| d.id == device) {
            return Err(Error::unavailable(format!("Unknown GPU device '{device}'")));
        }
        self.gpu
            .state_mut()
            .zones
            .insert(format!("{device}:{zone}"), state);
        if !apply {
            return Ok(ActionResult {
                snapshot: self.snapshot(),
                message: format!("GPU {zone} staged"),
            });
        }
        let mut outcome = self.apply_gpu()?;
        outcome.message = format!("GPU {zone} set to {}", state.hex());
        Ok(outcome)
    }

    /// Mirrors a board channel's colour to every GPU zone. Used by the UI's
    /// "sync with board" action.
    pub fn sync_gpu_from_board(&mut self) -> Result<ActionResult> {
        if !self.settings.gpu_led_enabled {
            return Err(Error::unavailable(
                "GPU LED control is disabled in settings",
            ));
        }
        let source = self
            .led
            .state()
            .statuses()
            .into_iter()
            .find(|c| c.enabled)
            .map(|c| c.color)
            .unwrap_or([255, 255, 255]);
        let mut count = 0usize;
        for device in self.gpu.status().devices {
            for zone in device.zones {
                let mut state = self
                    .gpu
                    .state()
                    .zones
                    .get(&format!("{}:{}", device.id, zone.id))
                    .copied()
                    .unwrap_or_default();
                state.color = source;
                state.enabled = true;
                self.gpu
                    .state_mut()
                    .zones
                    .insert(format!("{}:{}", device.id, zone.id), state);
                count += 1;
            }
        }
        if count == 0 {
            return Err(Error::unavailable("No GPU RGB controller detected"));
        }
        let mut outcome = self.apply_gpu()?;
        outcome.message = format!("Synced {count} GPU zone(s) with the board colour");
        Ok(outcome)
    }

    /// Pushes every GPU zone to its controller and persists the state file.
    pub fn apply_gpu(&mut self) -> Result<ActionResult> {
        if !self.settings.gpu_led_enabled {
            return Err(Error::unavailable(
                "GPU LED control is disabled in settings",
            ));
        }
        let written = self.gpu.apply()?;
        if written > 0 {
            self.persist_active();
        }
        let message = format!("GPU lighting applied to {written} zone(s)");
        self.record("gpu", message.clone());
        Ok(ActionResult {
            snapshot: self.snapshot(),
            message,
        })
    }

    /// A sleep power-cycles the USB bus and reloads the Super I/O's defaults, so
    /// the frame is gone and the fans are back under firmware control — while
    /// everything this process remembers still describes the state it left
    /// behind. Only what the session actually applied is restored: a session
    /// that never touched a half leaves that half alone.
    ///
    /// No retries and no activity entry here: the shell retries while a
    /// re-enumerating device settles, and records one entry for the outcome.
    pub fn reassert_after_wake(&mut self) -> WakeReport {
        let mut report = WakeReport::default();

        match self.fan.reassert() {
            Ok(fans) => report.fans = fans,
            Err(error) => report.errors.push(format!("fans: {error}")),
        }

        self.reassert_lighting(&mut report);
        if self.settings.gpu_led_enabled
            && self.gpu.state().enabled
            && let Err(error) = self.gpu.reassert()
        {
            report.errors.push(format!("gpu: {error}"));
        }
        report
    }

    /// Re-applies the frame when the controller vanished and came back.
    ///
    /// A resume does not always produce a wall-clock gap the ticker can see: a
    /// two-second S3 blip, a hub power dip or a controller reset all invalidate
    /// the HID handle and drop the frame while the process keeps running. One
    /// attribute query per tick notices, and nothing is written unless the
    /// handle is actually dead.
    pub fn recover_if_needed(&mut self) -> Option<WakeReport> {
        if !should_recover(
            self.capabilities.led_control,
            self.lighting_in_use,
            self.led.device_present(),
        ) {
            return None;
        }
        let mut report = WakeReport::default();
        self.reassert_lighting(&mut report);
        Some(report)
    }

    fn reassert_lighting(&mut self, report: &mut WakeReport) {
        if !(self.lighting_in_use && self.capabilities.led_control) {
            return;
        }
        match self.led.reassert() {
            Ok(()) => report.lighting = true,
            Err(error) => report.errors.push(format!("lighting: {error}")),
        }
    }

    pub fn tick(&mut self) -> Snapshot {
        // A Super I/O reset need not move the wall clock. The backend's own
        // health probe catches it on the next tick and replays the live setup.
        if !self.fan.healthy() {
            let report = self.reassert_after_wake();
            let message = if report.errors.is_empty() {
                format!(
                    "Re-applied the saved setup after the fan controller returned — {} fans, lighting: {}",
                    report.fans, report.lighting
                )
            } else {
                format!(
                    "Fan controller did not answer — {}",
                    report.errors.join("; ")
                )
            };
            if self
                .activity
                .last()
                .is_none_or(|entry| entry.message != message)
            {
                self.record("wake", message);
            }
        }
        if let Some(report) = self.recover_if_needed()
            && report.lighting
        {
            self.record(
                "wake",
                "Re-applied lighting after the controller re-appeared",
            );
        }
        self.snapshot()
    }

    pub fn note(&mut self, kind: &str, message: impl Into<String>) {
        self.record(kind, message);
    }

    pub fn reload_led_state(&mut self) {
        let loaded = self
            .led
            .state_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| RgbState::from_bytes(&bytes));
        if let Some(state) = loaded {
            *self.led.state_mut() = state;
        }
    }
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("board", &self.board.display_name())
            .field("fan", &self.fan.info().name)
            .field("led", &self.led.info().name)
            .field("settings", &self.settings)
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::BackendMode;

    fn service(name: &str) -> (Service, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("opencolorhub-svc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let settings = Settings {
            backend: BackendMode::Simulated,
            smoothing: false,
            ..Settings::default()
        };
        (Service::new(settings, dir.clone()), dir)
    }

    fn restart(dir: &std::path::Path) -> Service {
        let settings = Settings {
            backend: BackendMode::Simulated,
            smoothing: false,
            ..Settings::default()
        };
        Service::new(settings, dir)
    }

    #[test]
    fn settings_default_to_restoring_and_keeping_the_setup() {
        let defaults = Settings::default();
        assert!(defaults.restore_setup_on_startup);
        assert!(!defaults.release_fans_on_exit);
    }

    #[test]
    fn the_live_setup_survives_a_restart() {
        let (mut first, dir) = service("restart");
        first.set_fan_percent(FanHeader::Cpu, 63).unwrap();
        first
            .set_channel(Channel::Led, ChannelConfig::rgb([9, 8, 7], 55), true)
            .unwrap();
        assert!(dir.join("active-setup.json").exists());

        let mut second = restart(&dir);
        let report = second.restore_active();
        assert!(report.found);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.fans, 1);
        assert!(report.lighting);

        let snapshot = second.snapshot();
        let cpu = snapshot
            .fans
            .iter()
            .find(|fan| fan.header == FanHeader::Cpu)
            .unwrap();
        assert_eq!(cpu.percent, 63);
        assert_eq!(snapshot.channels[0].color, [9, 8, 7]);
        assert!(
            second.activity().iter().any(|entry| entry.kind == "restore"
                && entry.message.contains("Restored saved setup at launch")),
            "the restore is visible in the activity feed"
        );
    }

    #[test]
    fn shutdown_hands_fans_back_only_when_asked() {
        let (mut svc, _dir) = service("release");
        svc.set_fan_percent(FanHeader::Cpu, 40).unwrap();
        svc.shutdown();
        assert!(
            !svc.activity()
                .iter()
                .any(|entry| entry.message.contains("handed back")),
            "the default keeps the user's setup in force"
        );

        let dir =
            std::env::temp_dir().join(format!("opencolorhub-svc-release-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let settings = Settings {
            backend: BackendMode::Simulated,
            release_fans_on_exit: true,
            ..Settings::default()
        };
        let mut explicit = Service::new(settings, dir);
        explicit.set_fan_percent(FanHeader::Cpu, 40).unwrap();
        explicit.shutdown();
        assert!(
            explicit
                .activity()
                .iter()
                .any(|entry| entry.message.contains("handed back"))
        );
    }

    fn fan_status(header: FanHeader, rpm: u16) -> FanStatus {
        FanStatus {
            header,
            label: header.label().to_string(),
            description: header.description().to_string(),
            bank: crate::chipmap::bank(header),
            pump: false,
            rpm,
            pwm: 0,
            percent: 50,
            detected: false,
            stalled: false,
            mode: FanMode::Manual,
            curve: None,
            curve_percent: None,
        }
    }

    #[test]
    fn detection_latches_and_leaves_silent_headers_alone() {
        let (mut svc, _dir) = service("detect");
        let mut fans = vec![
            fan_status(FanHeader::Cpu, 900),
            fan_status(FanHeader::Aux1, 0),
        ];

        svc.latch_detection(&mut fans);
        assert!(fans[0].detected, "a spinning header is a fan");
        assert!(!fans[1].detected, "a silent header is an empty socket");

        fans[0].rpm = 0;
        svc.latch_detection(&mut fans);
        assert!(fans[0].detected, "a stopped fan is still a fan");
        assert!(!fans[1].detected, "silence never becomes presence");
    }

    #[test]
    fn a_fan_asked_to_spin_without_a_tach_is_flagged_as_stalled() {
        let (mut svc, _dir) = service("stall");

        let mut spinning = fan_status(FanHeader::Cpu, 900);
        spinning.detected = true;
        spinning.percent = 50;
        let mut silent = fan_status(FanHeader::Aux0, 0);
        silent.detected = true;
        silent.percent = 50;
        let mut parked = fan_status(FanHeader::Aux1, 0);
        parked.detected = true;
        parked.percent = 0;
        let mut empty = fan_status(FanHeader::Aux2, 0);
        empty.detected = false;
        empty.percent = 50;

        let mut fans = vec![spinning, silent, parked, empty];
        svc.check_stalls(&mut fans);
        assert!(
            !fans[1].stalled,
            "one sample is not a stall — a fan may be spinning up"
        );

        svc.stall_since[FanHeader::Aux0.index()] =
            Some(now_ms().saturating_sub(STALL_ALARM_MS + 1));
        svc.check_stalls(&mut fans);

        assert!(!fans[0].stalled, "a fan reporting rpm is fine");
        assert!(fans[1].stalled, "silent while driven is a stall");
        assert!(!fans[2].stalled, "0% duty is not a stall");
        assert!(!fans[3].stalled, "an empty socket is not a stall");

        fans[1].rpm = 3;
        svc.check_stalls(&mut fans);
        assert!(fans[1].stalled, "tach noise is not rotation");

        fans[1].rpm = 700;
        svc.check_stalls(&mut fans);
        assert!(
            !fans[1].stalled,
            "a fan that starts again is no longer stalled"
        );
    }

    #[test]
    fn snapshot_has_a_card_per_header_and_channel() {
        let (mut svc, dir) = service("shape");
        let snap = svc.snapshot();
        assert_eq!(snap.fans.len(), FanHeader::all().len());
        assert_eq!(snap.channels.len(), Channel::all().len());
        assert!(snap.temperature_c.is_some());
        assert!(snap.fan_device.simulated);
        assert!(snap.capabilities.fan_control);
        assert!(snap.avg_percent > 0);
        assert!(!snap.board.display_name().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn manual_duty_updates_the_snapshot() {
        let (mut svc, dir) = service("duty");
        let outcome = svc.set_fan_percent(FanHeader::Cpu, 70).unwrap();
        let cpu = outcome
            .snapshot
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Cpu)
            .unwrap();
        assert_eq!(cpu.percent, 70);
        assert_eq!(cpu.mode, FanMode::Manual);
        assert!(outcome.message.contains("CPU_FAN"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_bound_pump_channel_is_floored() {
        let (mut svc, dir) = service("pump-bound");
        svc.settings.pump_header = Some(FanHeader::Aux3);

        let outcome = svc.set_fan_percent(FanHeader::Aux3, 5).unwrap();
        let pump = outcome
            .snapshot
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Aux3)
            .unwrap();
        assert_eq!(pump.percent, 60, "a bound pump must not fall below 60%");
        assert!(pump.pump);

        let outcome = svc.set_fan_percent(FanHeader::Cpu, 5).unwrap();
        let cpu = outcome
            .snapshot
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Cpu)
            .unwrap();
        assert_eq!(cpu.percent, 5, "an ordinary channel keeps its duty");
        assert!(!cpu.pump);

        let low = [
            curve::CurvePoint::new(30, 20),
            curve::CurvePoint::new(40, 30),
            curve::CurvePoint::new(50, 40),
            curve::CurvePoint::new(60, 50),
        ];
        assert!(
            svc.set_fan_curve(FanHeader::Aux3, &low).is_err(),
            "a pump curve below 60% must be refused"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_channel_claims_a_pump_floor_until_one_is_bound() {
        // The board's channels are SYS, CPU and AUX0-AUX3; none of them is a
        // pump under the vendor's naming, so a 5% request must survive intact.
        let (mut svc, dir) = service("pump");
        let outcome = svc.set_fan_percent(FanHeader::Aux3, 5).unwrap();
        let fan = outcome
            .snapshot
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Aux3)
            .unwrap();
        assert_eq!(fan.percent, 5, "an unbound channel must not be floored");

        // The floor itself still exists for a table that does bind one.
        let settings = crate::settings::Settings::default();
        assert_eq!(settings.clamp_duty(true, 5), 60);
        assert_eq!(settings.clamp_duty(false, 5), 5);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn curves_are_validated_before_touching_hardware() {
        let (mut svc, dir) = service("curve");
        let bad = [
            curve::CurvePoint::new(60, 20),
            curve::CurvePoint::new(50, 40),
            curve::CurvePoint::new(70, 60),
            curve::CurvePoint::new(80, 100),
        ];
        assert!(svc.set_fan_curve(FanHeader::Cpu, &bad).is_err());

        let good = FanProfile::Quiet.points_for(false);
        let outcome = svc.set_fan_curve(FanHeader::Cpu, &good).unwrap();
        let cpu = outcome
            .snapshot
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Cpu)
            .unwrap();
        assert_eq!(cpu.mode, FanMode::SmartFan);
        assert_eq!(cpu.curve, Some(good));
        assert!(cpu.curve_percent.is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn profiles_reach_every_selected_header() {
        let (mut svc, dir) = service("profile");
        let outcome = svc
            .apply_profile(FanHeader::all(), FanProfile::Full)
            .unwrap();
        for fan in &outcome.snapshot.fans {
            assert_eq!(fan.mode, FanMode::SmartFan);
            assert_eq!(fan.curve.unwrap()[0].percent, 100);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lighting_mutations_persist_to_disk() {
        let (mut svc, dir) = service("led");
        svc.set_channel(Channel::Argb2, ChannelConfig::rgb([10, 20, 30], 80), true)
            .unwrap();

        let state_file = svc.state_path().expect("state path");
        assert!(state_file.exists(), "state file written");
        let bytes = std::fs::read(&state_file).unwrap();
        let state = RgbState::from_bytes(&bytes).unwrap();
        assert_eq!(state.get(Channel::Argb2).color, [10, 20, 30]);
        assert_eq!(state.get(Channel::Argb2).brightness, 80);

        svc.turn_off_channel(Channel::Argb2).unwrap();
        let bytes = std::fs::read(&state_file).unwrap();
        let state = RgbState::from_bytes(&bytes).unwrap();
        assert!(!state.get(Channel::Argb2).enabled);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn global_brightness_applies_to_lit_channels_only() {
        let (mut svc, dir) = service("brightness");
        svc.set_all_channels([255, 255, 255], 100, true).unwrap();
        let outcome = svc.set_global_brightness(30).unwrap();
        assert!(outcome.snapshot.channels.iter().all(|c| c.brightness == 30));
        assert!(outcome.snapshot.channels.iter().all(|c| c.enabled));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn activity_feed_records_writes() {
        let (mut svc, dir) = service("activity");
        svc.set_fan_percent(FanHeader::Cpu, 50).unwrap();
        svc.set_all_channels([1, 2, 3], 50, true).unwrap();
        let feed = svc.activity();
        assert!(feed.iter().any(|a| a.kind == "fan"));
        assert!(feed.iter().any(|a| a.kind == "led"));
        assert!(feed.len() <= 40);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_wake_re_applies_only_what_this_session_programmed() {
        let (mut svc, dir) = service("wake");

        let untouched = svc.reassert_after_wake();
        assert_eq!(
            untouched.fans, 0,
            "a session that drove nothing has nothing to restore"
        );
        assert!(
            !untouched.lighting,
            "a session that never lit anything leaves the controller alone"
        );
        assert!(untouched.errors.is_empty());

        svc.set_fan_percent(FanHeader::Cpu, 64).unwrap();
        svc.set_all_channels([0x11, 0x22, 0x33], 70, true).unwrap();

        let report = svc.reassert_after_wake();
        assert_eq!(report.fans, 1, "the channel this session set is rewritten");
        assert!(report.lighting, "the applied frame is pushed again");
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.restored_something());

        // The shell owns the feed; the wake itself records nothing, so a retry
        // cannot leave six identical entries behind.
        let before = svc.activity().len();
        svc.reassert_after_wake();
        assert_eq!(svc.activity().len(), before);

        svc.note("wake", "Re-applied after wake: lighting");
        assert_eq!(svc.activity().last().unwrap().kind, "wake");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_controller_that_went_away_is_the_only_recovery_trigger() {
        assert!(
            !should_recover(false, true, false),
            "a backend without LED control has nothing to restore"
        );
        assert!(
            !should_recover(true, false, false),
            "a session that never lit anything has no frame to restore"
        );
        assert!(
            !should_recover(true, true, true),
            "a controller that still answers needs nothing"
        );
        assert!(
            should_recover(true, true, false),
            "a dead handle on a session that lit something is the case worth a write"
        );
    }

    #[test]
    fn a_tick_leaves_a_present_controller_alone() {
        let (mut svc, dir) = service("recover");
        svc.set_all_channels([9, 9, 9], 40, true).unwrap();

        assert!(
            svc.recover_if_needed().is_none(),
            "the simulator's controller never vanishes"
        );
        let before = svc.activity().len();
        svc.tick();
        assert_eq!(
            svc.activity().len(),
            before,
            "a tick with nothing to recover records nothing"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn settings_change_rebuilds_backends() {
        let (mut svc, dir) = service("settings");
        let same = svc.settings().clone();
        assert!(!svc.update_settings(same));

        let mut changed = svc.settings().clone();
        changed.backend = BackendMode::Simulated;
        changed.poll_interval_ms = 500;
        svc.update_settings(changed.clone());
        assert_eq!(svc.settings().poll_interval_ms, 500);

        let mut hardware = changed.clone();
        hardware.backend = BackendMode::Hardware;
        assert!(svc.update_settings(hardware));
        assert_eq!(svc.settings().backend, BackendMode::Hardware);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn smoothing_reduces_jitter() {
        let (mut svc, dir) = service("smooth");
        let mut s = svc.settings().clone();
        s.smoothing = true;
        svc.update_settings(s);

        let first = svc.snapshot().temperature_c.unwrap();
        let second = svc.snapshot().temperature_c.unwrap();
        assert!((first - second).abs() < 2.0, "EMA should not jump");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- GPU LED ----

    #[test]
    fn a_simulated_snapshot_exposes_the_gpu_section() {
        let (mut svc, dir) = service("gpu-snapshot");
        let snap = svc.snapshot();
        let gpu = snap.gpu_led.expect("GPU section present");
        assert!(gpu.enabled);
        assert!(gpu.device.simulated);
        assert_eq!(gpu.devices.len(), 1);
        assert_eq!(gpu.devices[0].zones.len(), crate::gpu::GPU_ZONES.len());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_gpu_zone_write_is_reflected_and_persisted() {
        let (mut svc, dir) = service("gpu-write");
        let mut state = GpuZoneState {
            color: [255, 0, 0],
            ..Default::default()
        };
        state.mode = 0x02;
        svc.set_gpu_zone("sim0".into(), "fan".into(), state, true)
            .unwrap();

        let zone = svc.snapshot().gpu_led.unwrap().devices[0]
            .zones
            .iter()
            .find(|z| z.id == "fan")
            .unwrap()
            .clone();
        assert_eq!(zone.color, [255, 0, 0]);
        assert_eq!(zone.mode, 2);
        assert!(dir.join("gpu-led.state").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn syncing_from_the_board_copies_a_lit_colour() {
        let (mut svc, dir) = service("gpu-sync");
        svc.set_channel(Channel::Led, ChannelConfig::rgb([12, 34, 56], 80), true)
            .unwrap();
        svc.sync_gpu_from_board().unwrap();
        let gpu = svc.snapshot().gpu_led.unwrap();
        for zone in gpu.devices[0].zones.iter() {
            assert_eq!(zone.color, [12, 34, 56], "zone {}", zone.id);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn disabling_gpu_led_refuses_writes_and_keeps_state() {
        let (mut svc, dir) = service("gpu-toggle");
        svc.set_gpu_zone("sim0".into(), "fan".into(), GpuZoneState::default(), true)
            .unwrap();
        let mut settings = svc.settings().clone();
        settings.gpu_led_enabled = false;
        svc.update_settings(settings);
        assert!(!svc.snapshot().gpu_led.unwrap().enabled);
        assert!(
            svc.set_gpu_zone("sim0".into(), "fan".into(), GpuZoneState::default(), true)
                .is_err()
        );

        let mut settings = svc.settings().clone();
        settings.gpu_led_enabled = true;
        svc.update_settings(settings);
        let gpu = svc.snapshot().gpu_led.unwrap();
        assert!(gpu.enabled);
        assert!(gpu.devices[0].zones.iter().any(|z| z.id == "fan"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unknown_gpu_device_is_rejected() {
        let (mut svc, dir) = service("gpu-unknown");
        assert!(
            svc.set_gpu_zone("nope".into(), "fan".into(), GpuZoneState::default(), true)
                .is_err()
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
