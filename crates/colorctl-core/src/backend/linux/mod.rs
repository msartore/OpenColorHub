//! Linux hardware backends.
//!
//! * [`portio`] — raw port I/O through `/dev/port` plus the Super I/O probe.
//! * [`fan`] — NCT5584D fan/temperature backend built on `portio`.
//! * [`led`] — Colorful onboard RGB controller through `/dev/hidraw`.
//!
//! Adapted from upstream `colorctl` (MIT, © Curstantine); the register
//! programming is unchanged so both tools stay compatible on the same board.

pub mod fan;
pub mod led;
pub mod portio;
