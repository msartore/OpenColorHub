//! User settings, persisted as JSON next to the app config.
//!
//! The GUI edits individual fields, so [`SettingsPatch`] exists to keep a
//! half-filled form from resetting unrelated values. Loading is forgiving: a
//! malformed file is reported but never blocks startup (the app falls back to
//! defaults and keeps a timestamped copy of the broken file).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Which backend to talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackendMode {
    /// Probe real hardware, silently fall back to the simulator with a note.
    #[default]
    Auto,
    /// Real hardware only — fail loudly when it is missing.
    Hardware,
    /// Never touch hardware (UI development, CI, demos).
    Simulated,
}

impl BackendMode {
    pub fn id(&self) -> &'static str {
        match self {
            BackendMode::Auto => "auto",
            BackendMode::Hardware => "hardware",
            BackendMode::Simulated => "simulated",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            BackendMode::Auto => "Auto detect",
            BackendMode::Hardware => "Hardware only",
            BackendMode::Simulated => "Simulated",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            BackendMode::Auto => {
                "Use the Super I/O and HID devices when present, otherwise simulate"
            }
            BackendMode::Hardware => {
                "Require real hardware; surface an error when it is unavailable"
            }
            BackendMode::Simulated => "Never access hardware — safe for demos and development",
        }
    }
}

/// Temperature display unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TempUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TempUnit {
    pub fn symbol(&self) -> &'static str {
        match self {
            TempUnit::Celsius => "°C",
            TempUnit::Fahrenheit => "°F",
        }
    }

    /// Converts a Celsius reading for display.
    pub fn convert(&self, celsius: f32) -> f32 {
        match self {
            TempUnit::Celsius => celsius,
            TempUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
        }
    }
}

/// Polling cadence bounds (ms). Slow enough not to hammer `/dev/port`,
/// fast enough for a live dashboard.
pub const POLL_MIN_MS: u64 = 250;
pub const POLL_MAX_MS: u64 = 10_000;
pub const POLL_DEFAULT_MS: u64 = 1_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Which backend to use.
    pub backend: BackendMode,
    /// Telemetry interval in milliseconds.
    pub poll_interval_ms: u64,
    /// Display unit for temperatures.
    pub temp_unit: TempUnit,
    /// Exponential smoothing of RPM/temperature readings.
    pub smoothing: bool,
    /// Re-apply the stored lighting state when the app starts.
    pub reapply_led_on_launch: bool,
    /// Ask before writing to hardware (fan/LED changes).
    pub confirm_writes: bool,
    /// UI accent colour (`#rrggbb`).
    pub accent: String,
    /// Optional override for the `CCTL` state file.
    pub state_file: Option<PathBuf>,
    /// Remember the last screen so the app reopens where it was.
    pub last_screen: String,
    /// Warn when a fan is configured below this duty (percent).
    pub low_duty_warning: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backend: BackendMode::default(),
            poll_interval_ms: POLL_DEFAULT_MS,
            temp_unit: TempUnit::default(),
            smoothing: true,
            reapply_led_on_launch: false,
            confirm_writes: true,
            accent: "#4ade80".to_string(),
            state_file: None,
            last_screen: "dashboard".to_string(),
            low_duty_warning: 20,
        }
    }
}

impl Settings {
    /// Loads settings, returning defaults when the file is missing.
    ///
    /// A corrupt file yields `Err`, but the caller keeps the defaults it passed
    /// in — the GUI shows a warning banner instead of refusing to start.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str::<Self>(&raw).map_err(|source| Error::Settings {
                path: path.to_path_buf(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::Io(e)),
        }
    }

    /// Loads without failing: returns defaults plus an optional warning.
    pub fn load_or_default(path: &Path) -> (Self, Option<String>) {
        match Self::load(path) {
            Ok(mut s) => {
                s.sanitize();
                (s, None)
            }
            Err(e) => (Self::default(), Some(e.to_string())),
        }
    }

    /// Writes atomically (temp file + rename) so a crash cannot truncate it.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let body = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Clamps every field to a sane range. Called after load and after a patch.
    pub fn sanitize(&mut self) {
        self.poll_interval_ms = self.poll_interval_ms.clamp(POLL_MIN_MS, POLL_MAX_MS);
        self.low_duty_warning = self.low_duty_warning.min(100);
        if !is_valid_hex(&self.accent) {
            self.accent = Self::default().accent;
        }
        if self.last_screen.is_empty() {
            self.last_screen = "dashboard".to_string();
        }
    }

    /// Applies a partial update from the UI.
    pub fn apply_patch(&mut self, patch: SettingsPatch) {
        if let Some(v) = patch.backend {
            self.backend = v;
        }
        if let Some(v) = patch.poll_interval_ms {
            self.poll_interval_ms = v;
        }
        if let Some(v) = patch.temp_unit {
            self.temp_unit = v;
        }
        if let Some(v) = patch.smoothing {
            self.smoothing = v;
        }
        if let Some(v) = patch.reapply_led_on_launch {
            self.reapply_led_on_launch = v;
        }
        if let Some(v) = patch.confirm_writes {
            self.confirm_writes = v;
        }
        if let Some(v) = patch.accent {
            self.accent = v;
        }
        if let Some(v) = patch.state_file {
            self.state_file = v;
        }
        if let Some(v) = patch.last_screen {
            self.last_screen = v;
        }
        if let Some(v) = patch.low_duty_warning {
            self.low_duty_warning = v;
        }
        self.sanitize();
    }

    /// Clamp a requested duty for a header, protecting the pump.
    pub fn clamp_duty(&self, is_pump: bool, percent: u8) -> u8 {
        let floor = if is_pump { 60 } else { 0 };
        percent.clamp(floor, 100)
    }
}

