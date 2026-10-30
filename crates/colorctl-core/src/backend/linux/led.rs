//! Colorful onboard RGB controller backend (`/dev/hidraw`).
//!
//! The controller enumerates as HID `2F4C:1000` with usage page `0xFF01`. Frames
//! are 64 or 65 bytes depending on whether the kernel/driver wants a leading
//! report-ID byte, so the first write probes both forms — same approach as
//! upstream, which is the only reliable way across kernels.
//!
//! The packet layout itself lives in [`crate::backend::frame`] and is shared
//! with the Windows backend, so the two transports cannot drift apart.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::backend::LedBackend;
use crate::backend::frame;
use crate::error::{Error, Result};
use crate::model::{
    COLORFUL_PID, COLORFUL_USAGE_PAGE, COLORFUL_VID, Channel, DeviceInfo, TOTAL_LEDS,
};
use crate::state::RgbState;

pub struct LinuxLed {
    dev_path: PathBuf,
    state_path: PathBuf,
    state: RgbState,
}

impl LinuxLed {
    /// Finds the controller and loads the persisted state.
    pub fn open(state_path: PathBuf) -> Result<Self> {
        let dev_path = Self::find_device_path()?;
        let state = fs::read(&state_path)
            .ok()
            .and_then(|bytes| RgbState::from_bytes(&bytes))
            .unwrap_or_default();
        Ok(Self {
            dev_path,
            state_path,
            state,
        })
    }

    pub fn device_path(&self) -> &Path {
        &self.dev_path
    }

    /// Walks `/sys/class/hidraw` looking for VID/PID + usage page 0xFF01.
    pub fn find_device_path() -> Result<PathBuf> {
        let hidraw_dir = Path::new("/sys/class/hidraw");
        if !hidraw_dir.exists() {
            return Err(Error::unavailable("/sys/class/hidraw does not exist"));
        }

        for entry in fs::read_dir(hidraw_dir)? {
            let entry = entry?;
            let device_path = entry.path().join("device");
            let uevent_path = device_path.join("uevent");
            let report_desc_path = device_path.join("report_descriptor");

            let Ok(uevent) = fs::read_to_string(&uevent_path) else {
                continue;
            };

            let mut is_target = false;
            for line in uevent.lines() {
                if let Some(rest) = line.strip_prefix("HID_ID=") {
                    // Format: BusType:VVVVVVVV:PPPPPPPP (8 hex chars, zero padded).
                    let parts: Vec<&str> = rest.split(':').collect();
                    if parts.len() >= 3 {
                        let vid = u32::from_str_radix(parts[1].trim(), 16).unwrap_or(0) as u16;
                        let pid = u32::from_str_radix(parts[2].trim(), 16).unwrap_or(0) as u16;
                        if vid == COLORFUL_VID && pid == COLORFUL_PID {
                            is_target = true;
                        }
                    }
                }
            }
            if !is_target {
                continue;
            }

            // Usage Page 0xFF01 appears as the byte sequence 06 01 FF.
            if let Ok(desc) = fs::read(&report_desc_path)
                && desc.windows(3).any(|w| w == [0x06, 0x01, 0xFF])
            {
                return Ok(Path::new("/dev").join(entry.file_name()));
            }
        }

        Err(Error::unavailable(format!(
            "Colorful motherboard RGB controller (VID: 0x{COLORFUL_VID:04X}, PID: 0x{COLORFUL_PID:04X}, \
             UsagePage: 0x{COLORFUL_USAGE_PAGE:04X}) not found. Enable the onboard RGB controller in BIOS."
        )))
    }

    /// Writes the 200-LED frame and commits it.
    fn write_frame(&self) -> Result<()> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.dev_path)
            .map_err(|e| {
                Error::backend(format!(
                    "Failed to open HID device {:?}: {e}",
                    self.dev_path
                ))
            })?;

        let leds = self.state.to_leds();
        let mut packets = frame::frame(&leds).into_iter();

        // Probe the first packet to discover whether the driver wants the
        // report ID; the answer applies to the whole frame.
        let first = packets
            .next()
            .expect("frame() always yields ten data packets plus a commit");
        let use_report_id = Self::probe_packet(&mut file, &first)?;

        for pkt in packets {
            Self::send(&mut file, &pkt, use_report_id)?;
        }
        Ok(())
    }

    fn probe_packet(file: &mut fs::File, pkt: &[u8; frame::PACKET_LEN]) -> Result<bool> {
        match file.write_all(pkt) {
            Ok(()) => Ok(true),
            Err(e) if e.raw_os_error() == Some(libc::EINVAL) => {
                // Kernel rejected the report-ID form; retry without it.
                file.write_all(&pkt[1..])?;
                Ok(false)
            }
            Err(e) => Err(Error::Io(e)),
        }
    }

    fn send(file: &mut fs::File, pkt: &[u8; frame::PACKET_LEN], use_report_id: bool) -> Result<()> {
        if use_report_id {
            file.write_all(pkt)?;
        } else {
            file.write_all(&pkt[1..])?;
        }
        Ok(())
    }

    /// Persists state world-readable/writable so a root service and the user
    /// session can share it (upstream behaviour).
    fn save_state(&self) -> Result<()> {
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.state_path, self.state.to_bytes())?;
        let _ = fs::set_permissions(&self.state_path, fs::Permissions::from_mode(0o666));
        Ok(())
    }
}

impl LedBackend for LinuxLed {
    fn info(&self) -> DeviceInfo {
        DeviceInfo::hardware(
            "Colorful RGB controller",
            format!(
                "USB HID {COLORFUL_VID:04X}:{COLORFUL_PID:04X} — {} LEDs over {} packets",
                TOTAL_LEDS,
                frame::PACKET_COUNT
            ),
        )
        .with_path(self.dev_path.display().to_string())
    }

    fn state(&self) -> &RgbState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut RgbState {
        &mut self.state
    }

    fn apply(&mut self) -> Result<()> {
        self.write_frame()?;
        self.save_state()?;
        Ok(())
    }

    fn state_path(&self) -> Option<PathBuf> {
        Some(self.state_path.clone())
    }
}

/// Length of one frame packet, kept for callers that predate [`frame`].
pub fn frame_len() -> usize {
    frame::PACKET_LEN
}

/// Channels covered by the first packet (onboard LEDs), useful for the CLI.
pub fn first_packet_channels() -> &'static [Channel] {
    frame::first_packet_channels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_len_matches_the_shared_layout() {
        assert_eq!(frame_len(), 65);
        assert_eq!(frame::LEDS_PER_PACKET * 3 + 5, 65);
    }

    #[test]
    fn total_leds_divide_evenly_into_packets() {
        assert_eq!(TOTAL_LEDS / frame::LEDS_PER_PACKET, 10);
        assert_eq!(TOTAL_LEDS % frame::LEDS_PER_PACKET, 0);
    }

    #[test]
    fn onboard_leds_fit_in_the_probe_packet() {
        let range = Channel::Led.led_range();
        assert!(
            range.end <= frame::LEDS_PER_PACKET,
            "onboard LEDs must fit in packet 0"
        );
    }
}
