//! Super I/O access through PawnIO's sandboxed kernel driver.
//!
//! PawnIO replaces the old "expose physical memory and raw port I/O to user
//! mode" model (`WinRing0`, `inpoutx64`, `RTCore64`) with a kernel runtime that
//! executes signed bytecode modules. The driver keeps its own allow-list: the
//! module can only touch the Super I/O index/data pair it selected and the BAR
//! windows it discovered, and it can never reach the PCI config ports. A
//! compromised caller therefore cannot read arbitrary memory or poke arbitrary
//! hardware the way the WinRing0 driver allowed (CVE-2020-14979).
//!
//! This module wraps two layers:
//!
//! * [`PawnIo`] — `PawnIOLib.dll`, loaded dynamically so a machine without
//!   PawnIO installed simply reports "not available" instead of failing to load
//!   the whole binary.
//! * [`LpcIo`] — the official `LpcIO.bin` module, whose `ioctl_*` functions are
//!   the only way this crate can reach a port.
//!
//! The module is LGPL-2.1 (see `resources/COPYING.LGPL-2.1`); it is passed to
//! the driver as data, never linked into this binary.

use std::ffi::{c_char, c_void};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use crate::error::{Error, Result};

/// The official module, compiled and signed by the PawnIO project (0.2.11).
const LPCIO_MODULE: &[u8] = include_bytes!("../../../resources/LpcIO.bin");

type Hr = i32;

// Function-pointer shapes from PawnIOLib.h. HRESULT is a 32-bit signed integer
// and every buffer is an array of ULONG64, not bytes.
type PawnioVersion = unsafe extern "system" fn(*mut u32) -> Hr;
type PawnioOpen = unsafe extern "system" fn(*mut HANDLE) -> Hr;
type PawnioLoad = unsafe extern "system" fn(HANDLE, *const u8, usize) -> Hr;
type PawnioExecute = unsafe extern "system" fn(
    HANDLE,
    *const c_char,
    *const u64,
    usize,
    *mut u64,
    usize,
    *mut usize,
) -> Hr;
type PawnioClose = unsafe extern "system" fn(HANDLE) -> Hr;

struct Api {
    _library: *mut c_void,
    version: PawnioVersion,
    open: PawnioOpen,
    load: PawnioLoad,
    execute: PawnioExecute,
    close: PawnioClose,
}

// The library handle and function pointers are immutable for the process
// lifetime and the DLL is reference-counted by LoadLibrary.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

/// Loads `PawnIOLib.dll` once per process.
fn api() -> Result<&'static Api> {
    use std::sync::OnceLock;
    static API: OnceLock<std::result::Result<Api, String>> = OnceLock::new();

    match API.get_or_init(load_api) {
        Ok(api) => Ok(api),
        Err(message) => Err(Error::unavailable(message.clone())),
    }
}

fn load_api() -> std::result::Result<Api, String> {
    // PawnIO installs to Program Files and does not put the DLL on the search
    // path. Try the install location, then the system directory, then the
    // default search order (some deployments register it elsewhere).
    let candidates = [
        r"C:\Program Files\PawnIO\PawnIOLib.dll",
        r"C:\Program Files (x86)\PawnIO\PawnIOLib.dll",
        r"C:\Windows\System32\PawnIOLib.dll",
        "PawnIOLib.dll",
    ];

    let mut library: *mut c_void = std::ptr::null_mut();
    for path in candidates {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call.
        let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
        if !handle.is_null() {
            library = handle;
            break;
        }
    }

    if library.is_null() {
        return Err(
            "PawnIO is not installed. Install it from https://pawnio.eu to read fan speeds \
             and temperatures on Windows."
                .to_string(),
        );
    }

    // SAFETY: the module was just loaded; each symbol is looked up once and the
    // signatures match PawnIOLib.h.
    unsafe {
        let symbol = |name: &str| -> std::result::Result<usize, String> {
            let c: Vec<u8> = name.bytes().chain(std::iter::once(0)).collect();
            let address = GetProcAddress(library, c.as_ptr());
            address
                .map(|f| f as usize)
                .ok_or_else(|| format!("PawnIOLib.dll is missing `{name}`"))
        };

        Ok(Api {
            _library: library,
            version: std::mem::transmute::<usize, PawnioVersion>(symbol("pawnio_version")?),
            open: std::mem::transmute::<usize, PawnioOpen>(symbol("pawnio_open")?),
            load: std::mem::transmute::<usize, PawnioLoad>(symbol("pawnio_load")?),
            execute: std::mem::transmute::<usize, PawnioExecute>(symbol("pawnio_execute")?),
            close: std::mem::transmute::<usize, PawnioClose>(symbol("pawnio_close")?),
        })
    }
}

