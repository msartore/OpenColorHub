//! Hardware model: fan headers, ARGB channels and the wire types shared with
//! the frontend.
//!
//! Everything that crosses the Tauri IPC boundary is defined here so the Rust
//! and TypeScript sides cannot drift apart silently (`src/types.ts` mirrors
//! these serde representations).

use serde::{Deserialize, Serialize};

use crate::board::BoardInfo;
use crate::curve::FanCurve;
use crate::units::pwm_to_pct;

/// Number of PWM steps the Super I/O accepts.
pub const PWM_MAX: u8 = 255;

/// Nuvoton NCT5584D chip ID. The upper 12 bits identify the family; the low
/// nibble is a revision stepping.
pub const NCT5584D_CHIP_ID: u16 = 0xD42A;

/// USB vendor ID of the onboard Colorful RGB controller.
pub const COLORFUL_VID: u16 = 0x2F4C;

/// USB product ID of the onboard Colorful RGB controller.
pub const COLORFUL_PID: u16 = 0x1000;

/// Usage page the RGB controller advertises (HID report descriptor).
pub const COLORFUL_USAGE_PAGE: u16 = 0xFF01;

/// Fan headers exposed by the NCT5584D firmware on supported Colorful boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanHeader {
    Cpu,
    ChaFan1,
    ChaFan2,
    ChaFan3,
    Pump,
}

pub const FAN_HEADERS: [FanHeader; 5] = [
    FanHeader::Cpu,
    FanHeader::ChaFan1,
    FanHeader::ChaFan2,
    FanHeader::ChaFan3,
    FanHeader::Pump,
];

impl FanHeader {
    /// Stable iteration order — must not change: it defines register layout.
    pub fn all() -> &'static [FanHeader] {
        &FAN_HEADERS
    }

    /// Serialized identifier (matches `FanHeaderId` in the frontend).
    pub fn id(&self) -> &'static str {
        match self {
            FanHeader::Cpu => "cpu",
            FanHeader::ChaFan1 => "cha_fan1",
            FanHeader::ChaFan2 => "cha_fan2",
            FanHeader::ChaFan3 => "cha_fan3",
            FanHeader::Pump => "pump",
        }
    }

    /// Silkscreen label printed on the motherboard.
    pub fn label(&self) -> &'static str {
        match self {
            FanHeader::Cpu => "CPU_FAN",
            FanHeader::ChaFan1 => "CHA_FAN1",
            FanHeader::ChaFan2 => "CHA_FAN2",
            FanHeader::ChaFan3 => "CHA_FAN3",
            FanHeader::Pump => "AIO_PUMP",
        }
    }

    /// Human description shown on the fan screen.
    pub fn description(&self) -> &'static str {
        match self {
            FanHeader::Cpu => "CPU cooler / AIO radiator",
            FanHeader::ChaFan1 => "Chassis intake",
            FanHeader::ChaFan2 => "Chassis exhaust",
            FanHeader::ChaFan3 => "Chassis auxiliary",
            FanHeader::Pump => "AIO pump (keep ≥ 80%)",
        }
    }

    /// HWM bank the header is wired to.
    pub fn bank(&self) -> u8 {
        match self {
            FanHeader::Cpu => 2,
            FanHeader::ChaFan1 => 1,
            FanHeader::ChaFan2 => 10,
            FanHeader::ChaFan3 => 3,
            FanHeader::Pump => 11,
        }
    }

    /// Register holding the 16-bit tachometer value (bank 4).
    pub fn rpm_reg(&self) -> u8 {
        match self {
            FanHeader::ChaFan1 => 0xC0,
            FanHeader::Cpu => 0xC2,
            FanHeader::ChaFan3 => 0xC4,
            FanHeader::ChaFan2 => 0xCA,
            FanHeader::Pump => 0xCE,
        }
    }

    /// Pumps must never be stopped — the UI enforces a floor.
    pub fn is_pump(&self) -> bool {
        matches!(self, FanHeader::Pump)
    }

    /// Index inside [`FAN_HEADERS`]; used by per-header state arrays.
    pub fn index(&self) -> usize {
        match self {
            FanHeader::Cpu => 0,
            FanHeader::ChaFan1 => 1,
            FanHeader::ChaFan2 => 2,
            FanHeader::ChaFan3 => 3,
            FanHeader::Pump => 4,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        FAN_HEADERS
            .into_iter()
            .find(|h| h.id() == id || h.label().eq_ignore_ascii_case(id))
    }
}

