//! The setup currently in force, persisted so a new process can put the machine
//! back the way the user left it.
//!
//! Presets ([`crate::setups`]) are named, manually-applied halves of a setup.
//! This file is different: it is the *live* setup — the duties and curves the
//! backends are actually driving, plus the lighting frame on the controller —
//! rewritten after every successful hardware write. A fresh process reads it
//! before its first paint, so a shutdown, an app restart, an elevation relaunch
//! or an update no longer drops the user's configuration on the floor.
//!
//! Both halves are stored in one document so restore is a single decision. A
//! half that was never touched is stored empty and left alone on restore,
//! exactly like the wake path: an app that never drove the fans must not
//! suddenly write 0% to every header at startup.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::{ChannelConfig, now_ms};
use crate::service::Service;
use crate::setups::{ChannelSetting, FanSetting};

/// On-disk schema version. Bump when the meaning of a field changes.
pub const VERSION: u32 = 1;

/// The last setup that was actually applied to the hardware.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActiveSetup {
    pub version: u32,
    pub saved_at_ms: u64,
    /// Duties/curves the backends are driving. Empty means "never set".
    pub fans: Vec<FanSetting>,
    /// Lighting frame on the controller. Empty means "never set".
    pub channels: Vec<ChannelSetting>,
}

impl ActiveSetup {
    pub fn capture(service: &Service) -> Self {
        let channels = service
            .led_state()
            .statuses()
            .into_iter()
            .map(|status: crate::model::ChannelStatus| ChannelSetting {
                channel: status.id,
                color: status.color,
                brightness: status.brightness,
                enabled: status.enabled,
            })
            .collect();
        Self {
            version: VERSION,
            saved_at_ms: now_ms(),
            fans: service.programmed_fans(),
            channels,
        }
    }

    /// Reads a stored setup. A missing or malformed file is simply "nothing to
    /// restore" — the caller still starts up with a working service.
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let setup: Self = serde_json::from_str(&text).ok()?;
        (setup.version == VERSION).then_some(setup)
    }

    /// Atomic write: a crash mid-save cannot leave a half-parsed setup behind.
    pub fn persist(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    pub fn has_fans(&self) -> bool {
        !self.fans.is_empty()
    }

    pub fn has_lighting(&self) -> bool {
        !self.channels.is_empty()
    }
}

/// Outcome of replaying a stored setup, kept separate from [`WakeReport`](crate::WakeReport)
/// so the UI can say "restored at launch" rather than "woke".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RestoreReport {
    /// A stored setup was found at all.
    pub found: bool,
    pub fans: usize,
    pub lighting: bool,
    pub errors: Vec<String>,
}

impl RestoreReport {
    pub fn restored_something(&self) -> bool {
        self.fans > 0 || self.lighting
    }
}

/// Replays one half at a time so a failure in fans cannot suppress lighting or
/// the other way round. Every successful write re-persists the active setup via
/// the service, so the file stays the authority.
pub fn restore(setup: &ActiveSetup, service: &mut Service) -> RestoreReport {
    let mut report = RestoreReport {
        found: true,
        ..RestoreReport::default()
    };

    for fan in &setup.fans {
        let result = match &fan.curve {
            Some(curve) => service.set_fan_curve(fan.header, curve).map(|_| ()),
            None => service.set_fan_percent(fan.header, fan.percent).map(|_| ()),
        };
        match result {
            Ok(()) => report.fans += 1,
            Err(error) => report
                .errors
                .push(format!("{}: {error}", fan.header.label())),
        }
    }

    if setup.has_lighting() {
        for channel in &setup.channels {
            service.stage_channel(
                channel.channel,
                ChannelConfig {
                    color: channel.color,
                    brightness: channel.brightness,
                    enabled: channel.enabled,
                },
            );
        }
        match service.apply_led() {
            Ok(_) => report.lighting = true,
            Err(error) => report.errors.push(format!("lighting: {error}")),
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::CurvePoint;
    use crate::model::{Channel, FanHeader};
    use crate::settings::{BackendMode, Settings};
    use crate::state::RgbState;

    fn service(name: &str) -> (Service, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("opencolorhub-active-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let settings = Settings {
            backend: BackendMode::Simulated,
            smoothing: false,
            ..Settings::default()
        };
        (Service::new(settings, dir.clone()), dir)
    }

    #[test]
    fn captures_only_what_the_session_drove() {
        let (mut svc, _dir) = service("capture");
        svc.set_fan_percent(FanHeader::Cpu, 55).unwrap();
        svc.set_all_channels([10, 20, 30], 70, true).unwrap();

        let setup = ActiveSetup::capture(&mut svc);
        assert_eq!(setup.fans.len(), 1, "only the header set this session");
        assert_eq!(setup.fans[0].header, FanHeader::Cpu);
        assert_eq!(setup.fans[0].percent, 55);
        assert_eq!(setup.channels.len(), Channel::all().len());
        assert_eq!(setup.channels[0].color, [10, 20, 30]);
    }

    #[test]
    fn persists_and_restores_both_halves() {
        let (mut source, dir) = service("roundtrip");
        let curve = [
            CurvePoint::new(30, 40),
            CurvePoint::new(50, 60),
            CurvePoint::new(70, 80),
            CurvePoint::new(90, 100),
        ];
        source.set_fan_curve(FanHeader::Aux0, &curve).unwrap();
        source.set_fan_percent(FanHeader::Cpu, 42).unwrap();
        source
            .set_channel(Channel::Led, ChannelConfig::rgb([1, 2, 3], 66), true)
            .unwrap();

        let setup = ActiveSetup::capture(&mut source);
        let path = dir.join("active-setup.json");
        setup.persist(&path).unwrap();
        let loaded = ActiveSetup::load(&path).expect("reload");
        assert_eq!(loaded, setup);

        let (mut fresh, _other) = service("fresh");
        let report = restore(&loaded, &mut fresh);
        assert!(report.found);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.fans, 2);
        assert!(report.lighting);

        let snapshot = fresh.snapshot();
        let cpu = snapshot
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Cpu)
            .unwrap();
        assert_eq!(cpu.percent, 42);
        let aux0 = snapshot
            .fans
            .iter()
            .find(|f| f.header == FanHeader::Aux0)
            .unwrap();
        assert!(aux0.curve.is_some());
        assert_eq!(snapshot.channels[0].color, [1, 2, 3]);
        assert_eq!(snapshot.channels[0].brightness, 66);
    }

    #[test]
    fn an_empty_setup_restores_nothing() {
        let (mut svc, _dir) = service("empty");
        let report = restore(&ActiveSetup::default(), &mut svc);
        assert!(!report.restored_something());
    }

    #[test]
    fn a_corrupt_file_is_ignored() {
        let (_svc, dir) = service("corrupt");
        let path = dir.join("active.json");
        std::fs::write(&path, "{ nope").unwrap();
        assert!(ActiveSetup::load(&path).is_none());
        assert!(ActiveSetup::load(&dir.join("missing.json")).is_none());
    }

    #[test]
    fn lighting_state_is_persisted_in_the_cctl_blob_too() {
        let (svc, dir) = service("cctl");
        let state = svc.led_state().clone();
        let bytes = state.to_bytes();
        assert_eq!(RgbState::from_bytes(&bytes), Some(state));
        assert!(dir.exists());
    }
}
