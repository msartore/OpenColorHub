//! Privilege state, and an explicit request for elevation.
//!
//! Deliberately **not** the pattern vendor utilities use: a hidden scheduled task
//! with `RunLevel::Highest` grants administrator with no consent prompt and keeps
//! doing so for every launch afterwards. Here the user sees a single UAC prompt,
//! can refuse it, and the result is reported back either way.
//!
//! Windows raises the prompt through `ShellExecuteEx(..., "runas")`, which is the
//! documented way for a process to relaunch itself elevated. The calling instance
//! is expected to exit once the elevated one has started.

use serde::Serialize;

/// What the UI needs to decide whether to offer the action.
#[derive(Debug, Clone, Serialize)]
pub struct Elevation {
    /// True when the current process already holds an elevated token (or euid 0).
    pub elevated: bool,
    /// True when this platform/build can actually raise a prompt.
    pub can_request: bool,
    /// Human-readable reason, shown verbatim when the action is unavailable.
    pub detail: String,
}

/// Result of asking the OS for elevation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The elevated instance was started; the caller should exit.
    Launched,
    /// The user dismissed the prompt.
    Declined,
    /// This platform cannot raise an elevation prompt.
    Unsupported,
}

/// Current privilege state, without side effects.
pub fn state() -> Elevation {
    let elevated = is_elevated();
    if elevated {
        return Elevation {
            elevated: true,
            can_request: false,
            detail: imp::ELEVATED_DETAIL.to_string(),
        };
    }
    Elevation {
        elevated: false,
        can_request: imp::CAN_REQUEST,
        detail: imp::UNELEVATED_DETAIL.to_string(),
    }
}

/// Whether this process already has the privileges it would ask for.
pub fn is_elevated() -> bool {
    imp::is_elevated()
}

/// Raises the platform's consent prompt and relaunches this binary elevated.
pub fn request() -> crate::error::Result<Outcome> {
    imp::request()
}