impl std::fmt::Display for FanHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Fan control mode as reported by bank register `0x02`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanMode {
    /// Manual duty cycle programmed through register `0x09`.
    Manual,
    /// Firmware SmartFan curve (4 points) is driving the header.
    SmartFan,
    /// Register read-back is unavailable (simulated or access denied).
    Unknown,
}

impl FanMode {
    pub fn label(&self) -> &'static str {
        match self {
            FanMode::Manual => "Manual",
            FanMode::SmartFan => "SmartFan",
            FanMode::Unknown => "Unknown",
        }
    }

    /// Decodes register `0x02`.
    pub fn from_register(value: u8) -> Self {
        match value & 0xC0 {
            0x00 => FanMode::Manual,
            0x40 => FanMode::SmartFan,
            _ => FanMode::Unknown,
        }
    }

    pub fn register_value(&self) -> u8 {
        match self {
            FanMode::Manual => 0x00,
            FanMode::SmartFan => 0x40,
            FanMode::Unknown => 0x00,
        }
    }
}

/// ARGB/RGB channels wired to the onboard HID controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// 18 onboard LEDs near the chipset/IO shroud.
    Led,
    /// 12V 4-pin header 1 (single-colour zones driven as one LED).
    Rgb1,
    /// 12V 4-pin header 2.
    Rgb2,
    /// 5V 3-pin ARGB header 1.
    Argb1,
    /// 5V 3-pin ARGB header 2.
    Argb2,
    /// 5V 3-pin ARGB header 3.
    Argb3,
}

pub const CHANNELS: [Channel; 6] = [
    Channel::Led,
    Channel::Rgb1,
    Channel::Rgb2,
    Channel::Argb1,
    Channel::Argb2,
    Channel::Argb3,
];

/// Total LEDs in the HID frame — fixed by the controller firmware.
pub const TOTAL_LEDS: usize = 200;

impl Channel {
    /// Stable iteration order — defines the `CCTL` state layout and LED offsets.
    pub fn all() -> &'static [Channel] {
        &CHANNELS
    }

    pub fn id(&self) -> &'static str {
        match self {
            Channel::Led => "led",
            Channel::Rgb1 => "rgb1",
            Channel::Rgb2 => "rgb2",
            Channel::Argb1 => "argb1",
            Channel::Argb2 => "argb2",
            Channel::Argb3 => "argb3",
        }
    }

    /// Short label used in compact UI.
    pub fn label(&self) -> &'static str {
        match self {
            Channel::Led => "Onboard",
            Channel::Rgb1 => "12V_1",
            Channel::Rgb2 => "12V_2",
            Channel::Argb1 => "5V_1",
            Channel::Argb2 => "5V_2",
            Channel::Argb3 => "5V_3",
        }
    }

    /// Full upstream name, kept for CLI compatibility.
    pub fn name(&self) -> &'static str {
        match self {
            Channel::Led => "LED (Onboard)",
            Channel::Rgb1 => "12V_1 (12V RGB Header 1)",
            Channel::Rgb2 => "12V_2 (12V RGB Header 2)",
            Channel::Argb1 => "5V_1 (ARGB Header 1)",
            Channel::Argb2 => "5V_2 (ARGB Header 2)",
            Channel::Argb3 => "5V_3 (ARGB Header 3)",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Channel::Led => "Chipset shroud + I/O cover, addressable",
            Channel::Rgb1 | Channel::Rgb2 => "12V 4-pin, single-zone (G-R-B)",
            _ => "5V 3-pin addressable, per-LED",
        }
    }

    /// LED slice owned by this channel inside the 200-LED frame.
    pub fn led_range(&self) -> std::ops::Range<usize> {
        match self {
            Channel::Led => 0..18,
            Channel::Rgb1 => 18..19,
            Channel::Rgb2 => 19..20,
            Channel::Argb1 => 20..80,
            Channel::Argb2 => 80..140,
            Channel::Argb3 => 140..200,
        }
    }

    pub fn led_count(&self) -> usize {
        let r = self.led_range();
        r.end - r.start
    }

    /// Index inside [`CHANNELS`]; also the offset in the `CCTL` state blob.
    pub fn index(&self) -> usize {
        match self {
            Channel::Led => 0,
            Channel::Rgb1 => 1,
            Channel::Rgb2 => 2,
            Channel::Argb1 => 3,
            Channel::Argb2 => 4,
            Channel::Argb3 => 5,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        let normalised = id.trim().to_ascii_lowercase().replace(['-', '_'], "");
        CHANNELS.into_iter().find(|c| {
            let candidate = c.id().replace('_', "");
            candidate == normalised || c.label().replace('_', "").eq_ignore_ascii_case(&normalised)
        })
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Colour + brightness + on/off for one channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub color: [u8; 3],
    pub brightness: u8,
    pub enabled: bool,
}

impl ChannelConfig {
    pub fn rgb(color: [u8; 3], brightness: u8) -> Self {
        Self {
            color,
            brightness: brightness.min(100),
            enabled: true,
        }
    }

    pub fn off() -> Self {
        Self::default()
    }

    /// Colour actually written to the LEDs (brightness applied).
    pub fn scaled(&self) -> [u8; 3] {
        scale_color(self.color, self.brightness)
    }

    pub fn hex(&self) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            self.color[0], self.color[1], self.color[2]
        )
    }
}

