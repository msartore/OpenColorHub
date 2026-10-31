//! CLI argument definitions.
//!
//! Mirrors upstream `colorctl` so existing scripts keep working, and adds a
//! global `--backend` switch (handy for testing the whole flow off-hardware)
//! plus `--json` for machine-readable output.

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use colorctl_core::FanProfile;
use colorctl_core::curve::{self, FanCurve};
use colorctl_core::model::{Channel, FanHeader};
use colorctl_core::settings::BackendMode;

#[derive(Parser, Debug)]
#[command(
    name = "colorctl",
    version,
    about = "Fan and ARGB control for Colorful motherboards",
    long_about = "colorctl controls the NCT5584D Super I/O (fans, temperatures) and the \
                  onboard Colorful RGB controller (onboard LEDs, 12V and 5V headers) on \
                  supported AMD motherboards. Command-compatible with the original \
                  colorctl, plus --backend and --json. The desktop app is `opencolorhub`."
)]
pub struct Cli {
    /// Backend to use: auto (probe hardware), hardware (require it) or simulated.
    #[arg(long, global = true, value_enum, default_value_t = BackendArg::Auto)]
    pub backend: BackendArg,

    /// Print JSON instead of human-readable text.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendArg {
    Auto,
    Hardware,
    Simulated,
}

