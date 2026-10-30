//! Static information for the About screen.

use serde::{Deserialize, Serialize};

use crate::board::{BOARDS, BoardInfo};
use crate::model::{
    CHANNELS, COLORFUL_PID, COLORFUL_VID, Channel, FAN_HEADERS, FanHeader, NCT5584D_CHIP_ID,
    TOTAL_LEDS,
};
use crate::{UPSTREAM_AUTHOR, UPSTREAM_NAME, UPSTREAM_URL};

fn default_vid() -> u16 {
    COLORFUL_VID
}

fn default_pid() -> u16 {
    COLORFUL_PID
}

/// Attribution for the project this work derives from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Upstream {
    pub name: String,
    pub url: String,
    pub author: String,
    pub license: String,
}

/// Build-time facts, injected by the shell crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildMeta {
    pub version: String,
    /// Tauri runtime version, when running inside the desktop shell.
    pub tauri: Option<String>,
    /// Rust compiler used for the build (`rustc --version` short form).
    pub rustc: String,
    pub profile: String,
    pub os: String,
    pub arch: String,
    pub family: String,
}

/// One row of the supported-hardware table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeaderInfo {
    pub id: String,
    pub label: String,
    pub bank: u8,
    pub description: String,
}

/// One row of the channel table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub label: String,
    pub leds: usize,
    pub range: String,
    pub description: String,
}

/// Everything the About screen renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AboutInfo {
    pub name: String,
    pub version: String,
    pub tagline: String,
    pub license: String,
    pub build: BuildMeta,
    pub upstream: Upstream,
    pub chip: String,
    pub chip_id: u16,
    #[serde(default = "default_vid")]
    pub vid: u16,
    #[serde(default = "default_pid")]
    pub pid: u16,
    pub total_leds: usize,
    /// What this machine actually is, and how well it matches the catalogue.
    pub board: BoardInfo,
    pub boards: Vec<String>,
    pub headers: Vec<HeaderInfo>,
    pub channels: Vec<ChannelInfo>,
    pub capabilities: Vec<String>,
    pub paths: Paths,
    pub links: Vec<Link>,
}

/// On-disk locations shown so users can find (or delete) their data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paths {
    pub data_dir: String,
    pub settings: String,
    pub state: String,
    pub config_override: Option<String>,
}

/// External link rendered as a button.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub label: String,
    pub url: String,
    pub kind: String,
}

/// Inputs the shell supplies when building the About payload.
pub struct AboutParams {
    pub name: String,
    pub version: String,
    pub tagline: String,
    pub board: BoardInfo,
    pub tauri_version: Option<String>,
    pub data_dir: String,
    pub settings_path: String,
    pub state_path: String,
    pub config_override: Option<String>,
}

impl AboutInfo {
    pub fn build(p: AboutParams) -> Self {
        let chip_id = NCT5584D_CHIP_ID;
        Self {
            name: p.name,
            version: p.version,
            tagline: p.tagline,
            license: "GPL-3.0-only".to_string(),
            build: BuildMeta {
                version: crate::VERSION.to_string(),
                tauri: p.tauri_version,
                rustc: rustc_version().to_string(),
                profile: if cfg!(debug_assertions) {
                    "debug".to_string()
                } else {
                    "release".to_string()
                },
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                family: std::env::consts::FAMILY.to_string(),
            },
            upstream: Upstream {
                name: UPSTREAM_NAME.to_string(),
                url: UPSTREAM_URL.to_string(),
                author: UPSTREAM_AUTHOR.to_string(),
                license: "MIT".to_string(),
            },
            chip: "Nuvoton NCT5584D".to_string(),
            chip_id,
            vid: COLORFUL_VID,
            pid: COLORFUL_PID,
            total_leds: TOTAL_LEDS,
            board: p.board,
            boards: supported_boards(),
            headers: FanHeader::all()
                .iter()
                .map(|h| HeaderInfo {
                    id: h.id().to_string(),
                    label: h.label().to_string(),
                    bank: h.bank(),
                    description: h.description().to_string(),
                })
                .collect(),
            channels: Channel::all()
                .iter()
                .map(|c| {
                    let range = c.led_range();
                    ChannelInfo {
                        id: c.id().to_string(),
                        label: c.label().to_string(),
                        leds: c.led_count(),
                        range: format!("{}..{}", range.start, range.end),
                        description: c.description().to_string(),
                    }
                })
                .collect(),
            capabilities: vec![
                format!(
                    "{} PWM fan headers with 4-point SmartFan curves",
                    FAN_HEADERS.len()
                ),
                format!(
                    "{} lighting channels across {TOTAL_LEDS} LEDs",
                    CHANNELS.len()
                ),
                "Manual duty, built-in profiles, live tachometers".to_string(),
                "State compatible with upstream colorctl (CCTL v1)".to_string(),
                "Linux hardware backends plus a cross-platform simulator".to_string(),
            ],
            paths: Paths {
                data_dir: p.data_dir,
                settings: p.settings_path,
                state: p.state_path,
                config_override: p.config_override,
            },
            links: vec![
                Link {
                    label: "opencolorhub on GitHub".to_string(),
                    url: "https://github.com/msartore/OpenColorHub".to_string(),
                    kind: "primary".to_string(),
                },
                Link {
                    label: "Upstream colorctl".to_string(),
                    url: UPSTREAM_URL.to_string(),
                    kind: "secondary".to_string(),
                },
                Link {
                    label: "Tauri".to_string(),
                    url: "https://tauri.app".to_string(),
                    kind: "secondary".to_string(),
                },
            ],
        }
    }
}

/// Verified board catalogue, rendered on the About screen.
pub fn supported_boards() -> Vec<String> {
    BOARDS
        .iter()
        .map(|board| format!("{} · {}", board.name, board.series))
        .collect()
}

/// Compiler identification captured at build time (see `build.rs`).
fn rustc_version() -> &'static str {
    option_env!("RUSTC_VERSION").unwrap_or("rustc (unknown)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> AboutParams {
        AboutParams {
            name: "OpenColorHub".into(),
            version: "0.2.0".into(),
            tagline: "Fan and lighting control for Colorful motherboards".into(),
            board: crate::board::demo_board(),
            tauri_version: Some("2.9.0".into()),
            data_dir: "/tmp/opencolorhub".into(),
            settings_path: "/tmp/opencolorhub/settings.json".into(),
            state_path: "/tmp/opencolorhub/rgb.state".into(),
            config_override: None,
        }
    }

    #[test]
    fn about_carries_attribution_and_tables() {
        let about = AboutInfo::build(params());
        assert_eq!(about.upstream.name, "colorctl");
        assert_eq!(about.upstream.license, "MIT");
        assert_eq!(about.chip_id, 0xD42A);
        assert_eq!(about.vid, 0x2F4C);
        assert_eq!(about.pid, 0x1000);
        assert_eq!(about.headers.len(), FAN_HEADERS.len());
        assert_eq!(about.channels.len(), CHANNELS.len());
        assert_eq!(about.total_leds, 200);
        let total: usize = about.channels.iter().map(|c| c.leds).sum();
        assert_eq!(total, TOTAL_LEDS);
        assert!(!about.boards.is_empty());
        assert_eq!(about.board.confidence, crate::BoardConfidence::Verified);
        assert_eq!(about.board.series.as_deref(), Some("B650"));
        assert!(
            about
                .boards
                .iter()
                .any(|b| b.contains("CVN B650M GAMING FROZEN"))
        );
        assert!(!about.links.is_empty());
        assert_eq!(about.paths.state, "/tmp/opencolorhub/rgb.state");
    }
}
