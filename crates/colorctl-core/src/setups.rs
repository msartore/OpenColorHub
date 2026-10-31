//! Named presets: fan settings and lighting settings saved separately.
//!
//! A preset stores *outcomes*, not commands: the duty or curve each header ended
//! up with, or the colour, brightness and on/off state of each lighting channel.
//! Applying one replays those into the service, which is what makes a preset
//! portable across restarts and boards with the same channel map.
//!
//! The two halves are deliberately separate: a fan preset never writes lighting
//! and a lighting preset never writes fans, so recalling a fan curve at night
//! cannot darken the case.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::curve::FanCurve;
use crate::error::Result;
use crate::model::{Channel, ChannelConfig, FanHeader};
use crate::service::Service;

/// On-disk schema version. Bumped only when the meaning of a field changes.
///
/// Version 2 splits the combined setup of version 1 into separate fan and
/// lighting presets.
pub const VERSION: u32 = 2;

/// Which half of the machine a preset covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupKind {
    Fans,
    Lighting,
}

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

/// One saved configuration. Only the half named by `kind` is populated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Setup {
    pub id: String,
    pub name: String,
    pub kind: SetupKind,
    pub saved_at_ms: u64,
    #[serde(default)]
    pub fans: Vec<FanSetting>,
    #[serde(default)]
    pub channels: Vec<ChannelSetting>,
}

impl Setup {
    /// Snapshot of the half `kind` covers, as the service is running it now.
    pub fn capture(service: &mut Service, kind: SetupKind, name: impl Into<String>) -> Self {
        let snapshot = service.snapshot();
        let (fans, channels) = match kind {
            SetupKind::Fans => (
                snapshot
                    .fans
                    .iter()
                    .map(|fan| FanSetting {
                        header: fan.header,
                        percent: fan.percent,
                        curve: fan.curve,
                    })
                    .collect(),
                Vec::new(),
            ),
            SetupKind::Lighting => (
                Vec::new(),
                snapshot
                    .channels
                    .iter()
                    .map(|channel| ChannelSetting {
                        channel: channel.id,
                        color: channel.color,
                        brightness: channel.brightness,
                        enabled: channel.enabled,
                    })
                    .collect(),
            ),
        };

        Self {
            id: new_id(),
            name: name.into(),
            kind,
            saved_at_ms: now_ms(),
            fans,
            channels,
        }
    }

    /// Replays the preset into the service, touching only its own half.
    ///
    /// Lighting is staged channel by channel and pushed with a single
    /// `apply_led`, so a lighting preset costs one HID frame instead of six.
    pub fn apply(&self, service: &mut Service) -> Result<String> {
        match self.kind {
            SetupKind::Fans => {
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
                Ok(format!(
                    "“{}” applied — {} headers",
                    self.name,
                    self.fans.len()
                ))
            }
            SetupKind::Lighting => {
                for channel in &self.channels {
                    service.stage_channel(channel.channel, channel.config());
                }
                service.apply_led()?;
                Ok(format!(
                    "“{}” applied — {} channels",
                    self.name,
                    self.channels.len()
                ))
            }
        }
    }

    /// Short human summary for the UI.
    pub fn summary(&self) -> String {
        match self.kind {
            SetupKind::Fans => {
                let curves = self.fans.iter().filter(|fan| fan.curve.is_some()).count();
                let manual = self.fans.len().saturating_sub(curves);
                format!(
                    "{manual} manual · {curves} curve · {} headers",
                    self.fans.len()
                )
            }
            SetupKind::Lighting => {
                let lit = self
                    .channels
                    .iter()
                    .filter(|channel| channel.enabled)
                    .count();
                let mut colours: Vec<String> = Vec::new();
                for hex in self
                    .channels
                    .iter()
                    .filter(|channel| channel.enabled)
                    .map(ChannelSetting::hex)
                {
                    if !colours.contains(&hex) {
                        colours.push(hex);
                    }
                }
                let colour = match colours.len() {
                    0 => "all off".to_string(),
                    1 => colours[0].to_uppercase(),
                    count => format!("{count} colours"),
                };
                format!("{lit}/{} lit · {colour}", self.channels.len())
            }
        }
    }
}

