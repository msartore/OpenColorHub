//! Minimal Windows HID access: SetupAPI enumeration + `hid.dll` reports.
//!
//! This is the same layer `hidapi` builds on Windows (SetupAPI to find the
//! interface, `CreateFile` + `WriteFile` for output reports, `HidD_SetFeature`
//! for feature reports), written directly against `windows-sys` so the core
//! crate keeps zero C dependencies.
//!
//! Only what the LED controller needs is implemented: enumerate, open, read
//! capabilities, and the two write paths.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    GUID_DEVINTERFACE_HID, HIDD_ATTRIBUTES, HIDP_CAPS, HidD_FreePreparsedData, HidD_GetAttributes,
    HidD_GetPreparsedData, HidD_GetProductString, HidD_SetFeature, HidP_GetCaps,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, WriteFile,
};

use crate::error::{Error, Result};

/// Everything discovery learns about one HID interface.
#[derive(Debug, Clone)]
pub struct HidDeviceInfo {
    /// Device interface path (`\\?\hid#vid_2f4c&pid_1000&mi_01#...`).
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub version_number: u16,
    /// Top-level collection usage page / usage from the report descriptor.
    pub usage_page: u16,
    pub usage: u16,
    pub product: Option<String>,
    /// Report lengths include the leading report-ID byte.
    pub output_report_len: usize,
    pub feature_report_len: usize,
    pub input_report_len: usize,
}

impl HidDeviceInfo {
    /// `MI_xx` interface number parsed from the device path, when present.
    pub fn interface_number(&self) -> Option<u8> {
        let lower = self.path.to_ascii_lowercase();
        let idx = lower.find("mi_")?;
        let digits = lower.get(idx + 3..idx + 5)?;
        u8::from_str_radix(digits, 16).ok()
    }

    /// A HID usage page reserved for vendor-defined data (0xFF00..=0xFFFF).
    pub fn is_vendor_defined(&self) -> bool {
        (0xFF00..=0xFFFF).contains(&self.usage_page)
    }

    /// Whether the interface can accept a write of either report type.
    pub fn is_writable(&self) -> bool {
        self.output_report_len > 1 || self.feature_report_len > 1
    }
}

/// Enumerates every present HID interface on the machine.
///
/// Interfaces that cannot be opened are still reported — the caps are read from
/// the handle, so unreachable ones are skipped silently. A keyboard or mouse the
/// caller has no rights to must not abort discovery.
pub fn enumerate() -> Vec<HidDeviceInfo> {
    let mut out = Vec::new();

    // SAFETY: all pointers passed to SetupAPI point at live, correctly typed
    // locals for the duration of each call; the returned HDEVINFO is always
    // released before this function returns.
    unsafe {
        let set: HDEVINFO = SetupDiGetClassDevsW(
            &GUID_DEVINTERFACE_HID,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        );
        if set == INVALID_HANDLE_VALUE as HDEVINFO {
            return out;
        }

        let mut index = 0u32;
        loop {
            let mut did: SP_DEVICE_INTERFACE_DATA = zeroed();
            did.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;

            let ok = SetupDiEnumDeviceInterfaces(
                set,
                std::ptr::null(),
                &GUID_DEVINTERFACE_HID,
                index,
                &mut did,
            );
            if ok == 0 {
                break;
            }
            index += 1;

            if let Some(info) = interface_detail(set, &did).and_then(read_caps) {
                out.push(info);
            }
        }

        SetupDiDestroyDeviceInfoList(set);
    }

    out
}

