//! Backend abstraction.
//!
//! [`FanBackend`] and [`LedBackend`] are the only traits that know how to talk
//! to hardware. Two families exist:
//!
//! * `linux` — the real thing on Linux: Super I/O port I/O through `/dev/port`
//!   and the onboard RGB controller through `/dev/hidraw`;
//! * `windows` — the real thing on Windows for lighting: the RGB controller is a
//!   plain HID device, so it needs no driver. Fan control still cannot work
//!   (raw I/O ports are kernel-only), so that half stays unavailable;
//! * [`sim`] — a deterministic-ish simulator available on every platform. It is
//!   used for development, CI, and any host where hardware cannot be reached,
//!   and it is always labelled as simulated in the UI.
//!
//! Both hardware families build their frames from [`frame`], so the wire format
//! cannot drift between platforms.

use std::path::{Path, PathBuf};

use crate::board::BoardInfo;
use crate::curve::FanCurve;
use crate::error::Error;
use crate::model::{Capabilities, DeviceInfo, FanHeader, Snapshot, Telemetry};
use crate::settings::BackendMode;
use crate::state::RgbState;

pub mod frame;
pub mod sim;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

/// Fan + thermal backend.
pub trait FanBackend: Send {
    /// Identification for the UI.
    fn info(&self) -> DeviceInfo;

    /// Reads temperatures, tachometers and programmed duty.
    fn read(&mut self) -> std::result::Result<Telemetry, Error>;

    /// Programs a manual duty cycle.
    fn set_pwm(&mut self, header: FanHeader, pwm: u8) -> std::result::Result<(), Error>;

    /// Programs a 4-point SmartFan curve.
    fn set_curve(&mut self, header: FanHeader, curve: &FanCurve) -> std::result::Result<(), Error>;

    /// Whether curves can be programmed (false for read-only backends).
    fn supports_curves(&self) -> bool {
        true
    }
}

/// Lighting backend. The state is mutable in memory; [`LedBackend::apply`]
/// pushes it to the controller and persists it.
pub trait LedBackend: Send {
    fn info(&self) -> DeviceInfo;

    fn state(&self) -> &RgbState;

    fn state_mut(&mut self) -> &mut RgbState;

    /// Writes the full 200-LED frame and saves state.
    fn apply(&mut self) -> std::result::Result<(), Error>;

    /// Where the state blob lives, when known.
    fn state_path(&self) -> Option<PathBuf> {
        None
    }
}

/// A fan backend that exists only to report "this build cannot do that".
pub struct UnsupportedFan {
    info: DeviceInfo,
}

impl UnsupportedFan {
    pub fn new(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            info: DeviceInfo::unavailable(
                "No fan controller",
                "Super I/O fan control needs raw I/O ports, which only Linux exposes",
                reason,
            ),
        }
    }
}

impl FanBackend for UnsupportedFan {
    fn info(&self) -> DeviceInfo {
        self.info.clone()
    }

    fn read(&mut self) -> std::result::Result<Telemetry, Error> {
        Err(Error::unavailable(
            self.info
                .error
                .clone()
                .unwrap_or_else(|| "fan backend unavailable".into()),
        ))
    }

    fn set_pwm(&mut self, _header: FanHeader, _pwm: u8) -> std::result::Result<(), Error> {
        Err(Error::unavailable(
            self.info
                .error
                .clone()
                .unwrap_or_else(|| "fan backend unavailable".into()),
        ))
    }

    fn set_curve(
        &mut self,
        _header: FanHeader,
        _curve: &FanCurve,
    ) -> std::result::Result<(), Error> {
        Err(Error::unavailable(
            self.info
                .error
                .clone()
                .unwrap_or_else(|| "fan backend unavailable".into()),
        ))
    }

    fn supports_curves(&self) -> bool {
        false
    }
}

/// Lighting backend counterpart of [`UnsupportedFan`].
pub struct UnsupportedLed {
    info: DeviceInfo,
    state: RgbState,
    state_path: PathBuf,
}

impl UnsupportedLed {
    pub fn new(reason: impl Into<String>, state_path: PathBuf) -> Self {
        Self {
            info: DeviceInfo::unavailable(
                "No RGB controller",
                "Colorful HID controller (2F4C:1000) could not be opened",
                reason,
            ),
            state: RgbState::default(),
            state_path,
        }
    }
}

impl LedBackend for UnsupportedLed {
    fn info(&self) -> DeviceInfo {
        self.info.clone()
    }

    fn state(&self) -> &RgbState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut RgbState {
        &mut self.state
    }

