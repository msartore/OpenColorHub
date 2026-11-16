//! OS power notifications, so a resume does not have to be guessed from a
//! wall-clock gap alone.
//!
//! The watcher is deliberately tiny: an OS registration sets one atomic flag
//! when the machine resumes, and the telemetry loop checks that flag on its
//! next tick and re-applies the stored setup. No hardware write, allocation or
//! blocking happens in a power callback — those run in restrictive contexts.
//!
//! Windows uses `RegisterSuspendResumeNotification` with a callback, which
//! needs neither a window handle nor a message pump. Linux listens for logind's
//! `PrepareForSleep(false)` signal on the system bus with a small D-Bus client
//! built on `libc` (no new dependency). If neither platform service is
//! available, the watcher is inert and the existing wall-clock and device
//! checks in [`crate::power::ResumeWatch`] remain the fallback.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A registration that flips `resume` when the OS reports a wake.
///
/// Cloneable: the telemetry task and the controller can share one watcher.
#[derive(Clone)]
pub struct PowerWatcher {
    resume: Arc<AtomicBool>,
    _platform: Option<Arc<PlatformWatcher>>,
}

impl std::fmt::Debug for PowerWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PowerWatcher")
            .field("active", &self._platform.is_some())
            .finish()
    }
}

impl Default for PowerWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerWatcher {
    pub fn new() -> Self {
        let resume = Arc::new(AtomicBool::new(false));
        let platform = PlatformWatcher::start(resume.clone()).map(Arc::new);
        Self {
            resume,
            _platform: platform,
        }
    }

    /// True once since the last call. The telemetry loop calls this on every
    /// tick; a resume is handled exactly once.
    pub fn take_resume(&self) -> bool {
        self.resume.swap(false, Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// Windows: RegisterSuspendResumeNotification with DEVICE_NOTIFY_CALLBACK.
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Power::{
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, RegisterSuspendResumeNotification,
        UnregisterSuspendResumeNotification,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMECRITICAL,
        PBT_APMRESUMESTANDBY, PBT_APMRESUMESUSPEND,
    };

    pub(super) struct PlatformWatcher {
        handle: HPOWERNOTIFY,
        /// Keeps the context pointer the callback holds alive.
        _flag: Arc<AtomicBool>,
        _params: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
    }

    unsafe extern "system" fn on_power(
        context: *const c_void,
        event: u32,
        _setting: *const c_void,
    ) -> u32 {
        if matches!(
            event,
            PBT_APMRESUMESUSPEND
                | PBT_APMRESUMEAUTOMATIC
                | PBT_APMRESUMECRITICAL
                | PBT_APMRESUMESTANDBY
        ) && !context.is_null()
        {
            // SAFETY: the pointer is `Arc::as_ptr(&flag)` of a watcher that is
            // still alive — the registration is unregistered in Drop first.
            unsafe { (*(context as *const AtomicBool)).store(true, Ordering::SeqCst) };
        }
        0
    }

    impl PlatformWatcher {
        pub(super) fn start(flag: Arc<AtomicBool>) -> Option<Self> {
            let mut params = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
                Callback: Some(on_power),
                Context: Arc::as_ptr(&flag) as *mut c_void,
            });
            // SAFETY: `params` is boxed and outlives the registration; with
            // DEVICE_NOTIFY_CALLBACK the recipient is that struct's address.
            let handle = unsafe {
                RegisterSuspendResumeNotification(
                    &mut *params as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS as HANDLE,
                    DEVICE_NOTIFY_CALLBACK,
                )
            };
            (handle != 0).then_some(Self {
                handle,
                _flag: flag,
                _params: params,
            })
        }
    }

    impl Drop for PlatformWatcher {
        fn drop(&mut self) {
            // SAFETY: `handle` came from a successful registration above.
            unsafe {
                UnregisterSuspendResumeNotification(self.handle);
            }
        }
    }
}

#[cfg(target_os = "windows")]
use platform::PlatformWatcher;