impl From<BackendArg> for BackendMode {
    fn from(value: BackendArg) -> Self {
        match value {
            BackendArg::Auto => BackendMode::Auto,
            BackendArg::Hardware => BackendMode::Hardware,
            BackendArg::Simulated => BackendMode::Simulated,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Motherboard RGB lighting control (USB HID 2F4C:1000)
    Rgb(RgbArgs),
    /// Fan and hardware monitor control (Nuvoton Super I/O)
    Fan(FanArgs),
    /// Show the detected motherboard and whether it is supported
    Board,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

// ---------------------------------------------------------------- RGB ----

#[derive(Args, Debug)]
pub struct RgbArgs {
    #[command(subcommand)]
    pub action: RgbAction,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbTarget {
    All,
    #[value(name = "led", alias = "onboard")]
    Led,
    #[value(name = "12v_1", alias = "12v-1", alias = "rgb1", alias = "rgb_1")]
    Rgb1,
    #[value(name = "12v_2", alias = "12v-2", alias = "rgb2", alias = "rgb_2")]
    Rgb2,
    #[value(name = "5v_1", alias = "5v-1", alias = "argb1", alias = "argb_1")]
    Argb1,
    #[value(name = "5v_2", alias = "5v-2", alias = "argb2", alias = "argb_2")]
    Argb2,
    #[value(name = "5v_3", alias = "5v-3", alias = "argb3", alias = "argb_3")]
    Argb3,
}

impl RgbTarget {
    pub fn channels(&self) -> &'static [Channel] {
        match self {
            RgbTarget::All => Channel::all(),
            RgbTarget::Led => &[Channel::Led],
            RgbTarget::Rgb1 => &[Channel::Rgb1],
            RgbTarget::Rgb2 => &[Channel::Rgb2],
            RgbTarget::Argb1 => &[Channel::Argb1],
            RgbTarget::Argb2 => &[Channel::Argb2],
            RgbTarget::Argb3 => &[Channel::Argb3],
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum RgbAction {
    /// Detect the controller and show channel state
    Status,
    /// List every HID interface the controller exposes (read-only)
    Probe,
    /// Set a colour on one or more channels
    Set {
        /// Colour: hex ('ff0000', '#00ff88') or name ('red', 'cyan', 'off', ...)
        #[arg(short = 'C', long)]
        color: String,

        /// Target channel(s); repeatable and comma separated
        #[arg(
            short = 'c',
            long,
            value_enum,
            value_delimiter = ',',
            default_value = "all"
        )]
        channel: Vec<RgbTarget>,

        /// Brightness percentage (0-100)
        #[arg(short = 'b', long, default_value_t = 100, value_parser = clap::value_parser!(u8).range(0..=100))]
        brightness: u8,
    },
    /// Turn lighting off
    Off {
        /// Target channel(s); repeatable and comma separated
        #[arg(
            short = 'c',
            long,
            value_enum,
            value_delimiter = ',',
            default_value = "all"
        )]
        channel: Vec<RgbTarget>,
    },
}

// ---------------------------------------------------------------- FAN ----

#[derive(Args, Debug)]
pub struct FanArgs {
    #[command(subcommand)]
    pub action: FanAction,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FanTarget {
    All,
    #[value(name = "cpu", alias = "cpu_fan", alias = "cpu-fan")]
    Cpu,
    #[value(name = "cha_fan1", alias = "cha-fan1")]
    ChaFan1,
    #[value(name = "cha_fan2", alias = "cha-fan2")]
    ChaFan2,
    #[value(name = "cha_fan3", alias = "cha-fan3")]
    ChaFan3,
    #[value(name = "pump", alias = "aio_pump", alias = "aio-pump")]
    Pump,
}

impl FanTarget {
    pub fn headers(&self) -> &'static [FanHeader] {
        match self {
            FanTarget::All => FanHeader::all(),
            FanTarget::Cpu => &[FanHeader::Cpu],
            FanTarget::ChaFan1 => &[FanHeader::ChaFan1],
            FanTarget::ChaFan2 => &[FanHeader::ChaFan2],
            FanTarget::ChaFan3 => &[FanHeader::ChaFan3],
            FanTarget::Pump => &[FanHeader::Pump],
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileArg {
    /// 20°C:20%, 40°C:30%, 60°C:50%, 80°C:85%
    Quiet,
    /// 20°C:20%, 40°C:40%, 60°C:60%, 80°C:100%
    Standard,
    /// 100% everywhere
    Full,
}

impl From<ProfileArg> for FanProfile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::Quiet => FanProfile::Quiet,
            ProfileArg::Standard => FanProfile::Standard,
            ProfileArg::Full => FanProfile::Full,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum FanAction {
    /// Show temperatures, fan speeds, modes and active curves
    Status,
    /// Dump every Super I/O register this app depends on (read-only)
    Probe,
    /// Set a manual fan speed (percent or raw PWM)
    SetSpeed {
        /// Target header
        #[arg(short = 'f', long, value_enum, default_value_t = FanTarget::All)]
        fan: FanTarget,

        /// Speed percentage (0-100)
        #[arg(short = 'p', long, value_parser = clap::value_parser!(u8).range(0..=100), conflicts_with = "pwm")]
        percent: Option<u8>,

        /// Raw PWM duty (0-255)
        #[arg(long, conflicts_with = "percent")]
        pwm: Option<u8>,
    },
    /// Program a 4-point SmartFan curve or apply a built-in profile
    SetCurve {
        /// Target header
        #[arg(short = 'f', long, value_enum, default_value_t = FanTarget::All)]
        fan: FanTarget,

        /// Points as 'T1:P1,T2:P2,T3:P3,T4:P4' (e.g. '30:25,50:45,70:75,85:100')
        #[arg(short = 'p', long, value_parser = parse_curve_arg, conflicts_with = "profile")]
        points: Option<FanCurve>,

        /// Built-in profile
        #[arg(short = 'P', long, value_enum, conflicts_with = "points")]
        profile: Option<ProfileArg>,
    },
}

fn parse_curve_arg(raw: &str) -> Result<FanCurve, String> {
    curve::parse(raw).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_upstream_shapes() {
        let cli = Cli::try_parse_from([
            "colorctl",
            "rgb",
            "set",
            "--color",
            "#00ff88",
            "--brightness",
            "80",
        ])
        .expect("rgb set");
        assert!(!cli.json);
        match cli.command {
            Command::Rgb(args) => match args.action {
                RgbAction::Set { brightness, .. } => assert_eq!(brightness, 80),
                _ => panic!("expected set"),
            },
            _ => panic!("expected rgb"),
        }

        let cli = Cli::try_parse_from([
            "colorctl",
            "fan",
            "set-curve",
            "--fan",
            "cpu",
            "--points",
            "30:25,50:45,70:75,85:100",
        ])
        .expect("fan set-curve");
        match cli.command {
            Command::Fan(args) => match args.action {
                FanAction::SetCurve { points, .. } => {
                    assert_eq!(points.unwrap()[0], colorctl_core::CurvePoint::new(30, 25));
                }
                _ => panic!("expected set-curve"),
            },
            _ => panic!("expected fan"),
        }
    }

    #[test]
    fn board_subcommand_parses() {
        let cli = Cli::try_parse_from(["colorctl", "board", "--json"]).expect("board");
        assert!(cli.json);
        assert!(matches!(cli.command, Command::Board));
    }

    #[test]
    fn rejects_conflicting_speed_flags() {
        assert!(
            Cli::try_parse_from([
                "opencolorhub",
                "fan",
                "set-speed",
                "--percent",
                "50",
                "--pwm",
                "100"
            ])
            .is_err()
        );
    }

    #[test]
    fn backend_arg_maps_to_mode() {
        assert_eq!(
            BackendMode::from(BackendArg::Simulated),
            BackendMode::Simulated
        );
        let cli = Cli::try_parse_from(["opencolorhub", "--backend", "simulated", "fan", "status"])
            .unwrap();
        assert_eq!(BackendMode::from(cli.backend), BackendMode::Simulated);
    }
}
