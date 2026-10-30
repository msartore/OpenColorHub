//! Colorful onboard RGB controller backend (Windows HID).
//!
//! The controller is a composite USB device, `2F4C:1000`, which exposes three
//! interfaces: a keyboard collection (`MI_00`) and two vendor-defined
//! collections (`MI_01`, `MI_02`). The LED data path is the vendor collection
//! with usage page `0xFF01`; the firmware sits on the other one.
//!
//! Frame layout is identical to the Linux backend — it comes from
//! [`crate::backend::frame`] — only the transport differs: SetupAPI + `hid.dll`
//! instead of `/dev/hidraw`.

use std::fs;
use std::path::PathBuf;

use crate::backend::LedBackend;
use crate::backend::frame;
use crate::backend::windows::hid::{self, HidDevice, HidDeviceInfo, Transport};
use crate::error::{Error, Result};
use crate::model::{COLORFUL_PID, COLORFUL_USAGE_PAGE, COLORFUL_VID, DeviceInfo, TOTAL_LEDS};
use crate::state::RgbState;

/// Environment override for the report type, for boards that only answer one.
/// Accepted values: `output` (default), `feature`.
const TRANSPORT_ENV: &str = "OPENCOLORHUB_HID_TRANSPORT";

pub struct WindowsLed {
    dev: HidDevice,
    state_path: PathBuf,
    state: RgbState,
    transport: Transport,
}

impl WindowsLed {
    /// Finds the controller, opens it and loads the persisted state.
    pub fn open(state_path: PathBuf) -> Result<Self> {
        let info = discover()?;
        let dev = HidDevice::open(info)?;
        let transport = transport_from_env();
        let state = fs::read(&state_path)
            .ok()
            .and_then(|bytes| RgbState::from_bytes(&bytes))
            .unwrap_or_default();
        Ok(Self {
            dev,
            state_path,
            state,
            transport,
        })
    }

    /// The interface currently used for writes.
    pub fn device_info(&self) -> &HidDeviceInfo {
        self.dev.info()
    }

    pub fn transport(&self) -> Transport {
        self.transport
    }

    fn report_len(&self) -> usize {
        match self.transport {
            Transport::Output => self.dev.info().output_report_len,
            Transport::Feature => self.dev.info().feature_report_len,
        }
    }

    fn push(&self, report: &[u8]) -> Result<()> {
        match self.transport {
            Transport::Output => self.dev.write_output(report),
            Transport::Feature => self.dev.send_feature(report),
        }
    }

    /// Streams the ten data packets and the commit packet.
    fn write_frame(&self) -> Result<()> {
        let report_len = self.report_len();
        if report_len < 2 {
            return Err(Error::backend(format!(
                "HID interface declares no usable report length (output={}, feature={})",
                self.dev.info().output_report_len,
                self.dev.info().feature_report_len
            )));
        }

        let leds = self.state.to_leds();
        for pkt in frame::frame(&leds) {
            let report = hid::to_report(&pkt, frame::PACKET_LEN, report_len);
            self.push(&report)?;
        }
        Ok(())
    }

    fn save_state(&self) -> Result<()> {
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.state_path, self.state.to_bytes())?;
        Ok(())
    }
}

impl LedBackend for WindowsLed {
    fn info(&self) -> DeviceInfo {
        let dev = self.dev.info();
        let transport = match self.transport {
            Transport::Output => "output report",
            Transport::Feature => "feature report",
        };
        let iface = dev
            .interface_number()
            .map(|n| format!(" MI_{n:02}"))
            .unwrap_or_default();
        DeviceInfo::hardware(
            "Colorful RGB controller",
            format!(
                "USB HID {COLORFUL_VID:04X}:{COLORFUL_PID:04X}{iface} — {TOTAL_LEDS} LEDs over {} packets, {transport}",
                frame::PACKET_COUNT
            ),
        )
        .with_path(dev.path.clone())
    }

    fn state(&self) -> &RgbState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut RgbState {
        &mut self.state
    }

    fn apply(&mut self) -> Result<()> {
        if let Err(first) = self.write_frame() {
            // The controller family answers either output or feature reports
            // depending on firmware. Switch once, then keep the winner.
            let can_fallback =
                self.transport == Transport::Output && self.dev.info().feature_report_len > 1;
            if !can_fallback {
                return Err(first);
            }
            self.transport = Transport::Feature;
            self.write_frame().map_err(|second| {
                Error::backend(format!(
                    "HID write failed on both transports — output: {first}; feature: {second}"
                ))
            })?;
        }
        self.save_state()
    }

    fn state_path(&self) -> Option<PathBuf> {
        Some(self.state_path.clone())
    }
}

fn transport_from_env() -> Transport {
    match std::env::var(TRANSPORT_ENV) {
        Ok(v) if v.eq_ignore_ascii_case("feature") => Transport::Feature,
        _ => Transport::Output,
    }
}

/// Every Colorful HID interface present, for diagnostics and error messages.
pub fn list_colorful() -> Vec<HidDeviceInfo> {
    hid::enumerate()
        .into_iter()
        .filter(|d| d.vendor_id == COLORFUL_VID && d.product_id == COLORFUL_PID)
        .collect()
}

