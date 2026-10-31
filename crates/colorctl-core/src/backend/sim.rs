//! Simulated backends.
//!
//! These keep the whole application — dashboard, fan curves, lighting, state
//! persistence — exercisable on any platform and in CI. They are deliberately
//! simple: a smooth synthetic temperature curve derived from wall time, RPM
//! that follows the programmed duty, and the real `CCTL` state file on disk so
//! the persistence path is genuinely covered.
//!
//! Nothing here touches `/dev/port` or `/dev/hidraw`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backend::{FanBackend, LedBackend};
use crate::curve::{FanCurve, eval};
use crate::error::Result;
use crate::model::{DeviceInfo, FAN_HEADERS, FanHeader, FanMode, FanSample, Telemetry, now_ms};
use crate::state::RgbState;

/// Per-header simulated configuration, persisted so the UI restarts cleanly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct SimFanConfig {
    pwm: u8,
    mode: FanMode,
    curve: Option<FanCurve>,
}

impl Default for SimFanConfig {
    fn default() -> Self {
        Self {
            pwm: 128,
            mode: FanMode::Manual,
            curve: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct SimFanFile {
    fans: Vec<SimFanConfig>,
    seed: u64,
}

impl Default for SimFanFile {
    fn default() -> Self {
        Self {
            fans: vec![SimFanConfig::default(); FAN_HEADERS.len()],
            seed: 0x5EED_1234,
        }
    }
}

/// Synthetic fan/thermal backend.
pub struct SimFan {
    path: PathBuf,
    file: SimFanFile,
    started_ms: u64,
}

impl SimFan {
    pub fn new(data_dir: &Path) -> Self {
        let path = data_dir.join("sim-fan.json");
        let file = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<SimFanFile>(&raw).ok())
            .map(|mut f| {
                f.fans.resize(FAN_HEADERS.len(), SimFanConfig::default());
                f
            })
            .unwrap_or_default();
        Self {
            path,
            file,
            started_ms: now_ms(),
        }
    }

    fn persist(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(body) = serde_json::to_string_pretty(&self.file) {
            let _ = std::fs::write(&self.path, body);
        }
    }

    /// Synthetic board temperature: a slow sine plus a sawtooth "load" term so
    /// the dashboard shows movement without random jumps.
    fn temperature(&self) -> f32 {
        let elapsed = (now_ms().saturating_sub(self.started_ms)) as f32 / 1000.0;
        let phase = (self.seed_fraction() * std::f32::consts::TAU) * 0.35;
        let drift = (elapsed / 42.0 + phase).sin() * 6.5;
        let load = ((elapsed / 95.0).sin().max(0.0)) * 9.0;
        37.0 + drift + load
    }

    fn seed_fraction(&self) -> f32 {
        (self.file.seed % 1000) as f32 / 1000.0
    }

    fn config(&self, header: FanHeader) -> &SimFanConfig {
        &self.file.fans[header.index()]
    }

    fn config_mut(&mut self, header: FanHeader) -> &mut SimFanConfig {
        &mut self.file.fans[header.index()]
    }

    /// RPM curve per header class, roughly matching real hardware.
    fn rpm_for(&self, header: FanHeader, pwm: u8) -> u16 {
        let span = match header {
            FanHeader::Cpu => 2100.0,
            FanHeader::Pump => 2900.0,
            _ => 1350.0,
        };
        let base = match header {
            FanHeader::Pump => 750.0,
            _ => 210.0,
        };
        let duty = pwm as f32 / 255.0;
        let wobble = 1.0 + ((now_ms() as f32 / 900.0) + header.index() as f32).sin() * 0.004;
        ((base + span * duty * duty.sqrt()) * wobble) as u16
    }

    /// Effective duty for a header at `temp`, honouring SmartFan mode.
    fn effective_pwm(&self, header: FanHeader, temp_c: f32) -> u8 {
        let cfg = self.config(header);
        match (cfg.mode, cfg.curve) {
            (FanMode::SmartFan, Some(curve)) => crate::units::pct(eval(&curve, temp_c)),
            _ => cfg.pwm,
        }
    }
}

impl FanBackend for SimFan {
    fn info(&self) -> DeviceInfo {
        DeviceInfo::simulated(
            "Simulated Super I/O",
            "Synthetic Super I/O — no hardware is touched",
        )
        .with_path(self.path.display().to_string())
    }

    fn read(&mut self) -> Result<Telemetry> {
        let temp = self.temperature();
        let fans = FAN_HEADERS
            .iter()
            .map(|header| {
                let pwm = self.effective_pwm(*header, temp);
                FanSample {
                    header: *header,
                    rpm: self.rpm_for(*header, pwm),
                    pwm,
                    mode: self.config(*header).mode,
                    curve: self.config(*header).curve,
                }
            })
            .collect();
        Ok(Telemetry {
            temperature_c: Some(temp),
            temperature_source: Some("synthetic".to_string()),
            fans,
        })
    }

    fn set_pwm(&mut self, header: FanHeader, pwm: u8) -> Result<()> {
        let cfg = self.config_mut(header);
        cfg.pwm = pwm;
        cfg.mode = FanMode::Manual;
        self.persist();
        Ok(())
    }

    fn set_curve(&mut self, header: FanHeader, curve: &FanCurve) -> Result<()> {
        let cfg = self.config_mut(header);
        cfg.curve = Some(*curve);
        cfg.mode = FanMode::SmartFan;
        self.persist();
        Ok(())
    }
}

/// Simulated lighting backend. Writes a *real* `CCTL` state file, so the state
/// code path is covered even without hardware.
pub struct SimLed {
    state: RgbState,
    state_path: PathBuf,
    applied_at: Option<u64>,
}

impl SimLed {
    pub fn new(state_path: PathBuf) -> Self {
        let state = std::fs::read(&state_path)
            .ok()
            .and_then(|bytes| RgbState::from_bytes(&bytes))
            .unwrap_or_default();
        Self {
            state,
            state_path,
            applied_at: None,
        }
    }

    pub fn applied_at(&self) -> Option<u64> {
        self.applied_at
    }
}

impl LedBackend for SimLed {
    fn info(&self) -> DeviceInfo {
        DeviceInfo::simulated(
            "Simulated RGB controller",
            "Frames are written to the state file only (2F4C:1000 not present)",
        )
        .with_path(self.state_path.display().to_string())
    }

    fn state(&self) -> &RgbState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut RgbState {
        &mut self.state
    }

    fn apply(&mut self) -> Result<()> {
        if let Some(parent) = self.state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.state_path, self.state.to_bytes())?;
        self.applied_at = Some(now_ms());
        Ok(())
    }

    fn state_path(&self) -> Option<PathBuf> {
        Some(self.state_path.clone())
    }
}

impl Default for SimFan {
    fn default() -> Self {
        Self::new(Path::new("."))
    }
}

/// Helper used by tests: the simulator must stay inside plausible bounds.
pub fn simulated_bounds() -> (f32, f32) {
    (20.0, 110.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Channel;
    use crate::settings::Settings;

    fn dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("opencolorhub-sim-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn temperature_stays_in_a_plausible_range() {
        let d = dir("temp");
        let mut fan = SimFan::new(&d);
        let (lo, hi) = simulated_bounds();
        for _ in 0..5 {
            let t = fan.read().unwrap().temperature_c.unwrap();
            assert!(t > lo && t < hi, "temperature {t} out of range");
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn manual_duty_drives_rpm() {
        let d = dir("rpm");
        let mut fan = SimFan::new(&d);
        fan.set_pwm(FanHeader::Cpu, 0).unwrap();
        let idle = fan.read().unwrap().fans[0].rpm;
        fan.set_pwm(FanHeader::Cpu, 255).unwrap();
        let full = fan.read().unwrap().fans[0].rpm;
        assert!(full > idle, "expected {full} > {idle}");
        assert_eq!(fan.read().unwrap().fans[0].mode, FanMode::Manual);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn curves_switch_the_header_to_smartfan() {
        let d = dir("curve");
        let mut fan = SimFan::new(&d);
        let curve = crate::curve::FanProfile::Standard.points_for(true);
        fan.set_curve(FanHeader::Pump, &curve).unwrap();
        let sample = fan.read().unwrap().fans[4].clone();
        assert_eq!(sample.mode, FanMode::SmartFan);
        assert_eq!(sample.curve, Some(curve));
        // Pump profile is 80%..100%, so the duty must never fall to zero.
        assert!(sample.pwm >= crate::units::pct(80));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn config_survives_a_restart() {
        let d = dir("persist");
        {
            let mut fan = SimFan::new(&d);
            fan.set_pwm(FanHeader::ChaFan2, 42).unwrap();
        }
        let mut reopened = SimFan::new(&d);
        let sample = reopened.read().unwrap();
        let cha2 = sample
            .fans
            .iter()
            .find(|f| f.header == FanHeader::ChaFan2)
            .unwrap();
        assert_eq!(cha2.pwm, 42);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn led_apply_writes_a_readable_state_file() {
        let d = dir("led");
        let path = d.join("rgb.state");
        let mut led = SimLed::new(path.clone());
        led.state_mut().set_rgb(Channel::Argb1, [0, 255, 0], 70);
        led.apply().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let state = RgbState::from_bytes(&bytes).unwrap();
        assert_eq!(state.get(Channel::Argb1).color, [0, 255, 0]);
        assert_eq!(state.get(Channel::Argb1).brightness, 70);
        assert!(!state.get(Channel::Argb2).enabled);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn settings_type_is_usable_here() {
        // Guards against accidental platform coupling in the sim module.
        let s = Settings::default();
        assert_eq!(s.clamp_duty(true, 0), 60);
    }
}