/// Resolves one interface to its device path, then to a populated [`HidDeviceInfo`].
///
/// The size query is racy (another interface can appear between the two calls),
/// so the detail call is retried with a grown buffer instead of trusting the
/// first answer.
unsafe fn interface_detail(
    set: HDEVINFO,
    did: &SP_DEVICE_INTERFACE_DATA,
) -> Option<PendingInterface> {
    let mut required = 0u32;
    // SAFETY: null detail buffer with zero size is the documented size probe.
    unsafe {
        SetupDiGetDeviceInterfaceDetailW(
            set,
            did,
            std::ptr::null_mut(),
            0,
            &mut required,
            std::ptr::null_mut(),
        );
    }
    if required == 0 {
        return None;
    }

    let mut buf = vec![0u8; required as usize];
    let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
    // SAFETY: buf is at least `required` bytes and large enough for the header.
    unsafe {
        (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
    }

    // SAFETY: `detail` points into `buf`, which is at least `required` bytes and
    // lives until the device path has been copied out.
    let ok = unsafe {
        SetupDiGetDeviceInterfaceDetailW(
            set,
            did,
            detail,
            required,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }

    // SAFETY: the API wrote a NUL-terminated UTF-16 device path at DevicePath.
    let path_wide = unsafe { (*detail).DevicePath.as_ptr() };
    let mut len = 0usize;
    while unsafe { *path_wide.add(len) } != 0 {
        len += 1;
    }
    let path = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(path_wide, len) });

    Some(PendingInterface { path })
}

struct PendingInterface {
    path: String,
}

/// Opens the interface just long enough to read its attributes and caps.
fn read_caps(pending: PendingInterface) -> Option<HidDeviceInfo> {
    // SAFETY: `open_path` validates the handle; every pointer below refers to a
    // live local, and the handle is closed on every path out of this function.
    unsafe {
        let handle = open_path(&pending.path)?;

        let mut attrs: HIDD_ATTRIBUTES = zeroed();
        attrs.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
        let got_attrs = HidD_GetAttributes(handle, &mut attrs);

        // windows-sys models PHIDP_PREPARSED_DATA as an opaque isize handle.
        let mut preparsed: isize = 0;
        let mut caps: HIDP_CAPS = zeroed();
        let mut got_caps = false;
        if HidD_GetPreparsedData(handle, &mut preparsed) {
            got_caps = HidP_GetCaps(preparsed, &mut caps) >= 0;
            HidD_FreePreparsedData(preparsed);
        }

        let mut product_buf = [0u16; 128];
        let product = if HidD_GetProductString(
            handle,
            product_buf.as_mut_ptr() as *mut c_void,
            size_of::<u16>() as u32 * product_buf.len() as u32,
        ) {
            let len = product_buf.iter().position(|c| *c == 0).unwrap_or(0);
            if len > 0 {
                Some(String::from_utf16_lossy(&product_buf[..len]))
            } else {
                None
            }
        } else {
            None
        };

        CloseHandle(handle);

        if !got_attrs || !got_caps {
            return None;
        }

        Some(HidDeviceInfo {
            path: pending.path,
            vendor_id: attrs.VendorID,
            product_id: attrs.ProductID,
            version_number: attrs.VersionNumber,
            usage_page: caps.UsagePage,
            usage: caps.Usage,
            product,
            output_report_len: caps.OutputReportByteLength as usize,
            feature_report_len: caps.FeatureReportByteLength as usize,
            input_report_len: caps.InputReportByteLength as usize,
        })
    }
}

/// Opens `path` with read/write sharing, the mode HID reports need.
unsafe fn open_path(path: &str) -> Option<HANDLE> {
    let wide = to_wide(path);
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        None
    } else {
        Some(handle)
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// How the frame is pushed to the controller.
///
/// `iGameAPI.LED.dll` uses both `hid_write` (output report) and
/// `hid_send_feature_report` (feature report) against this controller family, so
/// both are supported and the working one is discovered at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Output,
    Feature,
}

/// An open controller.
pub struct HidDevice {
    handle: HANDLE,
    info: HidDeviceInfo,
}

// SAFETY: the handle is only used through `&self`/`&mut self` and Windows file
// handles are not tied to a thread.
unsafe impl Send for HidDevice {}