    fn apply(&mut self) -> std::result::Result<(), Error> {
        Err(Error::unavailable(
            self.info
                .error
                .clone()
                .unwrap_or_else(|| "LED backend unavailable".into()),
        ))
    }

    fn state_path(&self) -> Option<PathBuf> {
        Some(self.state_path.clone())
    }
}

/// The pair of backends currently in use, plus what they can do.
pub struct BackendSet {
    pub fan: Box<dyn FanBackend>,
    pub led: Box<dyn LedBackend>,
    pub capabilities: Capabilities,
}

impl BackendSet {
    /// Only used by hosts with no hardware backend at all, hence the cfg.
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn degraded(notes: Vec<String>) -> Self {
        Self {
            fan: Box::new(UnsupportedFan::new(
                "no usable fan backend for the current settings",
            )),
            led: Box::new(UnsupportedLed::new(
                "no usable LED backend for the current settings",
                PathBuf::from("rgb.state"),
            )),
            capabilities: Capabilities {
                fan_control: false,
                led_control: false,
                notes,
            },
        }
    }
}

/// Instantiates the backend pair for `mode`.
///
/// `board` is the SMBIOS identity detected at startup. On a board that is
/// confirmed to be somebody else's, `Auto` refuses to touch port I/O at all:
/// writing NCT5584D registers on an unrelated Super I/O is exactly the kind of
/// damage this guard exists to prevent.
///
/// * [`BackendMode::Auto`] probes hardware (subject to the board guard) and
///   falls back to the simulator, recording why in `capabilities.notes`.
/// * [`BackendMode::Hardware`] never falls back: unavailable hardware surfaces
///   as a disabled UI with the underlying error.
/// * [`BackendMode::Simulated`] never touches hardware.
pub fn build(
    mode: BackendMode,
    state_file: Option<&Path>,
    data_dir: &Path,
    board: &BoardInfo,
) -> BackendSet {
    match mode {
        BackendMode::Simulated => simulated(state_file, data_dir, None),
        BackendMode::Hardware => hardware_only(state_file, data_dir),
        BackendMode::Auto if board.confidence.is_foreign() => simulated(
            state_file,
            data_dir,
            Some(vec![format!(
                "Hardware probe skipped: {} is not a supported Colorful board (detected via {}). Select the 'hardware only' backend to override.",
                board.display_name(),
                board.source
            )]),
        ),
        BackendMode::Auto => {
            #[cfg(target_os = "linux")]
            {
                let mut notes = Vec::new();
                let led = match open_hardware_led(state_file, data_dir) {
                    Ok(l) => Some(l),
                    Err(e) => {
                        notes.push(format!("LED hardware unavailable: {e}"));
                        None
                    }
                };
                let fan = match open_hardware_fan() {
                    Ok(f) => Some(f),
                    Err(e) => {
                        notes.push(format!("Fan hardware unavailable: {e}"));
                        None
                    }
                };

                if led.is_some() || fan.is_some() {
                    let led = led.unwrap_or_else(|| {
                        Box::new(sim::SimLed::new(state_path(state_file, data_dir)))
                    });
                    let fan = fan.unwrap_or_else(|| Box::new(sim::SimFan::new(data_dir)));
                    let mut caps = Capabilities {
                        fan_control: true,
                        led_control: true,
                        notes,
                    };
                    caps.notes.push(
                        "Missing subsystems fall back to the simulator (marked in the UI)."
                            .to_string(),
                    );
                    if board.confidence == crate::board::BoardConfidence::Compatible {
                        caps.notes.push(format!(
                            "Board {} is a Colorful board outside the verified catalogue.",
                            board.display_name()
                        ));
                    }
                    return BackendSet {
                        fan,
                        led,
                        capabilities: caps,
                    };
                }

                notes.push(
                    "No Colorful hardware detected on this host — using the simulated backend."
                        .to_string(),
                );
                simulated(state_file, data_dir, Some(notes))
            }

            #[cfg(target_os = "windows")]
            {
                let mut notes = Vec::new();

                // Preferred: the Super I/O itself, through PawnIO's sandboxed
                // driver. That is the only path giving real fan speed and real
                // temperature without a vendor utility running alongside.
                let (fan, fan_writable): (Box<dyn FanBackend>, bool) =
                    match windows::fan::PawnIoFan::open() {
                        Ok(f) => {
                            notes.push(
                                "Fan speeds, temperatures and manual duty are driven through \
                                 PawnIO's sandboxed driver. Curves are not written: SmartFan \
                                 registers are unmapped for this chip."
                                    .to_string(),
                            );
                            (Box::new(f), true)
                        }
                        Err(pawn_error) => match windows::hwinfo::HwinfoFan::open() {
                            Ok(f) => {
                                notes.push(format!(
                                    "PawnIO unavailable ({pawn_error}); using HWiNFO shared \
                                     memory instead (read-only)."
                                ));
                                (Box::new(f), false)
                            }
                            Err(hwinfo_error) => {
                                notes.push(format!(
                                    "No fan telemetry: PawnIO ({pawn_error}); HWiNFO \
                                     ({hwinfo_error})"
                                ));
                                (Box::new(sim::SimFan::new(data_dir)), true)
                            }
                        },
                    };

                match open_hardware_led(state_file, data_dir) {
                    Ok(led) => {
                        notes.push("RGB control is live over USB HID.".to_string());
                        if board.confidence == crate::board::BoardConfidence::Compatible {
                            notes.push(format!(
                                "Board {} is a Colorful board outside the verified catalogue.",
                                board.display_name()
                            ));
                        }
                        BackendSet {
                            fan,
                            led,
                            capabilities: Capabilities {
                                fan_control: fan_writable,
                                led_control: true,
                                notes,
                            },
                        }
                    }
                    Err(e) => {
                        notes.push(format!("LED hardware unavailable: {e}"));
                        notes.push(
                            "No Colorful RGB controller detected on this host — using the \
                             simulated backend."
                                .to_string(),
                        );
                        BackendSet {
                            fan,
                            led: Box::new(sim::SimLed::new(state_path(state_file, data_dir))),
                            capabilities: Capabilities {
                                fan_control: fan_writable,
                                led_control: true,
                                notes,
                            },
                        }
                    }
                }
            }

            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            {
                simulated(
                    state_file,
                    data_dir,
                    Some(vec![
                        "Hardware control targets Colorful AMD boards through Linux \
                         (/dev/port + hidraw) or Windows (USB HID for lighting); this build runs \
                         the simulated backend."
                            .to_string(),
                    ]),
                )
            }
        }
    }
}