/// The interface [`discover`] would pick: the documented `0xFF01` collection
/// first, then any other vendor-defined collection that accepts a write.
fn choose(candidates: &[HidDeviceInfo]) -> Option<&HidDeviceInfo> {
    candidates
        .iter()
        .find(|d| d.usage_page == COLORFUL_USAGE_PAGE && d.is_writable())
        .or_else(|| {
            candidates
                .iter()
                .find(|d| d.is_vendor_defined() && d.is_writable())
        })
}

/// Locates the LED collection.
///
/// Preference order:
/// 1. usage page `0xFF01` **and** a writable report — the documented controller,
/// 2. any vendor-defined usage page with a writable report — covers firmware
///    revisions that renumber the collection,
/// 3. `OPENCOLORHUB_HID_PATH` as an explicit override.
///
/// The keyboard collection (`MI_00`) is never selected: its usage page is the
/// generic desktop/keyboard page, not a vendor page.
pub fn discover() -> Result<HidDeviceInfo> {
    let candidates = list_colorful();

    if let Some(found) = choose(&candidates) {
        return Ok(found.clone());
    }

    if let Ok(path) = std::env::var("OPENCOLORHUB_HID_PATH")
        && let Some(found) = candidates.iter().find(|d| d.path == path)
    {
        return Ok(found.clone());
    }

    Err(Error::unavailable(not_found_message(&candidates)))
}

fn not_found_message(candidates: &[HidDeviceInfo]) -> String {
    let seen = if candidates.is_empty() {
        "no interface with that VID:PID is present".to_string()
    } else {
        candidates
            .iter()
            .map(|d| {
                format!(
                    "{} (usage page 0x{:04X}, output {}, feature {})",
                    d.interface_number()
                        .map(|n| format!("MI_{n:02}"))
                        .unwrap_or_else(|| "?".into()),
                    d.usage_page,
                    d.output_report_len,
                    d.feature_report_len
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        "Colorful motherboard RGB controller (VID: 0x{COLORFUL_VID:04X}, PID: 0x{COLORFUL_PID:04X}, \
         UsagePage: 0x{COLORFUL_USAGE_PAGE:04X}) not usable — {seen}. \
         Enable the onboard RGB controller in BIOS, or set OPENCOLORHUB_HID_TRANSPORT=feature.",
        COLORFUL_VID = COLORFUL_VID,
        COLORFUL_PID = COLORFUL_PID,
        COLORFUL_USAGE_PAGE = COLORFUL_USAGE_PAGE,
    )
}

/// One row of `colorctl rgb probe` output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HidDiagnostic {
    pub path: String,
    pub interface: Option<u8>,
    pub vendor_id: String,
    pub product_id: String,
    pub usage_page: String,
    pub usage: u16,
    pub product: Option<String>,
    pub output_report_len: usize,
    pub feature_report_len: usize,
    /// True for the interface [`discover`] would open.
    pub selected: bool,
}

/// Every Colorful HID interface with its caps, for `--json` output and support
/// requests. Read-only: it opens each interface just long enough to read the
/// descriptor and never writes.
pub fn diagnostics() -> Vec<HidDiagnostic> {
    let candidates = list_colorful();
    let chosen = choose(&candidates).map(|d| d.path.clone());
    candidates
        .into_iter()
        .map(|d| HidDiagnostic {
            selected: chosen.as_deref() == Some(d.path.as_str()),
            interface: d.interface_number(),
            vendor_id: format!("{:04X}", d.vendor_id),
            product_id: format!("{:04X}", d.product_id),
            usage_page: format!("{:04X}", d.usage_page),
            usage: d.usage,
            product: d.product,
            output_report_len: d.output_report_len,
            feature_report_len: d.feature_report_len,
            path: d.path,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_defaults_to_output() {
        assert_eq!(transport_from_env(), Transport::Output);
    }

    #[test]
    fn list_colorful_only_returns_matching_vid_pid() {
        for d in list_colorful() {
            assert_eq!(d.vendor_id, COLORFUL_VID);
            assert_eq!(d.product_id, COLORFUL_PID);
        }
    }

    #[test]
    fn discovery_error_mentions_the_expected_ids() {
        let msg = not_found_message(&[]);
        assert!(msg.contains("2F4C"));
        assert!(msg.contains("1000"));
        assert!(msg.contains("FF01"));
    }

    #[test]
    fn interface_number_parses_from_the_device_path() {
        let info = HidDeviceInfo {
            path: r"\\?\hid#vid_2f4c&pid_1000&mi_02#8&1a2b3c&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}".into(),
            vendor_id: COLORFUL_VID,
            product_id: COLORFUL_PID,
            version_number: 0,
            usage_page: COLORFUL_USAGE_PAGE,
            usage: 1,
            product: None,
            output_report_len: 65,
            feature_report_len: 65,
            input_report_len: 65,
        };
        assert_eq!(info.interface_number(), Some(2));
        assert!(info.is_vendor_defined());
        assert!(info.is_writable());
    }
}
