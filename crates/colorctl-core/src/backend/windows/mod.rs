//! Windows hardware backends.
//!
//! * [`hid`] — SetupAPI enumeration plus `hid.dll` report I/O.
//! * [`led`] — Colorful onboard RGB controller over that HID layer.
//! * [`hwinfo`] — read-only fan/temperature telemetry from HWiNFO's shared
//!   memory, for the half of the job Windows cannot do directly.
//! * [`pawnio`] — Super I/O access through PawnIO's sandboxed kernel driver.
//!   This is the real fan path: a signed bytecode module performs port I/O
//!   inside a driver-enforced allow-list, so no vulnerable driver ships here.
//!
//! Fan control needs a kernel driver either way. PawnIO is the supported one:
//! it validates every port against the Super I/O slot and BAR windows it
//! discovered, and never exposes physical memory. Without it the backend falls
//! back to HWiNFO readings (read-only) and finally to the simulator.

pub mod fan;
pub mod hid;
pub mod hwinfo;
pub mod led;
pub mod pawnio;
