//! Platform neutral core for **opencolorhub**.
//!
//! This crate owns everything that is not tied to a windowing toolkit or a
//! specific operating system:
//!
//! * the hardware model ([`model`]) — fan headers, ARGB channels, curves;
//! * the persistable RGB state in the upstream `CCTL` binary format ([`state`]);
//! * the settings document ([`settings`]);
//! * the backend abstraction ([`backend`]) with a Linux Super-I/O/HID
//!   implementation and a simulated implementation used on every other
//!   platform (and for UI development on Linux without root);
//! * the [`Service`] façade that both the `colorctl` CLI and the Tauri
//!   frontend drive.
//!
//! Nothing here opens a window, and nothing here assumes Linux at compile time:
//! `cargo test -p colorctl-core` passes on Windows and Linux.

pub mod about;
pub mod autostart;
pub mod backend;
pub mod board;
pub mod curve;
pub mod drivers;
pub mod error;
pub mod model;
pub mod platform;
pub mod service;
pub mod settings;
pub mod setups;
pub mod state;
pub mod units;

pub use about::AboutInfo;
pub use board::{BOARDS, BoardConfidence, BoardFamily, BoardIdentity, BoardInfo};
pub use curve::{CURVE_POINTS, CurvePoint, FanCurve, FanProfile};
pub use drivers::{DownloadedFile, DriverCatalogue, DriverFile, DriverKind};
pub use error::{CurveError, Error, Result};
pub use model::{
    Capabilities, Channel, ChannelConfig, ChannelStatus, DeviceInfo, FAN_HEADERS, FanHeader,
    FanMode, FanSample, FanStatus, Snapshot, Telemetry,
};
pub use platform::Elevation;
pub use service::Service;
pub use settings::{BackendMode, Settings, SettingsPatch, TempUnit};
pub use setups::{ChannelSetting, FanSetting, Setup, SetupStore};
pub use state::RgbState;
pub use units::{pct, pwm_to_pct};

/// Upstream project this work derives from.
pub const UPSTREAM_NAME: &str = "colorctl";
pub const UPSTREAM_URL: &str = "https://github.com/Curstantine/colorctl";
pub const UPSTREAM_AUTHOR: &str = "Curstantine";

/// Version of the core crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
