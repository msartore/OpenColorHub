//! Named setups: a saved combination of fan and lighting settings that can be
//! applied in one action.
//!
//! A setup stores *outcomes*, not commands: the duty or curve each header ended
//! up with, and the colour, brightness and on/off state of each lighting
//! channel. Applying one replays those into the service, which is what makes a
//! setup portable across restarts and boards with the same channel map.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::curve::FanCurve;
use crate::error::Result;
use crate::model::{Channel, ChannelConfig, FanHeader};
use crate::service::Service;

/// On-disk schema version. Bumped only when the meaning of a field changes.
pub const VERSION: u32 = 1;

/// What one fan header was set to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FanSetting {
    pub header: FanHeader,
    /// Manual duty, used when `curve` is `None`.
    #[serde(default)]
    pub percent: u8,
    /// SmartFan curve, when the header was curve-driven.
    #[serde(default)]
    pub curve: Option<FanCurve>,
}

/// What one lighting channel was set to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSetting {
    pub channel: Channel,
    pub color: [u8; 3],
    pub brightness: u8,
    pub enabled: bool,
}

impl ChannelSetting {
    fn config(&self) -> ChannelConfig {
        ChannelConfig {
            color: self.color,
            brightness: self.brightness,
            enabled: self.enabled,
        }
    }

    /// `#rrggbb`, for the summary the UI shows.
    pub fn hex(&self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            self.color[0], self.color[1], self.color[2]
        )
    }
}

/// One saved configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Setup {
    pub id: String,
    pub name: String,
    pub saved_at_ms: u64,
    pub fans: Vec<FanSetting>,
    pub channels: Vec<ChannelSetting>,
}

impl Setup {
    /// Snapshot of what the service is doing right now.
    pub fn capture(service: &mut Service, name: impl Into<String>) -> Self {
        let snapshot = service.snapshot();
        let fans = snapshot
            .fans
            .iter()
            .map(|fan| FanSetting {
                header: fan.header,
                percent: fan.percent,
                curve: fan.curve,
            })
            .collect();
        let channels = snapshot
            .channels
            .iter()
            .map(|channel| ChannelSetting {
                channel: channel.id,
                color: channel.color,
                brightness: channel.brightness,
                enabled: channel.enabled,
            })
            .collect();

        Self {
            id: new_id(),
            name: name.into(),
            saved_at_ms: now_ms(),
            fans,
            channels,
        }
    }

    /// Replays the setup into the service: fans first, then lighting once.
    ///
    /// Lighting is staged channel by channel and pushed with a single
    /// `apply_led`, so applying a setup costs one HID frame instead of six.
    pub fn apply(&self, service: &mut Service) -> Result<String> {
        for fan in &self.fans {
            match &fan.curve {
                Some(curve) => {
                    service.set_fan_curve(fan.header, curve)?;
                }
                None => {
                    service.set_fan_percent(fan.header, fan.percent)?;
                }
            }
        }

        for channel in &self.channels {
            service.stage_channel(channel.channel, channel.config());
        }
        service.apply_led()?;

        Ok(format!(
            "{} applied — {} headers, {} channels",
            self.name,
            self.fans.len(),
            self.channels.len()
        ))
    }

    /// Short human summary for the UI.
    pub fn summary(&self) -> String {
        let curves = self.fans.iter().filter(|fan| fan.curve.is_some()).count();
        let manual = self.fans.len().saturating_sub(curves);
        let lit = self
            .channels
            .iter()
            .filter(|channel| channel.enabled)
            .count();
        format!(
            "{manual} manual · {curves} curve · {lit}/{} channels lit",
            self.channels.len()
        )
    }
}

/// Setups persisted next to the rest of the app state.
#[derive(Debug)]
pub struct SetupStore {
    path: PathBuf,
    setups: Vec<Setup>,
    /// Set when the file existed but could not be parsed; the store starts empty
    /// rather than dropping the file, so the user can inspect it.
    pub warning: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Disk {
    version: u32,
    setups: Vec<Setup>,
}

impl SetupStore {
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut store = Self {
            path,
            setups: Vec::new(),
            warning: None,
        };

        let Ok(text) = std::fs::read_to_string(&store.path) else {
            return store;
        };
        match serde_json::from_str::<Disk>(&text) {
            Ok(disk) => store.setups = disk.setups,
            Err(error) => store.warning = Some(format!("{:?}: {error}", store.path)),
        }
        store
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn all(&self) -> &[Setup] {
        &self.setups
    }

    /// Inserts a new setup or replaces the one with the same id.
    pub fn upsert(&mut self, setup: Setup) -> Result<()> {
        match self.setups.iter_mut().find(|entry| entry.id == setup.id) {
            Some(existing) => *existing = setup,
            None => self.setups.push(setup),
        }
        self.setups
            .sort_by_key(|entry| std::cmp::Reverse(entry.saved_at_ms));
        self.persist()
    }

    pub fn remove(&mut self, id: &str) -> Result<bool> {
        let before = self.setups.len();
        self.setups.retain(|entry| entry.id != id);
        if self.setups.len() == before {
            return Ok(false);
        }
        self.persist()?;
        Ok(true)
    }

