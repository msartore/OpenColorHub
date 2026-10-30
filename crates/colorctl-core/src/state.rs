//! Persisted lighting state.
//!
//! The on-disk layout is byte-for-byte identical to upstream `colorctl` so an
//! existing `/var/lib/colorctl/rgb.state` keeps working:
//!
//! ```text
//! [0..4]   "CCTL" magic
//! [4]      format version (1)
//! [5..35]  6 channels × (R, G, B, brightness, enabled)
//! ```
//!
//! Keeping the format means the CLI, a systemd unit or a NixOS activation
//! script written against upstream can hand state to the GUI and vice versa.

use serde::{Deserialize, Serialize};

use crate::model::{CHANNELS, Channel, ChannelConfig, ChannelStatus, TOTAL_LEDS, scale_color};

/// Serialized lighting state for every channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbState {
    channels: [ChannelConfig; CHANNELS.len()],
}

impl Default for RgbState {
    fn default() -> Self {
        Self {
            channels: [ChannelConfig::default(); CHANNELS.len()],
        }
    }
}

impl RgbState {
    pub const MAGIC: &'static [u8; 4] = b"CCTL";
    pub const VERSION: u8 = 1;
    pub const BYTE_LEN: usize = 5 + CHANNELS.len() * 5;

    pub fn get(&self, channel: Channel) -> ChannelConfig {
        self.channels[channel.index()]
    }

    pub fn set(&mut self, channel: Channel, config: ChannelConfig) {
        self.channels[channel.index()] = ChannelConfig {
            brightness: config.brightness.min(100),
            ..config
        };
    }

    /// Marks a channel as an active solid colour.
    pub fn set_rgb(&mut self, channel: Channel, color: [u8; 3], brightness: u8) {
        self.set(channel, ChannelConfig::rgb(color, brightness));
    }

    pub fn turn_off(&mut self, channel: Channel) {
        self.set(channel, ChannelConfig::off());
    }

    pub fn turn_off_all(&mut self) {
        for ch in Channel::all() {
            self.turn_off(*ch);
        }
    }

    /// True when at least one channel is lit.
    pub fn any_enabled(&self) -> bool {
        self.channels.iter().any(|c| c.enabled)
    }

    /// Number of lit channels.
    pub fn enabled_count(&self) -> usize {
        self.channels.iter().filter(|c| c.enabled).count()
    }