// ---------------------------------------------------------------------------
// Linux: logind's PrepareForSleep signal over the system D-Bus.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod platform {
    use std::ffi::{c_int, c_void};
    use std::mem::size_of;
    use std::os::fd::RawFd;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::{self, JoinHandle};

    use libc::{
        AF_UNIX, EAGAIN, EINTR, SO_RCVTIMEO, SOCK_CLOEXEC, SOCK_STREAM, SOL_SOCKET, close, connect,
        getuid, recv, setsockopt, socket, timeval, write,
    };

    const MATCH_RULE: &str =
        "type='signal',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'";

    pub(super) struct PlatformWatcher {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl PlatformWatcher {
        pub(super) fn start(flag: Arc<AtomicBool>) -> Option<Self> {
            let fd = connect_bus()?;
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = stop.clone();
            let thread = thread::spawn(move || listen(fd, thread_stop, flag));
            Some(Self {
                stop,
                thread: Some(thread),
            })
        }
    }

    impl Drop for PlatformWatcher {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn bus_path() -> Option<String> {
        if let Ok(address) = std::env::var("DBUS_SYSTEM_BUS_ADDRESS") {
            if let Some(path) = address.strip_prefix("unix:path=") {
                return Some(path.split(',').next()?.to_string());
            }
        }
        for path in [
            "/run/dbus/system_bus_socket",
            "/var/run/dbus/system_bus_socket",
        ] {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
        None
    }

    fn connect_bus() -> Option<RawFd> {
        let path = bus_path()?;
        let cpath = std::ffi::CString::new(path).ok()?;
        // SAFETY: a fresh Unix stream socket; close it on any failure below.
        let fd = unsafe { socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0) };
        if fd < 0 {
            return None;
        }
        let address = unsafe {
            libc::sockaddr_un {
                sun_family: AF_UNIX as u16,
                sun_path: {
                    let mut bytes = [0i8; 108];
                    let raw = cpath.as_bytes();
                    if raw.len() >= bytes.len() {
                        close(fd);
                        return None;
                    }
                    for (slot, byte) in bytes.iter_mut().zip(raw.iter()) {
                        *slot = *byte as i8;
                    }
                    bytes
                },
            }
        };
        let ok = unsafe {
            connect(
                fd,
                &address as *const libc::sockaddr_un as *const libc::sockaddr,
                size_of::<libc::sockaddr_un>() as u32,
            )
        };
        if ok != 0 {
            unsafe { close(fd) };
            return None;
        }
        // A one-second receive timeout lets Drop stop the thread promptly.
        let timeout = timeval {
            tv_sec: 1,
            tv_usec: 0,
        };
        unsafe {
            setsockopt(
                fd,
                SOL_SOCKET,
                SO_RCVTIMEO,
                &timeout as *const timeval as *const c_void,
                size_of::<timeval>() as u32,
            );
        }
        if !authenticate(fd) {
            unsafe { close(fd) };
            return None;
        }
        Some(fd)
    }

    fn authenticate(fd: RawFd) -> bool {
        let uid = unsafe { getuid() }.to_string();
        let mut hex = String::new();
        for byte in uid.as_bytes() {
            hex.push_str(&format!("{byte:02x}"));
        }
        if write_line(fd, &format!("AUTH EXTERNAL {hex}\r\n"))
            && read_line(fd).is_some_and(|line| line.starts_with("OK"))
            && write_line(fd, "BEGIN\r\n")
        {
            return true;
        }
        // Some buses allow anonymous local clients; try that before giving up.
        write_line(fd, "AUTH ANONYMOUS\r\n")
            && read_line(fd).is_some_and(|line| line.starts_with("OK"))
            && write_line(fd, "BEGIN\r\n")
    }

    fn listen(fd: RawFd, stop: Arc<AtomicBool>, flag: Arc<AtomicBool>) {
        // Hello (serial 1) then the match rule (serial 2). Both method returns
        // are consumed before the signal loop starts.
        let hello = method_call(
            1,
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            "org.freedesktop.DBus",
            &[],
        );
        if write_all(fd, &hello).is_none() {
            unsafe { close(fd) };
            return;
        }
        let _ = wait_for_reply(fd, &stop, 1);

        let add_match = method_call(
            2,
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
            "org.freedesktop.DBus",
            &[MATCH_RULE],
        );
        if write_all(fd, &add_match).is_none() {
            unsafe { close(fd) };
            return;
        }
        let _ = wait_for_reply(fd, &stop, 2);

        while !stop.load(Ordering::SeqCst) {
            let Some(message) = read_message(fd, &stop) else {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                continue;
            };
            if is_resume_signal(&message).unwrap_or(false) {
                flag.store(true, Ordering::SeqCst);
            }
        }
        unsafe { close(fd) };
    }

    fn wait_for_reply(fd: RawFd, stop: &AtomicBool, serial: u32) -> Option<()> {
        for _ in 0..8 {
            if stop.load(Ordering::SeqCst) {
                return None;
            }
            let message = read_message(fd, stop)?;
            if message.len() >= 16 && u32::from_ne_bytes(message[12..16].try_into().ok()?) == serial
            {
                return Some(());
            }
        }
        None
    }

    // --- D-Bus wire format ---------------------------------------------------

    fn align_up(value: usize, alignment: usize) -> usize {
        (value + alignment - 1) & !(alignment - 1)
    }

    fn alignment(signature: &[u8]) -> usize {
        match signature {
            [b's'] | [b'o'] | [b'u'] | [b'i'] | [b'b'] => 4,
            _ => 1,
        }
    }

    fn pad(buffer: &mut Vec<u8>, alignment: usize) {
        while buffer.len() % alignment != 0 {
            buffer.push(0);
        }
    }

    fn put_string(buffer: &mut Vec<u8>, value: &str) {
        buffer.extend_from_slice(&(value.len() as u32).to_ne_bytes());
        buffer.extend_from_slice(value.as_bytes());
        buffer.push(0);
    }

    pub(super) fn string_value(value: &str) -> Vec<u8> {
        let mut out = Vec::new();
        put_string(&mut out, value);
        out
    }

    pub(super) fn add_field(fields: &mut Vec<u8>, code: u8, signature: &[u8], value: &[u8]) {
        fields.push(code);
        fields.push(signature.len() as u8);
        fields.extend_from_slice(signature);
        fields.push(0);
        pad(fields, alignment(signature));
        fields.extend_from_slice(value);
        pad(fields, 8);
    }

    fn method_call(
        serial: u32,
        path: &str,
        interface: &str,
        member: &str,
        destination: &str,
        arguments: &[&str],
    ) -> Vec<u8> {
        let mut body = Vec::new();
        for argument in arguments {
            pad(&mut body, 4);
            put_string(&mut body, argument);
        }
        let signature = "s".repeat(arguments.len());

        let mut fields = Vec::new();
        add_field(&mut fields, 1, b"o", &string_value(path));
        add_field(&mut fields, 2, b"s", &string_value(interface));
        add_field(&mut fields, 3, b"s", &string_value(member));
        add_field(&mut fields, 6, b"s", &string_value(destination));
        if !signature.is_empty() {
            let mut sig_value = vec![signature.len() as u8];
            sig_value.extend_from_slice(signature.as_bytes());
            sig_value.push(0);
            add_field(&mut fields, 8, b"g", &sig_value);
        }

        let mut message = vec![0u8; 16];
        message.extend_from_slice(&(fields.len() as u32).to_ne_bytes());
        pad(&mut message, 8);
        message.extend_from_slice(&fields);
        pad(&mut message, 8);
        message.extend_from_slice(&body);

        message[0] = b'l'; // little endian
        message[1] = 1; // method call
        message[2] = 0; // flags
        message[3] = 1; // protocol version
        message[8..12].copy_from_slice(&(body.len() as u32).to_ne_bytes());
        message[12..16].copy_from_slice(&serial.to_ne_bytes());
        message
    }

    fn read_exact(fd: RawFd, length: usize, stop: &AtomicBool) -> Option<Vec<u8>> {
        let mut out = vec![0u8; length];
        let mut filled = 0;
        while filled < length && !stop.load(Ordering::SeqCst) {
            // SAFETY: `out` has room for `length - filled` bytes.
            let count = unsafe {
                recv(
                    fd,
                    out.as_mut_ptr().add(filled) as *mut c_void,
                    length - filled,
                    0,
                )
            };
            if count > 0 {
                filled += count as usize;
            } else if count == 0 || (count < 0 && last_error() != EINTR && last_error() != EAGAIN) {
                return None;
            }
        }
        (filled == length).then_some(out)
    }

    fn read_message(fd: RawFd, stop: &AtomicBool) -> Option<Vec<u8>> {
        // 16-byte fixed header plus the 4-byte header-field array length.
        let mut message = read_exact(fd, 20, stop)?;
        if message[0] != b'l' {
            return None;
        }
        let body_len = u32::from_ne_bytes(message[8..12].try_into().ok()?) as usize;
        let fields_len = u32::from_ne_bytes(message[16..20].try_into().ok()?) as usize;
        let total = align_up(align_up(20, 8) + fields_len, 8) + body_len;
        if total < 20 || total > 1 << 20 {
            return None;
        }
        message.extend_from_slice(&read_exact(fd, total - 20, stop)?);
        Some(message)
    }

    pub(super) fn is_resume_signal(message: &[u8]) -> Option<bool> {
        if message.len() < 16 || message[1] != 4 {
            return None; // 4 = signal
        }
        let fields_len = match message.get(16..20) {
            Some(bytes) => u32::from_ne_bytes(bytes.try_into().ok()?) as usize,
            None => return None,
        };
        let end = 24usize.saturating_add(fields_len).min(message.len());
        let mut cursor = 24usize;
        let mut member = None;
        while cursor < end {
            cursor = align_up(cursor, 8);
            if cursor + 2 > end {
                break;
            }
            let code = message[cursor];
            cursor += 1;
            let signature_len = message[cursor] as usize;
            cursor += 1;
            let signature = message.get(cursor..cursor + signature_len)?;
            cursor += signature_len + 1; // trailing NUL
            cursor = align_up(cursor, alignment(signature));
            if signature == b"s" || signature == b"o" {
                let length =
                    u32::from_ne_bytes(message.get(cursor..cursor + 4)?.try_into().ok()?) as usize;
                cursor += 4;
                let value = message.get(cursor..cursor + length)?;
                cursor += length + 1;
                if code == 3 {
                    member = Some(std::str::from_utf8(value).ok()?);
                }
            } else {
                break;
            }
        }
        if member != Some("PrepareForSleep") {
            return None;
        }
        let body_start = align_up(end, 8);
        let bytes = message.get(body_start..body_start + 4)?;
        // false = the machine is back, true = about to sleep.
        Some(u32::from_ne_bytes(bytes.try_into().ok()?) == 0)
    }

    fn read_line(fd: RawFd) -> Option<String> {
        let mut out = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            // SAFETY: one byte of a live stack buffer.
            let count = unsafe { recv(fd, byte.as_mut_ptr() as *mut c_void, 1, 0) };
            if count > 0 {
                out.push(byte[0]);
                if byte[0] == b'\n' {
                    return String::from_utf8(out).ok();
                }
            } else if count == 0 || (count < 0 && last_error() != EINTR && last_error() != EAGAIN) {
                return None;
            }
        }
    }

    fn write_line(fd: RawFd, line: &str) -> bool {
        write_all(fd, line.as_bytes()).is_some()
    }

    fn write_all(fd: RawFd, bytes: &[u8]) -> Option<()> {
        let mut sent = 0;
        while sent < bytes.len() {
            // SAFETY: `bytes` stays alive for the duration of the call.
            let count = unsafe {
                write(
                    fd,
                    bytes.as_ptr().add(sent) as *const c_void,
                    bytes.len() - sent,
                )
            };
            if count > 0 {
                sent += count as usize;
            } else if count < 0 && last_error() == EINTR {
                continue;
            } else {
                return None;
            }
        }
        Some(())
    }

    fn last_error() -> c_int {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }
}