/// `0x80070005` — the device object rejects unelevated callers.
const E_ACCESSDENIED: u32 = 0x8007_0005;

fn describe(hr: i32) -> String {
    let unsigned = hr as u32;
    if unsigned == E_ACCESSDENIED {
        return "access denied — PawnIO requires administrator rights".to_string();
    }
    if (unsigned & 0xFFFF_0000) == 0x8007_0000 {
        // Win32 error wrapped in HRESULT
        let code = unsigned & 0xFFFF;
        let text = std::io::Error::from_raw_os_error(code as i32).to_string();
        return format!("{text} (0x{unsigned:08X})");
    }
    format!("HRESULT 0x{unsigned:08X}")
}

/// An open PawnIO executor with a module loaded.
pub struct PawnIo {
    handle: HANDLE,
}

// SAFETY: a PawnIO handle is a kernel object, not thread-affine; every call is
// an IOCTL. The backend that owns it is serialised behind the service mutex.
unsafe impl Send for PawnIo {}

impl PawnIo {
    /// Opens the driver and loads the `LpcIO` module.
    pub fn open() -> Result<Self> {
        let api = api()?;

        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: valid out-pointer; the handle is closed in `Drop`.
        let hr = unsafe { (api.open)(&mut handle) };
        if hr < 0 || handle.is_null() {
            return Err(Error::unavailable(format!(
                "PawnIO driver unavailable: {}",
                describe(hr)
            )));
        }

        // SAFETY: handle is valid and the blob is a 'static slice.
        let hr = unsafe { (api.load)(handle, LPCIO_MODULE.as_ptr(), LPCIO_MODULE.len()) };
        if hr < 0 {
            // SAFETY: handle came from pawnio_open and is not yet closed.
            unsafe { (api.close)(handle) };
            return Err(Error::backend(format!(
                "PawnIO rejected the LpcIO module: {}",
                describe(hr)
            )));
        }

        Ok(Self { handle })
    }

    /// Executes a module function. Returns the output words.
    fn execute(&self, name: &str, input: &[u64], out_words: usize) -> Result<Vec<u64>> {
        let api = api()?;
        let mut out = vec![0u64; out_words.max(1)];
        let mut written: usize = 0;
        let c_name: Vec<u8> = name.bytes().chain(std::iter::once(0)).collect();

        // SAFETY: the name is NUL-terminated, `input`/`out` are sized by their
        // slice lengths, and `written` is a valid out-pointer.
        let hr = unsafe {
            (api.execute)(
                self.handle,
                c_name.as_ptr() as *const c_char,
                input.as_ptr(),
                input.len(),
                out.as_mut_ptr(),
                out_words,
                &mut written,
            )
        };

        if hr < 0 {
            return Err(Error::backend(format!(
                "PawnIO `{name}` failed: {}",
                describe(hr)
            )));
        }
        out.truncate(written.min(out.len()));
        Ok(out)
    }
}

impl Drop for PawnIo {
    fn drop(&mut self) {
        if let Ok(api) = api() {
            // SAFETY: the handle was produced by pawnio_open and is closed once.
            unsafe { (api.close)(self.handle) };
        }
    }
}

// NOTE: the LpcIO module documents that callers should hold
// `\BaseNamedObjects\Access_ISABUS.HTP.Method` before touching the chip, so
// HWiNFO and friends do not interleave index writes. That mutex is
// thread-affine, and this backend can be dropped on a different thread than it
// was created on, so a guard here would release incorrectly. Serialising
// against *other* monitoring software is therefore not implemented; PawnIO's
// driver still serialises our own IOCTLs.
//
/// The `LpcIO` module: port access bounded to one Super I/O slot.
pub struct LpcIo {
    io: PawnIo,
    slot: u8,
}