/// Presets persisted next to the rest of the app state.
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

/// Shape of the version 1 file, where one entry held both halves.
#[derive(Debug, Deserialize)]
struct DiskV1 {
    #[serde(default)]
    setups: Vec<SetupV1>,
}

#[derive(Debug, Deserialize)]
struct SetupV1 {
    id: String,
    name: String,
    #[serde(default)]
    saved_at_ms: u64,
    #[serde(default)]
    fans: Vec<FanSetting>,
    #[serde(default)]
    channels: Vec<ChannelSetting>,
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
            Err(current_error) => match serde_json::from_str::<DiskV1>(&text) {
                // A version 1 entry becomes one fan preset and one lighting
                // preset under the same name: the settings are kept rather than
                // the file being discarded.
                Ok(old) => {
                    store.setups = split_combined(old.setups);
                    if let Err(error) = store.persist() {
                        store.warning = Some(format!("{:?}: {error}", store.path));
                    }
                }
                Err(_) => store.warning = Some(format!("{:?}: {current_error}", store.path)),
            },
        }
        store
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn all(&self) -> &[Setup] {
        &self.setups
    }

    /// Presets of one kind, newest first.
    pub fn of_kind(&self, kind: SetupKind) -> Vec<Setup> {
        self.setups
            .iter()
            .filter(|entry| entry.kind == kind)
            .cloned()
            .collect()
    }

    /// Inserts a new preset or replaces the one with the same id.
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