/// Partial settings update coming from the UI.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsPatch {
    pub backend: Option<BackendMode>,
    pub poll_interval_ms: Option<u64>,
    pub temp_unit: Option<TempUnit>,
    pub smoothing: Option<bool>,
    pub reapply_led_on_launch: Option<bool>,
    pub confirm_writes: Option<bool>,
    pub accent: Option<String>,
    pub state_file: Option<Option<PathBuf>>,
    pub last_screen: Option<String>,
    pub low_duty_warning: Option<u8>,
}

/// `#rgb` / `#rrggbb`, case-insensitive.
pub fn is_valid_hex(value: &str) -> bool {
    let v = value.trim();
    let Some(hex) = v.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("opencolorhub-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn defaults_are_valid() {
        let mut s = Settings::default();
        s.sanitize();
        assert_eq!(s, Settings::default());
        assert_eq!(s.poll_interval_ms, POLL_DEFAULT_MS);
        assert!(is_valid_hex(&s.accent));
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = temp_dir("settings");
        let path = dir.join("settings.json");

        let mut s = Settings {
            poll_interval_ms: 2500,
            temp_unit: TempUnit::Fahrenheit,
            backend: BackendMode::Simulated,
            accent: "#22d3ee".into(),
            ..Settings::default()
        };
        s.sanitize();
        s.save(&path).expect("save");
        let loaded = Settings::load(&path).expect("load");
        assert_eq!(loaded, s);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file cleaned up"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let path = std::env::temp_dir()
            .join("opencolorhub-does-not-exist")
            .join("settings.json");
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
    }

    #[test]
    fn corrupt_file_surfaces_a_warning_but_keeps_defaults() {
        let dir = temp_dir("corrupt");
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        let (settings, warning) = Settings::load_or_default(&path);
        assert_eq!(settings, Settings::default());
        assert!(warning.unwrap().contains("malformed"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn patch_only_touches_present_fields() {
        let mut s = Settings::default();
        s.apply_patch(SettingsPatch {
            smoothing: Some(false),
            ..Default::default()
        });
        assert!(!s.smoothing);
        assert_eq!(s.poll_interval_ms, POLL_DEFAULT_MS);
        assert_eq!(s.temp_unit, TempUnit::Celsius);
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut s = Settings {
            poll_interval_ms: 5,
            accent: "green".into(),
            low_duty_warning: 250,
            last_screen: String::new(),
            ..Settings::default()
        };
        s.sanitize();
        assert_eq!(s.poll_interval_ms, POLL_MIN_MS);
        assert_eq!(s.accent, Settings::default().accent);
        assert_eq!(s.low_duty_warning, 100);
        assert_eq!(s.last_screen, "dashboard");

        s.poll_interval_ms = 999_999;
        s.sanitize();
        assert_eq!(s.poll_interval_ms, POLL_MAX_MS);
    }

    #[test]
    fn pump_floor_is_enforced() {
        let s = Settings::default();
        assert_eq!(s.clamp_duty(true, 10), 60);
        assert_eq!(s.clamp_duty(true, 100), 100);
        assert_eq!(s.clamp_duty(false, 10), 10);
    }

    #[test]
    fn temperature_conversion() {
        assert_eq!(TempUnit::Celsius.convert(50.0), 50.0);
        assert_eq!(TempUnit::Fahrenheit.convert(0.0), 32.0);
        assert_eq!(TempUnit::Fahrenheit.convert(100.0), 212.0);
    }
}
