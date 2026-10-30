//! Starting with the OS.
//!
//! Windows: a Task Scheduler logon task with `RunLevel Highest`. It has to be a
//! task rather than a `Run` registry value, because those launch unelevated and
//! would start the app without the administrator rights the fan backend needs —
//! the user would get a UAC prompt at every sign-in instead, or a useless app.
//!
//! This is *not* the vendor pattern criticised in `platform.rs` and the README:
//! that one registers a hidden task during installation and rewrites shortcuts so
//! elevation happens without consent. Here the task is only created when the user
//! flips the toggle, it is named plainly in Task Scheduler, and turning the
//! toggle off deletes it.
//!
//! Linux: an XDG autostart entry under `$XDG_CONFIG_HOME/autostart`.
//! macOS: not implemented; the state reports unsupported instead of pretending.

use std::path::PathBuf;

use serde::Serialize;

use crate::error::{Error, Result};

/// Name of the scheduled task / desktop entry, kept stable so toggling off finds
/// what toggling on created.
pub const ID: &str = "OpenColorHub";

#[derive(Debug, Clone, Serialize)]
pub struct Autostart {
    /// False on platforms with no implementation.
    pub supported: bool,
    /// Whether the OS currently has an entry for this user.
    pub enabled: bool,
    /// `logon-task`, `xdg-autostart` or `none`.
    pub mechanism: String,
    /// Shown verbatim in the UI: where the entry lives and what it does.
    pub detail: String,
}

impl Autostart {
    #[cfg(not(any(windows, target_os = "linux")))]
    fn unsupported(detail: &str) -> Self {
        Self {
            supported: false,
            enabled: false,
            mechanism: "none".into(),
            detail: detail.into(),
        }
    }
}

fn exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(Error::Io)
}

/// Current state, without side effects.
pub fn status() -> Autostart {
    imp::status()
}

/// Registers or removes the OS entry. Idempotent in both directions.
pub fn set_enabled(enabled: bool) -> Result<Autostart> {
    imp::set_enabled(enabled)?;
    Ok(imp::status())
}

// ---------------------------------------------------------------- Windows --

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    use super::{Autostart, Error, ID, exe};
    use crate::error::Result;

    /// `CREATE_NO_WINDOW`: schtasks.exe must not flash a console at the user.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn schtasks(args: &[&str]) -> std::io::Result<std::process::Output> {
        Command::new("schtasks")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    }

    pub fn status() -> Autostart {
        let exists = schtasks(&["/Query", "/TN", ID])
            .map(|out| out.status.success())
            .unwrap_or(false);
        Autostart {
            supported: true,
            enabled: exists,
            mechanism: "logon-task".into(),
            detail: if exists {
                format!(
                    "Runs at sign-in with administrator rights, through the Task Scheduler \
                     task \"{ID}\". Delete that task to undo it."
                )
            } else {
                "Not registered — the app only runs when you open it.".into()
            },
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<()> {
        if enabled {
            let exe = exe()?;
            let quoted = format!("\"{}\"", exe.display());
            let out = schtasks(&[
                "/Create", "/F", "/SC", "ONLOGON", "/TN", ID, "/TR", &quoted, "/RL", "HIGHEST",
            ])
            .map_err(Error::Io)?;
            if !out.status.success() {
                return Err(Error::unavailable(format!(
                    "Task Scheduler refused: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
        } else {
            // Removing something that is not there is success, not failure.
            let _ = schtasks(&["/Delete", "/F", "/TN", ID]);
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ Linux --

#[cfg(target_os = "linux")]
mod imp {
    use std::fs;
    use std::path::PathBuf;

    use super::{Autostart, Error, ID, exe};
    use crate::error::Result;

    fn entry_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        base.join("autostart")
            .join(format!("{}.desktop", ID.to_ascii_lowercase()))
    }

    pub fn status() -> Autostart {
        let path = entry_path();
        let exists = path.is_file();
        Autostart {
            supported: true,
            enabled: exists,
            mechanism: "xdg-autostart".into(),
            detail: if exists {
                format!("Runs at sign-in through {}.", path.display())
            } else {
                "Not registered — the app only runs when you open it.".into()
            },
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<()> {
        let path = entry_path();
        if enabled {
            let exe = exe()?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(Error::Io)?;
            }
            let entry = format!(
                "[Desktop Entry]\n\
                 Type=Application\n\
                 Name={ID}\n\
                 Comment=Fan and ARGB control for Colorful motherboards\n\
                 Exec=\"{}\"\n\
                 Terminal=false\n\
                 X-GNOME-Autostart-enabled=true\n",
                exe.display()
            );
            fs::write(&path, entry).map_err(Error::Io)?;
        } else {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::Io(e)),
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------- everything --

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::Autostart;

    pub fn status() -> Autostart {
        Autostart::unsupported("Starting with the OS is only implemented on Windows and Linux.")
    }

    pub fn set_enabled(_enabled: bool) -> super::Result<()> {
        Err(super::Error::unavailable(
            "Starting with the OS is only implemented on Windows and Linux.",
        ))
    }
}