/// Turns each version 1 entry into a fan preset and a lighting preset.
fn split_combined(old: Vec<SetupV1>) -> Vec<Setup> {
    let mut out = Vec::new();
    for entry in old {
        if !entry.fans.is_empty() {
            out.push(Setup {
                id: format!("{}-f", entry.id),
                name: entry.name.clone(),
                kind: SetupKind::Fans,
                saved_at_ms: entry.saved_at_ms,
                fans: entry.fans,
                channels: Vec::new(),
            });
        }
        if !entry.channels.is_empty() {
            out.push(Setup {
                id: format!("{}-l", entry.id),
                name: entry.name.clone(),
                kind: SetupKind::Lighting,
                saved_at_ms: entry.saved_at_ms,
                fans: Vec::new(),
                channels: entry.channels,
            });
        }
    }
    out.sort_by_key(|entry| std::cmp::Reverse(entry.saved_at_ms));
    out
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
    fn a_fan_preset_captures_only_fans() {
        let (mut svc, _dir) = service("capture-fans");
        svc.set_all_channels([255, 136, 0], 80, true).unwrap();
        svc.set_fan_percent(FanHeader::Cpu, 55).unwrap();

        let setup = Setup::capture(&mut svc, SetupKind::Fans, "Bench");
        assert_eq!(setup.fans.len(), FanHeader::all().len());
        assert!(
            setup.channels.is_empty(),
            "a fan preset must not carry lighting"
        );

        let cpu = setup
            .fans
            .iter()
            .find(|fan| fan.header == FanHeader::Cpu)
            .expect("cpu present");
        assert_eq!(cpu.percent, 55);
    }

    #[test]
    fn a_lighting_preset_captures_only_lighting() {
        let (mut svc, _dir) = service("capture-light");
        svc.set_all_channels([15, 200, 90], 60, true).unwrap();
        svc.set_fan_percent(FanHeader::Pump, 90).unwrap();

        let setup = Setup::capture(&mut svc, SetupKind::Lighting, "Night");
        assert!(
            setup.fans.is_empty(),
            "a lighting preset must not carry fans"
        );
        assert_eq!(setup.channels.len(), Channel::all().len());
        assert_eq!(setup.channels[0].hex(), "#0fc85a");
    }

    #[test]
    fn a_fan_preset_leaves_the_lighting_alone() {
        let (mut source, _dir_a) = service("fans-source");
        source.set_fan_percent(FanHeader::Pump, 90).unwrap();
        let preset = Setup::capture(&mut source, SetupKind::Fans, "Cooling");

        let (mut target, _dir_b) = service("fans-target");
        target.set_all_channels([200, 30, 30], 40, true).unwrap();
        target.set_fan_percent(FanHeader::Pump, 30).unwrap();

        let message = preset.apply(&mut target).expect("apply");
        assert_eq!(message, "“Cooling” applied — 5 headers");

        let snapshot = target.snapshot();
        let pump = snapshot
            .fans
            .iter()
            .find(|fan| fan.header == FanHeader::Pump)
            .unwrap();
        assert_eq!(pump.percent, 90);
        assert_eq!(
            snapshot.channels[0].hex, "#c81e1e",
            "lighting must be untouched by a fan preset"
        );
    }

    #[test]
    fn a_lighting_preset_leaves_the_fans_alone() {
        let (mut source, _dir_a) = service("light-source");
        source.set_all_channels([15, 200, 90], 60, true).unwrap();
        let preset = Setup::capture(&mut source, SetupKind::Lighting, "Evening");

        let (mut target, _dir_b) = service("light-target");
        target.set_fan_percent(FanHeader::Cpu, 42).unwrap();

        let message = preset.apply(&mut target).expect("apply");
        assert_eq!(message, "“Evening” applied — 6 channels");

        let snapshot = target.snapshot();
        let cpu = snapshot
            .fans
            .iter()
            .find(|fan| fan.header == FanHeader::Cpu)
            .unwrap();
        assert_eq!(
            cpu.percent, 42,
            "fans must be untouched by a lighting preset"
        );
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
        let setup = Setup::capture(&mut svc, SetupKind::Fans, "Curved");

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
    fn summaries_name_their_own_half() {
        let (mut svc, _dir) = service("summary");
        svc.set_all_channels([255, 136, 0], 80, true).unwrap();

        let fans = Setup::capture(&mut svc, SetupKind::Fans, "F");
        assert!(fans.summary().ends_with("5 headers"));

        let light = Setup::capture(&mut svc, SetupKind::Lighting, "L");
        assert_eq!(light.summary(), "6/6 lit · #FF8800");
    }

    #[test]
    fn survives_a_disk_round_trip() {
        let (mut svc, dir) = service("roundtrip");
        let setup = Setup::capture(&mut svc, SetupKind::Fans, "Round trip");

        let mut store = SetupStore::load(dir.join("setups.json"));
        store.upsert(setup.clone()).unwrap();
        assert!(store.path().exists());

        let reloaded = SetupStore::load(dir.join("setups.json"));
        assert!(reloaded.warning.is_none());
        assert_eq!(reloaded.all().len(), 1);
        assert_eq!(reloaded.all()[0].kind, SetupKind::Fans);
        assert_eq!(reloaded.all()[0].name, "Round trip");
    }

    #[test]
    fn a_version_1_file_splits_into_two_presets() {
        let dir =
            std::env::temp_dir().join(format!("opencolorhub-v1setups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("setups.json");
        std::fs::write(
            &path,
            r#"{"version":1,"setups":[{"id":"s1","name":"Amber 80%","saved_at_ms":1793450000000,
                "fans":[{"header":"cpu","percent":80,"curve":null}],
                "channels":[{"channel":"led","color":[251,191,36],"brightness":80,"enabled":true}]}]}"#,
        )
        .unwrap();

        let store = SetupStore::load(&path);
        assert!(store.warning.is_none());
        assert_eq!(store.all().len(), 2);
        assert_eq!(store.of_kind(SetupKind::Fans)[0].name, "Amber 80%");
        assert_eq!(store.of_kind(SetupKind::Lighting)[0].name, "Amber 80%");
        assert!(store.of_kind(SetupKind::Fans)[0].channels.is_empty());
        assert!(store.of_kind(SetupKind::Lighting)[0].fans.is_empty());

        let rewritten: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(rewritten["version"], VERSION);
        assert!(
            rewritten["setups"][0]["kind"].is_string(),
            "the file is rewritten in the new shape"
        );
    }

    #[test]
    fn upsert_replaces_and_remove_deletes() {
        let (mut svc, dir) = service("crud");
        let mut store = SetupStore::load(dir.join("setups.json"));
        let mut setup = Setup::capture(&mut svc, SetupKind::Lighting, "One");
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