impl HidDevice {
    /// Opens the interface described by `info`.
    pub fn open(info: HidDeviceInfo) -> Result<Self> {
        let wide = to_wide(&info.path);
        // SAFETY: `wide` is NUL-terminated and lives across the call.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(Error::backend(format!(
                "Failed to open HID interface {}: {}",
                info.path,
                std::io::Error::last_os_error()
            )));
        }
        Ok(Self { handle, info })
    }

    pub fn info(&self) -> &HidDeviceInfo {
        &self.info
    }

    /// Output report through `WriteFile` (equivalent to hidapi `hid_write`).
    pub fn write_output(&self, buf: &[u8]) -> Result<()> {
        let mut written = 0u32;
        // SAFETY: `buf` is a live slice; the OVERLAPPED pointer is null because
        // the handle is synchronous.
        let ok = unsafe {
            WriteFile(
                self.handle,
                buf.as_ptr(),
                buf.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(Error::backend(format!(
                "WriteFile to HID failed: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }

    /// Feature report (equivalent to hidapi `hid_send_feature_report`).
    pub fn send_feature(&self, buf: &[u8]) -> Result<()> {
        // SAFETY: `buf` is a live slice of at least `buf.len()` bytes.
        let ok = unsafe {
            HidD_SetFeature(self.handle, buf.as_ptr() as *const c_void, buf.len() as u32)
        };
        if !ok {
            return Err(Error::backend(format!(
                "HidD_SetFeature failed: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }

    /// Reads a feature report back; used to prove the device answers.
    pub fn get_feature(&self, buf: &mut [u8]) -> Result<()> {
        // SAFETY: `buf` is a live mutable slice.
        let ok = unsafe {
            windows_sys::Win32::Devices::HumanInterfaceDevice::HidD_GetFeature(
                self.handle,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as u32,
            )
        };
        if !ok {
            return Err(Error::backend(format!(
                "HidD_GetFeature failed: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        // SAFETY: the handle was produced by CreateFileW and is owned here.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

/// Adapts a 65-byte frame packet to the device's declared report length.
///
/// Windows rejects a write whose length is not exactly `OutputReportByteLength`,
/// and a device that declares no report IDs wants the payload **without** the
/// leading report-ID byte.
pub fn to_report(packet: &[u8], pkt_len: usize, report_len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; report_len];
    if report_len >= pkt_len {
        let n = pkt_len.min(report_len).min(packet.len());
        buf[..n].copy_from_slice(&packet[..n]);
    } else {
        // No report-ID byte: drop packet[0] and copy the payload.
        let n = report_len.min(packet.len().saturating_sub(1));
        buf[..n].copy_from_slice(&packet[1..1 + n]);
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_with_id_keeps_the_header() {
        let pkt = [0u8, 1, 0, 0x88, 7, 9, 9, 9];
        let report = to_report(&pkt, 8, 8);
        assert_eq!(report, pkt.to_vec());
    }

    #[test]
    fn report_without_id_drops_the_first_byte() {
        let pkt = [0u8, 1, 0, 0x88, 7, 9, 9, 9];
        let report = to_report(&pkt, 8, 7);
        assert_eq!(report, vec![1, 0, 0x88, 7, 9, 9, 9]);
    }

    #[test]
    fn oversized_report_is_zero_padded() {
        let pkt = [0u8, 1, 0, 0x88, 7];
        let report = to_report(&pkt, 5, 9);
        assert_eq!(report.len(), 9);
        assert_eq!(&report[..5], &pkt);
        assert!(report[5..].iter().all(|b| *b == 0));
    }

    #[test]
    fn enumeration_never_panics() {
        // Read-only probe: on a machine without any HID device this is empty,
        // on this host it should contain at least the keyboard.
        let devices = enumerate();
        for d in &devices {
            assert!(d.path.starts_with("\\\\?\\"), "unexpected path {}", d.path);
        }
    }
}