    /// Compact 35-byte blob (upstream compatible).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::BYTE_LEN);
        bytes.extend_from_slice(Self::MAGIC);
        bytes.push(Self::VERSION);
        for ch in Channel::all() {
            let cfg = self.get(*ch);
            bytes.push(cfg.color[0]);
            bytes.push(cfg.color[1]);
            bytes.push(cfg.color[2]);
            bytes.push(cfg.brightness);
            bytes.push(u8::from(cfg.enabled));
        }
        bytes
    }

    /// Parses the upstream format; returns `None` for anything unexpected so
    /// callers can fall back to defaults instead of failing to start.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::BYTE_LEN {
            return None;
        }
        if &bytes[0..4] != Self::MAGIC || bytes[4] != Self::VERSION {
            return None;
        }
        let mut state = Self::default();
        let mut offset = 5;
        for ch in Channel::all() {
            state.channels[ch.index()] = ChannelConfig {
                color: [bytes[offset], bytes[offset + 1], bytes[offset + 2]],
                brightness: bytes[offset + 3].min(100),
                enabled: bytes[offset + 4] != 0,
            };
            offset += 5;
        }
        Some(state)
    }

    /// Expands to the 200-LED frame written to the HID device.
    pub fn to_leds(&self) -> Vec<[u8; 3]> {
        let mut leds = vec![[0u8, 0u8, 0u8]; TOTAL_LEDS];
        for ch in Channel::all() {
            let cfg = self.get(*ch);
            if cfg.enabled {
                let scaled = cfg.scaled();
                for i in ch.led_range() {
                    leds[i] = scaled;
                }
            }
        }
        leds
    }

    pub fn statuses(&self) -> Vec<ChannelStatus> {
        Channel::all()
            .iter()
            .map(|ch| {
                let cfg = self.get(*ch);
                let render = if cfg.enabled { cfg.scaled() } else { [0, 0, 0] };
                ChannelStatus {
                    id: *ch,
                    label: ch.label().to_string(),
                    name: ch.name().to_string(),
                    description: ch.description().to_string(),
                    leds: ch.led_count(),
                    color: cfg.color,
                    hex: cfg.hex(),
                    render_hex: ChannelConfig::rgb(render, 100).hex(),
                    brightness: cfg.brightness,
                    enabled: cfg.enabled,
                }
            })
            .collect()
    }

    /// Average brightness of the lit channels, for the dashboard summary tile.
    pub fn average_brightness(&self) -> u8 {
        let lit: Vec<_> = self.channels.iter().filter(|c| c.enabled).collect();
        if lit.is_empty() {
            return 0;
        }
        let sum: u32 = lit.iter().map(|c| c.brightness as u32).sum();
        (sum / lit.len() as u32) as u8
    }

    /// Convenience for tests and the settings screen: 6 hex strings in channel order.
    pub fn as_hex_list(&self) -> Vec<String> {
        Channel::all().iter().map(|c| self.get(*c).hex()).collect()
    }

    /// Applies a solid colour to every channel at once.
    pub fn fill(&mut self, color: [u8; 3], brightness: u8) {
        for ch in Channel::all() {
            self.set(*ch, ChannelConfig::rgb(color, brightness));
        }
    }

    /// Scales every lit channel (used by the global brightness slider).
    pub fn set_global_brightness(&mut self, brightness: u8) {
        for i in 0..self.channels.len() {
            if self.channels[i].enabled {
                self.channels[i].brightness = brightness.min(100);
            }
        }
    }

    /// Colour of a channel after brightness, as the UI swatch.
    pub fn rendered(&self, channel: Channel) -> [u8; 3] {
        let cfg = self.get(channel);
        if cfg.enabled {
            scale_color(cfg.color, cfg.brightness)
        } else {
            [0, 0, 0]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_format_is_upstream_compatible() {
        let mut state = RgbState::default();
        state.set_rgb(Channel::Argb1, [255, 0, 0], 100);
        state.set_rgb(Channel::Argb2, [0, 0, 255], 80);

        let bytes = state.to_bytes();
        assert_eq!(bytes.len(), 35);
        assert_eq!(bytes.len(), RgbState::BYTE_LEN);
        assert_eq!(&bytes[0..4], b"CCTL");
        assert_eq!(bytes[4], 1);

        // Channel 3 (5V_1) starts at 5 + 3*5 = 20.
        assert_eq!(&bytes[20..25], &[255, 0, 0, 100, 1]);
        // Channel 4 (5V_2) starts at 25.
        assert_eq!(&bytes[25..30], &[0, 0, 255, 80, 1]);

        let back = RgbState::from_bytes(&bytes).expect("round-trip");
        assert_eq!(back, state);
    }

    #[test]
    fn rejects_foreign_or_truncated_blobs() {
        assert!(RgbState::from_bytes(&[0, 1, 2]).is_none());
        assert!(RgbState::from_bytes(&[b'X', b'X', b'X', b'X', 1, 0, 0, 0, 0, 0]).is_none());
        let mut wrong_version = RgbState::default().to_bytes();
        wrong_version[4] = 2;
        assert!(RgbState::from_bytes(&wrong_version).is_none());
    }

    #[test]
    fn led_frame_matches_channel_layout() {
        let mut state = RgbState::default();
        state.set_rgb(Channel::Argb1, [255, 0, 0], 100);
        state.set_rgb(Channel::Argb2, [0, 0, 255], 80);
        state.set_rgb(Channel::Led, [0, 255, 0], 50);

        let leds = state.to_leds();
        assert_eq!(leds.len(), TOTAL_LEDS);
        assert_eq!(leds[0], [0, 127, 0]);
        assert_eq!(leds[17], [0, 127, 0]);
        assert_eq!(leds[18], [0, 0, 0], "12V channels stay dark");
        for i in Channel::Argb1.led_range() {
            assert_eq!(leds[i], [255, 0, 0]);
        }
        for i in Channel::Argb2.led_range() {
            assert_eq!(leds[i], [0, 0, 204]);
        }
        for i in Channel::Argb3.led_range() {
            assert_eq!(leds[i], [0, 0, 0]);
        }
    }

    #[test]
    fn turning_one_channel_off_leaves_the_others_alone() {
        let mut state = RgbState::default();
        state.set_rgb(Channel::Argb1, [255, 0, 0], 100);
        state.set_rgb(Channel::Argb3, [0, 255, 0], 60);

        state.turn_off(Channel::Argb1);

        assert!(!state.get(Channel::Argb1).enabled);
        assert_eq!(state.get(Channel::Argb1).color, [0, 0, 0]);
        let argb3 = state.get(Channel::Argb3);
        assert!(argb3.enabled);
        assert_eq!(argb3.color, [0, 255, 0]);
        assert_eq!(argb3.brightness, 60);
    }

    #[test]
    fn global_brightness_only_touches_lit_channels() {
        let mut state = RgbState::default();
        state.set_rgb(Channel::Led, [255, 255, 255], 100);
        state.set_global_brightness(25);
        assert_eq!(state.get(Channel::Led).brightness, 25);
        assert_eq!(state.get(Channel::Argb2).brightness, 0);
    }

    #[test]
    fn statuses_project_brightness() {
        let mut state = RgbState::default();
        state.set_rgb(Channel::Rgb1, [200, 100, 50], 50);
        let statuses = state.statuses();
        assert_eq!(statuses.len(), 6);
        assert_eq!(statuses[1].id, Channel::Rgb1);
        assert_eq!(statuses[1].hex, "#c86432");
        assert_eq!(statuses[1].render_hex, "#643219");
        assert_eq!(statuses[1].leds, 1);
        assert_eq!(statuses[0].leds, 18);
    }

    #[test]
    fn fill_and_counts() {
        let mut state = RgbState::default();
        assert_eq!(state.enabled_count(), 0);
        assert!(!state.any_enabled());
        state.fill([10, 20, 30], 100);
        assert_eq!(state.enabled_count(), 6);
        assert!(state.any_enabled());
        state.turn_off_all();
        assert_eq!(state.enabled_count(), 0);
        assert_eq!(state.as_hex_list(), vec!["#000000"; 6]);
    }
}