fn simulated(state_file: Option<&Path>, data_dir: &Path, notes: Option<Vec<String>>) -> BackendSet {
    let path = state_path(state_file, data_dir);
    let mut notes = notes.unwrap_or_default();
    notes.push("Readings on this screen are simulated.".to_string());
    BackendSet {
        fan: Box::new(sim::SimFan::new(data_dir)),
        led: Box::new(sim::SimLed::new(path)),
        capabilities: Capabilities {
            fan_control: true,
            led_control: true,
            notes,
        },
    }
}

fn hardware_only(state_file: Option<&Path>, data_dir: &Path) -> BackendSet {
    #[cfg(target_os = "linux")]
    {
        match (open_hardware_fan(), open_hardware_led(state_file, data_dir)) {
            (Ok(fan), Ok(led)) => BackendSet {
                fan,
                led,
                capabilities: Capabilities::full(),
            },
            (fan, led) => {
                let mut notes = Vec::new();
                let fan: Box<dyn FanBackend> = match fan {
                    Ok(f) => f,
                    Err(e) => {
                        notes.push(format!("Fan hardware unavailable: {e}"));
                        Box::new(UnsupportedFan::new(e.to_string()))
                    }
                };
                let led: Box<dyn LedBackend> = match led {
                    Ok(l) => l,
                    Err(e) => {
                        notes.push(format!("LED hardware unavailable: {e}"));
                        Box::new(UnsupportedLed::new(
                            e.to_string(),
                            state_path(state_file, data_dir),
                        ))
                    }
                };
                BackendSet {
                    fan,
                    led,
                    capabilities: Capabilities {
                        fan_control: notes.iter().all(|n| !n.starts_with("Fan hardware")),
                        led_control: notes.iter().all(|n| !n.starts_with("LED hardware")),
                        notes,
                    },
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut notes = Vec::new();
        let fan: Box<dyn FanBackend> = match windows::hwinfo::HwinfoFan::open() {
            Ok(f) => Box::new(f),
            Err(e) => {
                notes.push(format!("HWiNFO telemetry unavailable: {e}"));
                Box::new(UnsupportedFan::new(NO_WINDOWS_FAN))
            }
        };
        let led: Box<dyn LedBackend> = match open_hardware_led(state_file, data_dir) {
            Ok(l) => l,
            Err(e) => {
                notes.push(format!("LED hardware unavailable: {e}"));
                Box::new(UnsupportedLed::new(
                    e.to_string(),
                    state_path(state_file, data_dir),
                ))
            }
        };
        BackendSet {
            fan,
            led,
            capabilities: Capabilities {
                fan_control: false,
                led_control: notes.iter().all(|n| !n.starts_with("LED hardware")),
                notes,
            },
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = state_file;
        let _ = data_dir;
        BackendSet::degraded(vec![
            "Hardware backend selected, but this build runs on a host with no hardware support."
                .to_string(),
        ])
    }
}

#[cfg(target_os = "linux")]
fn open_hardware_fan() -> std::result::Result<Box<dyn FanBackend>, Error> {
    linux::fan::LinuxFan::open().map(|f| Box::new(f) as Box<dyn FanBackend>)
}

#[cfg(target_os = "linux")]
fn open_hardware_led(
    state_file: Option<&Path>,
    data_dir: &Path,
) -> std::result::Result<Box<dyn LedBackend>, Error> {
    linux::led::LinuxLed::open(state_path(state_file, data_dir))
        .map(|l| Box::new(l) as Box<dyn LedBackend>)
}

#[cfg(target_os = "windows")]
fn open_hardware_led(
    state_file: Option<&Path>,
    data_dir: &Path,
) -> std::result::Result<Box<dyn LedBackend>, Error> {
    windows::led::WindowsLed::open(state_path(state_file, data_dir))
        .map(|l| Box::new(l) as Box<dyn LedBackend>)
}

/// Why fan control cannot exist on Windows, in one place.
#[cfg(target_os = "windows")]
const NO_WINDOWS_FAN: &str = "Windows cannot reach the NCT5584D hardware monitor: raw I/O port \
                              access requires a kernel driver, which opencolorhub does not ship.";

/// State file location honouring `COLORCTL_STATE_FILE`, `COLORCTL_STATE_DIR`,
/// the explicit override from settings, then the app data directory.
pub fn state_path(override_path: Option<&Path>, data_dir: &Path) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    if let Ok(p) = std::env::var("COLORCTL_STATE_FILE") {
        return PathBuf::from(p);
    }
    if let Ok(dir) = std::env::var("COLORCTL_STATE_DIR") {
        return PathBuf::from(dir).join("rgb.state");
    }
    #[cfg(target_os = "linux")]
    {
        let sys = Path::new("/var/lib/colorctl/rgb.state");
        if sys.is_file() {
            return sys.to_path_buf();
        }
    }
    data_dir.join("rgb.state")
}

/// Default per-user data directory when the caller has no better idea.
///
/// On Linux the system path `/var/lib/colorctl` wins when it exists or we are
/// root (matching upstream); everywhere else a conventional user directory is
/// used so the simulator and the CLI work without privileges.
pub fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let sys = Path::new("/var/lib/colorctl");
        let is_root = unsafe { libc::geteuid() == 0 };
        if is_root || sys.is_dir() {
            return sys.to_path_buf();
        }
        if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
            return PathBuf::from(xdg).join("colorctl");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local/state/colorctl");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("opencolorhub");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Application Support/opencolorhub");
        }
    }

    std::env::temp_dir().join("opencolorhub")
}