/// Scales a colour by a percentage (0-100), matching upstream maths.
pub fn scale_color(color: [u8; 3], brightness: u8) -> [u8; 3] {
    let factor = brightness.min(100) as u16;
    [
        ((color[0] as u16 * factor) / 100) as u8,
        ((color[1] as u16 * factor) / 100) as u8,
        ((color[2] as u16 * factor) / 100) as u8,
    ]
}

/// One fan reading plus its programmed state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FanSample {
    pub header: FanHeader,
    pub rpm: u16,
    pub pwm: u8,
    pub mode: FanMode,
    /// Curve read back from the chip when the header is in SmartFan mode, or
    /// the curve last written through opencolorhub.
    pub curve: Option<FanCurve>,
}

impl FanSample {
    pub fn new(header: FanHeader, rpm: u16, pwm: u8, mode: FanMode) -> Self {
        Self {
            header,
            rpm,
            pwm,
            mode,
            curve: None,
        }
    }

    pub fn percent(&self) -> u8 {
        pwm_to_pct(self.pwm)
    }
}

/// A full telemetry read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    pub temperature_c: Option<f32>,
    pub fans: Vec<FanSample>,
}

impl Telemetry {
    pub fn empty() -> Self {
        Self {
            temperature_c: None,
            fans: Vec::new(),
        }
    }
}

/// Where a backend points at, and whether it can actually reach hardware.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Human name, e.g. "Nuvoton NCT5584D".
    pub name: String,
    /// What it is / how it is reached.
    pub detail: String,
    /// `/dev/port`, `/dev/hidraw3`, a config path, ... when known.
    pub path: Option<String>,
    /// False when the backend is a placeholder for missing hardware.
    pub available: bool,
    /// True when readings are synthetic — always surfaced in the UI.
    pub simulated: bool,
    /// Why the hardware backend is unavailable, when it is.
    pub error: Option<String>,
}

impl DeviceInfo {
    pub fn hardware(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail: detail.into(),
            path: None,
            available: true,
            simulated: false,
            error: None,
        }
    }

    pub fn simulated(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail: detail.into(),
            path: None,
            available: true,
            simulated: true,
            error: None,
        }
    }

    pub fn unavailable(
        name: impl Into<String>,
        detail: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            detail: detail.into(),
            path: None,
            available: false,
            simulated: false,
            error: Some(error.into()),
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// What the current backend can actually do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub fan_control: bool,
    pub led_control: bool,
    /// Short explanations of degraded modes (missing root, no HID device, ...).
    pub notes: Vec<String>,
}

impl Capabilities {
    pub fn full() -> Self {
        Self {
            fan_control: true,
            led_control: true,
            notes: Vec::new(),
        }
    }
}