impl LpcIo {
    /// Binds to the Super I/O at `0x2E` (slot 0) or `0x4E` (slot 1).
    pub fn open(slot: u8) -> Result<Self> {
        if slot > 1 {
            return Err(Error::backend("LpcIO slot must be 0 (0x2E) or 1 (0x4E)"));
        }
        let io = PawnIo::open()?;
        io.execute("ioctl_select_slot", &[slot as u64], 0)?;
        Ok(Self { io, slot })
    }

    fn index_port(&self) -> u16 {
        if self.slot == 0 { 0x2E } else { 0x4E }
    }

    /// Enters configuration mode (`0x87` twice) and returns the chip ID.
    pub fn unlock_and_identify(&self) -> Result<u16> {
        let port = self.index_port() as u64;
        self.io.execute("ioctl_pio_outb", &[port, 0x87], 0)?;
        self.io.execute("ioctl_pio_outb", &[port, 0x87], 0)?;
        let high = self.superio_inb(0x20)?;
        let low = self.superio_inb(0x21)?;
        Ok(u16::from(high) << 8 | u16::from(low))
    }

    /// Leaves configuration mode.
    pub fn lock(&self) -> Result<()> {
        let port = self.index_port() as u64;
        self.io.execute("ioctl_pio_outb", &[port, 0xAA], 0)?;
        Ok(())
    }

    /// Discovers the BAR windows; must be called while config mode is active.
    pub fn find_bars(&self) -> Result<()> {
        self.io.execute("ioctl_find_bars", &[], 0)?;
        Ok(())
    }

    pub fn superio_inb(&self, register: u8) -> Result<u8> {
        let out = self
            .io
            .execute("ioctl_superio_inb", &[register as u64], 1)?;
        Ok(out.first().copied().unwrap_or(0) as u8)
    }

    pub fn superio_inw(&self, register: u8) -> Result<u16> {
        let out = self
            .io
            .execute("ioctl_superio_inw", &[register as u64], 1)?;
        Ok(out.first().copied().unwrap_or(0) as u16)
    }

    pub fn superio_outb(&self, register: u8, value: u8) -> Result<()> {
        self.io
            .execute("ioctl_superio_outb", &[register as u64, value as u64], 0)?;
        Ok(())
    }

    pub fn pio_inb(&self, port: u16) -> Result<u8> {
        let out = self.io.execute("ioctl_pio_inb", &[port as u64], 1)?;
        Ok(out.first().copied().unwrap_or(0) as u8)
    }

    pub fn pio_outb(&self, port: u16, value: u8) -> Result<()> {
        self.io
            .execute("ioctl_pio_outb", &[port as u64, value as u64], 0)?;
        Ok(())
    }
}

/// Whether PawnIO is installed, without opening the driver.
pub fn installed() -> bool {
    api().is_ok()
}

/// Library version as reported by `pawnio_version` (e.g. 131072 = 2.0).
pub fn version() -> Option<u32> {
    let api = api().ok()?;
    let mut value = 0u32;
    // SAFETY: out-pointer to a live u32.
    let hr = unsafe { (api.version)(&mut value) };
    (hr >= 0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_blob_is_embedded() {
        assert!(LPCIO_MODULE.len() > 1024, "LpcIO.bin looks truncated");
    }

    #[test]
    fn denial_is_reported_as_an_elevation_problem() {
        // Unelevated callers get E_ACCESSDENIED; the message must say so rather
        // than surfacing a bare HRESULT.
        let text = describe(E_ACCESSDENIED as i32);
        assert!(text.contains("administrator"), "unhelpful: {text}");
    }

    #[test]
    fn win32_errors_are_translated() {
        // ERROR_FILE_NOT_FOUND wrapped as an HRESULT
        let text = describe(0x8007_0002u32 as i32);
        assert!(text.contains("0x80070002"), "unhelpful: {text}");
    }

    #[test]
    fn open_reports_installation_or_elevation_clearly() {
        match PawnIo::open() {
            Ok(_) => { /* elevated CI or dev box: nothing to assert */ }
            Err(error) => {
                let message = error.to_string().to_lowercase();
                assert!(
                    message.contains("administrator") || message.contains("not installed"),
                    "ambiguous failure: {message}"
                );
            }
        }
    }
}