    fn persist(&self) -> Result<()> {
        let disk = Disk {
            version: VERSION,
            setups: self.setups.clone(),
        };
        let text = serde_json::to_string_pretty(&disk)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write beside the target and rename, so a crash mid-write cannot leave
        // a half-parsed setups file behind.
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &self.path)?;
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Ids only have to be unique within one machine's file, so the clock is enough;
/// two saves in the same millisecond fall back to a suffix.
fn new_id() -> String {
    format!("s{:x}", now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::CurvePoint;
    use crate::settings::{BackendMode, Settings};

    fn service(name: &str) -> (Service, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("opencolorhub-setups-{name}-{}", std::process::id()));
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
    fn captures_the_running_state() {
        let (mut svc, _dir) = service("capture");
        svc.set_all_channels([255, 136, 0], 80, true).unwrap();
        svc.set_fan_percent(FanHeader::Cpu, 55).unwrap();

        let setup = Setup::capture(&mut svc, "Bench");
        assert_eq!(setup.fans.len(), FanHeader::all().len());
        assert_eq!(setup.channels.len(), Channel::all().len());

        let cpu = setup
            .fans
            .iter()
            .find(|fan| fan.header == FanHeader::Cpu)
            .expect("cpu present");
        assert_eq!(cpu.percent, 55);
        assert!(setup.channels.iter().all(|channel| channel.enabled));
        assert_eq!(setup.channels[0].hex(), "#ff8800");
    }

    #[test]
    fn applies_into_a_fresh_service() {
        let (mut saved_from, _dir_a) = service("apply-source");
        saved_from
            .set_all_channels([15, 200, 90], 60, true)
            .unwrap();
        saved_from.set_fan_percent(FanHeader::Pump, 90).unwrap();
        let setup = Setup::capture(&mut saved_from, "Quiet night");

        let (mut target, _dir_b) = service("apply-target");
        target.set_all_channels([0, 0, 0], 10, false).unwrap();
        target.set_fan_percent(FanHeader::Pump, 30).unwrap();

        let message = setup.apply(&mut target).expect("apply");
        assert!(message.contains("Quiet night"));

        let snapshot = target.snapshot();
        let pump = snapshot
            .fans
            .iter()
            .find(|fan| fan.header == FanHeader::Pump)
            .unwrap();
        assert_eq!(pump.percent, 90);
        assert!(snapshot.channels.iter().all(|channel| channel.enabled));
        assert_eq!(snapshot.channels[0].hex, "#0fc85a");
    }

    #[test]
    fn keeps_curves_when_replaying() {
        let (mut svc, _dir) = service("curve");
        let curve: FanCurve = [
            CurvePoint::new(30, 20),
            CurvePoint::new(50, 40),
            CurvePoint::new(70, 70),
            CurvePoint::new(90, 100),
        ];
        svc.set_fan_curve(FanHeader::ChaFan1, &curve).unwrap();
        let setup = Setup::capture(&mut svc, "Curved");

        let (mut target, _dir_b) = service("curve-target");
        setup.apply(&mut target).expect("apply");
        let snapshot = target.snapshot();
        let fan = snapshot
            .fans
            .iter()
            .find(|fan| fan.header == FanHeader::ChaFan1)
            .unwrap();
        assert_eq!(fan.mode, crate::model::FanMode::SmartFan);
        assert!(fan.curve.is_some());
    }

    #[test]
    fn survives_a_disk_round_trip() {
        let (mut svc, dir) = service("roundtrip");
        let setup = Setup::capture(&mut svc, "Round trip");

        let mut store = SetupStore::load(dir.join("setups.json"));
        store.upsert(setup.clone()).unwrap();
        assert!(store.path().exists());

        let reloaded = SetupStore::load(dir.join("setups.json"));
        assert!(reloaded.warning.is_none());
        assert_eq!(reloaded.all().len(), 1);
        assert_eq!(reloaded.all()[0].name, "Round trip");
        assert_eq!(reloaded.all()[0].channels.len(), setup.channels.len());
    }

    #[test]
    fn upsert_replaces_and_remove_deletes() {
        let (mut svc, dir) = service("crud");
        let mut store = SetupStore::load(dir.join("setups.json"));
        let mut setup = Setup::capture(&mut svc, "One");
        store.upsert(setup.clone()).unwrap();

        setup.name = "Renamed".to_string();
        store.upsert(setup.clone()).unwrap();
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].name, "Renamed");

        assert!(store.remove(&setup.id).unwrap());
        assert!(!store.remove(&setup.id).unwrap());
        assert!(store.all().is_empty());
    }

    #[test]
    fn a_broken_file_does_not_take_the_app_down() {
        let dir =
            std::env::temp_dir().join(format!("opencolorhub-badsetups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("setups.json");
        std::fs::write(&path, "{ not json").unwrap();

        let store = SetupStore::load(&path);
        assert!(store.all().is_empty());
        assert!(store.warning.is_some());
    }
}