/// Fan card payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FanStatus {
    pub header: FanHeader,
    pub label: String,
    pub description: String,
    pub bank: u8,
    pub rpm: u16,
    pub pwm: u8,
    pub percent: u8,
    pub mode: FanMode,
    pub curve: Option<FanCurve>,
    /// Duty the curve would ask for at the current temperature.
    pub curve_percent: Option<u8>,
}

/// Channel card payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub id: Channel,
    pub label: String,
    pub name: String,
    pub description: String,
    pub leds: usize,
    pub color: [u8; 3],
    pub hex: String,
    /// Colour as rendered (brightness applied).
    pub render_hex: String,
    pub brightness: u8,
    pub enabled: bool,
}

/// Everything the dashboard needs in one IPC round-trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp_ms: u64,
    pub temperature_c: Option<f32>,
    pub fans: Vec<FanStatus>,
    pub channels: Vec<ChannelStatus>,
    pub fan_device: DeviceInfo,
    pub led_device: DeviceInfo,
    pub capabilities: Capabilities,
    /// Average fan duty, used by the dashboard summary.
    pub avg_percent: u8,
    /// SMBIOS identity of the host board and how well it matches the catalogue.
    pub board: BoardInfo,
}

impl Snapshot {
    pub fn empty() -> Self {
        Self {
            timestamp_ms: 0,
            temperature_c: None,
            fans: Vec::new(),
            channels: Vec::new(),
            fan_device: DeviceInfo::unavailable("No fan backend", "none", "not initialised"),
            led_device: DeviceInfo::unavailable("No LED backend", "none", "not initialised"),
            capabilities: Capabilities {
                fan_control: false,
                led_control: false,
                notes: Vec::new(),
            },
            avg_percent: 0,
            board: BoardInfo::default(),
        }
    }
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_ids_round_trip() {
        for ch in Channel::all() {
            assert_eq!(Channel::from_id(ch.id()), Some(*ch), "id {}", ch.id());
            assert_eq!(
                Channel::from_id(ch.label()),
                Some(*ch),
                "label {}",
                ch.label()
            );
            assert_eq!(Channel::all()[ch.index()], *ch);
        }
        assert_eq!(Channel::from_id("5V_1"), Some(Channel::Argb1));
        assert_eq!(Channel::from_id("5v1"), Some(Channel::Argb1));
        assert_eq!(Channel::from_id("nope"), None);
    }

    #[test]
    fn fan_ids_round_trip() {
        for h in FanHeader::all() {
            assert_eq!(FanHeader::from_id(h.id()), Some(*h));
            assert_eq!(FanHeader::from_id(h.label()), Some(*h));
        }
    }

    #[test]
    fn led_ranges_cover_the_whole_frame_without_overlap() {
        let mut covered = [0u8; TOTAL_LEDS];
        for ch in Channel::all() {
            for i in ch.led_range() {
                covered[i] += 1;
            }
        }
        assert!(
            covered.iter().all(|c| *c == 1),
            "every LED owned exactly once"
        );
        let total: usize = Channel::all().iter().map(|c| c.led_count()).sum();
        assert_eq!(total, TOTAL_LEDS);
    }

    #[test]
    fn brightness_scaling_matches_upstream() {
        assert_eq!(scale_color([255, 0, 0], 100), [255, 0, 0]);
        assert_eq!(scale_color([255, 0, 0], 50), [127, 0, 0]);
        assert_eq!(scale_color([255, 255, 255], 0), [0, 0, 0]);
        assert_eq!(scale_color([200, 100, 50], 150), [200, 100, 50]);
    }

    #[test]
    fn fan_mode_decodes_registers() {
        assert_eq!(FanMode::from_register(0x00), FanMode::Manual);
        assert_eq!(FanMode::from_register(0x40), FanMode::SmartFan);
        assert_eq!(FanMode::from_register(0x80), FanMode::Unknown);
        assert_eq!(FanMode::SmartFan.register_value(), 0x40);
    }

    #[test]
    fn channel_config_hex() {
        let cfg = ChannelConfig::rgb([255, 0, 0x88], 50);
        assert_eq!(cfg.hex(), "#ff0088");
        assert_eq!(cfg.scaled(), [127, 0, 68]);
        assert!(cfg.enabled);
    }
}