#[cfg(target_os = "linux")]
use platform::PlatformWatcher;

// ---------------------------------------------------------------------------
// Other platforms: the wall-clock and device checks remain the whole story.
// ---------------------------------------------------------------------------

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
struct PlatformWatcher;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
impl PlatformWatcher {
    fn start(_flag: Arc<AtomicBool>) -> Option<Self> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_watcher_has_no_pending_resume() {
        let watcher = PowerWatcher::new();
        // Whether the OS registration succeeded is platform-specific; either
        // way a fresh watcher must not claim a resume.
        assert!(!watcher.take_resume());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_prepare_for_sleep_false_message_is_a_resume() {
        // Build a real signal-shaped message by hand: body is one boolean.
        let mut signal = vec![0u8; 16];
        signal[0] = b'l';
        signal[1] = 4;
        signal[3] = 1;
        let mut fields = Vec::new();
        platform::add_field(
            &mut fields,
            3,
            b"s",
            &platform::string_value("PrepareForSleep"),
        );
        signal.extend_from_slice(&(fields.len() as u32).to_ne_bytes());
        while signal.len() % 8 != 0 {
            signal.push(0);
        }
        signal.extend_from_slice(&fields);
        while signal.len() % 8 != 0 {
            signal.push(0);
        }
        signal.extend_from_slice(&0u32.to_ne_bytes());
        signal[8..12].copy_from_slice(&4u32.to_ne_bytes());
        assert_eq!(platform::is_resume_signal(&signal), Some(true));

        let last = signal.len() - 4;
        signal[last..].copy_from_slice(&1u32.to_ne_bytes());
        assert_eq!(platform::is_resume_signal(&signal), Some(false));
    }
}