#[cfg(target_os = "windows")]
mod imp {
    use std::ffi::{OsStr, c_void};
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TokenElevation};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::Outcome;
    use crate::error::{Error, Result};

    /// `ERROR_CANCELLED` — the user dismissed the UAC prompt.
    const ERROR_CANCELLED: u32 = 1223;

    pub(super) const CAN_REQUEST: bool = true;
    pub(super) const ELEVATED_DETAIL: &str = "Running with administrator rights.";
    pub(super) const UNELEVATED_DETAIL: &str = "Running as a standard user. Restarting elevated asks Windows for consent once — the \
         current RGB and HWiNFO paths do not need it, but a privileged task (or a driver-backed \
         fan backend) would.";

    pub(super) fn is_elevated() -> bool {
        // SAFETY: every handle is released before returning, and `elevation` is a
        // live local of exactly the size passed to GetTokenInformation.
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(
                GetCurrentProcess(),
                0x0008, /* TOKEN_QUERY */
                &mut token,
            ) == 0
            {
                return false;
            }
            let mut elevation: TOKEN_ELEVATION = zeroed();
            let mut returned = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut TOKEN_ELEVATION as *mut c_void,
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut returned,
            );
            CloseHandle(token);
            ok != 0 && elevation.TokenIsElevated != 0
        }
    }

    pub(super) fn request() -> Result<Outcome> {
        let exe = std::env::current_exe()
            .map_err(|e| Error::backend(format!("cannot locate the executable: {e}")))?;
        let cwd = std::env::current_dir().ok();

        let parameters = std::env::args_os()
            .skip(1)
            .map(|a| quote(&a))
            .collect::<Vec<_>>()
            .join(" ");

        let verb = wide("runas");
        let file = wide(&exe);
        let params = wide(&parameters);
        let dir = cwd.as_ref().map(wide);

        // SAFETY: all wide buffers outlive the call; the struct is fully
        // initialised apart from the union, which is zeroed.
        unsafe {
            let mut info: SHELLEXECUTEINFOW = zeroed();
            info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
            info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI;
            info.lpVerb = verb.as_ptr();
            info.lpFile = file.as_ptr();
            info.lpParameters = params.as_ptr();
            info.lpDirectory = dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr());
            info.nShow = SW_SHOWNORMAL;

            if ShellExecuteExW(&mut info) == 0 {
                let err = GetLastError();
                if err == ERROR_CANCELLED {
                    return Ok(Outcome::Declined);
                }
                return Err(Error::backend(format!(
                    "elevation request failed: {}",
                    std::io::Error::from_raw_os_error(err as i32)
                )));
            }
            if !info.hProcess.is_null() {
                CloseHandle(info.hProcess);
            }
        }
        Ok(Outcome::Launched)
    }

    fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Quotes one argument for `CommandLineToArgvW` parsing.
    fn quote(arg: &OsStr) -> String {
        let s = arg.to_string_lossy();
        if !s.is_empty() && !s.contains([' ', '\t', '\n', '"']) {
            return s.into_owned();
        }
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        let mut backslashes = 0usize;
        for ch in s.chars() {
            match ch {
                '\\' => backslashes += 1,
                '"' => {
                    // Escape the run of backslashes and the quote itself.
                    for _ in 0..backslashes * 2 + 1 {
                        out.push('\\');
                    }
                    out.push('"');
                    backslashes = 0;
                }
                _ => {
                    for _ in 0..backslashes {
                        out.push('\\');
                    }
                    backslashes = 0;
                    out.push(ch);
                }
            }
        }
        for _ in 0..backslashes * 2 {
            out.push('\\');
        }
        out.push('"');
        out
    }

    #[cfg(test)]
    mod tests {
        use super::quote;
        use std::ffi::OsStr;

        #[test]
        fn plain_arguments_are_not_quoted() {
            assert_eq!(quote(OsStr::new("--debug")), "--debug");
        }

        #[test]
        fn spaces_force_quotes() {
            assert_eq!(quote(OsStr::new("a b")), "\"a b\"");
        }

        #[test]
        fn quotes_are_escaped() {
            assert_eq!(quote(OsStr::new("say \"hi\"")), "\"say \\\"hi\\\"\"");
        }

        #[test]
        fn trailing_backslashes_are_doubled_inside_quotes() {
            assert_eq!(quote(OsStr::new("C:\\a b\\")), "\"C:\\a b\\\\\"");
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Outcome;
    use crate::error::{Error, Result};

    pub(super) const CAN_REQUEST: bool = false;
    pub(super) const ELEVATED_DETAIL: &str = "Running as root.";
    pub(super) const UNELEVATED_DETAIL: &str = "Fan control needs /dev/port, which requires root or CAP_SYS_RAWIO. Restart with sudo/doas, \
         or grant the capability to the binary; a graphical prompt is not implemented here.";

    pub(super) fn is_elevated() -> bool {
        // SAFETY: geteuid has no preconditions and cannot fail.
        unsafe { libc::geteuid() == 0 }
    }

    pub(super) fn request() -> Result<Outcome> {
        Err(Error::unavailable(
            "This build cannot raise a consent prompt on Linux — restart the app with sudo/doas.",
        ))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod imp {
    use super::Outcome;
    use crate::error::{Error, Result};

    pub(super) const CAN_REQUEST: bool = false;
    pub(super) const ELEVATED_DETAIL: &str = "Privileges are not tracked on this platform.";
    pub(super) const UNELEVATED_DETAIL: &str =
        "Hardware access is unavailable on this platform, so elevation would not help.";

    pub(super) fn is_elevated() -> bool {
        false
    }

    pub(super) fn request() -> Result<Outcome> {
        Err(Error::unavailable(
            "Elevation is not implemented on this platform.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_never_claims_both_elevated_and_requestable() {
        let s = state();
        assert!(
            !(s.elevated && s.can_request),
            "an already-elevated process must not offer to elevate again"
        );
        assert!(!s.detail.is_empty(), "the UI needs a reason to display");
    }

    #[test]
    fn detection_is_consistent_between_calls() {
        assert_eq!(is_elevated(), is_elevated());
    }
}

/// PawnIO availability, as the UI needs to see it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PawnioStatus {
    Ready {
        version: String,
    },
    NotInstalled {
        detail: String,
    },
    Denied {
        detail: String,
    },
    Failed {
        detail: String,
    },
    /// Platform where PawnIO does not apply (Linux uses /dev/port).
    Unsupported,
}

pub fn pawnio_status() -> PawnioStatus {
    #[cfg(target_os = "windows")]
    {
        use crate::backend::windows::pawnio;
        match pawnio::status() {
            pawnio::Status::Ready { version } => PawnioStatus::Ready { version },
            pawnio::Status::NotInstalled { detail } => PawnioStatus::NotInstalled { detail },
            pawnio::Status::Denied { detail } => PawnioStatus::Denied { detail },
            pawnio::Status::Failed { detail } => PawnioStatus::Failed { detail },
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        PawnioStatus::Unsupported
    }
}