/// Convenience used by the CLI: read-only snapshot without a UI.
pub fn quick_snapshot(fan: &mut dyn FanBackend) -> Snapshot {
    let mut snap = Snapshot::empty();
    if let Ok(t) = fan.read() {
        snap.temperature_c = t.temperature_c;
    }
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "opencolorhub-backend-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn simulated_mode_never_reports_unavailable() {
        let dir = data_dir("sim");
        let set = build(
            BackendMode::Simulated,
            None,
            &dir,
            &crate::board::demo_board(),
        );
        assert!(set.capabilities.fan_control);
        assert!(set.capabilities.led_control);
        assert!(set.fan.info().simulated);
        assert!(set.led.info().simulated);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn auto_mode_always_yields_working_backends() {
        let dir = data_dir("auto");
        let set = build(BackendMode::Auto, None, &dir, &crate::board::demo_board());
        // On Linux without root/hardware this is simulated; on other hosts
        // always simulated. Either way the UI must have something to show.
        assert!(set.capabilities.fan_control);
        assert!(set.capabilities.led_control);
        assert!(!set.capabilities.notes.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn state_path_prefers_explicit_override() {
        // COLORCTL_STATE_DIR/FILE are a documented override; callers that set
        // them (CI, packaging) legitimately change this behaviour.
        if std::env::var_os("COLORCTL_STATE_FILE").is_some()
            || std::env::var_os("COLORCTL_STATE_DIR").is_some()
        {
            return;
        }
        let dir = data_dir("state");
        let explicit = dir.join("custom.state");
        assert_eq!(state_path(Some(&explicit), &dir), explicit);
        assert_eq!(state_path(None, &dir), dir.join("rgb.state"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
