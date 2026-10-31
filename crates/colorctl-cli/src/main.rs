//! `colorctl` CLI (desktop app: `opencolorhub`).
//!
//! Thin layer over [`colorctl_core::Service`]: parse args, call the service,
//! print the result. All hardware knowledge lives in the core crate, so this
//! binary builds and runs on Windows too (against the simulator).

mod cli;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::generate;

use cli::{Cli, Command, FanAction, RgbAction, RgbTarget};
use colorctl_core::model::ChannelConfig;
use colorctl_core::service::Service;
use colorctl_core::settings::{BackendMode, Settings};
use colorctl_core::{FanMode, FanProfile, board, curve, pwm_to_pct};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("colorctl: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let settings = Settings {
        backend: cli.backend.into(),
        smoothing: false,
        ..Settings::default()
    };

    // The simulator reports a representative Colorful board; the hardware
    // backends report whatever SMBIOS says this machine actually is.
    let detected = if settings.backend == BackendMode::Simulated {
        board::demo_board()
    } else {
        board::detect()
    };
    let mut service = Service::with_board(settings, data_dir(), detected);

    match cli.command {
        Command::Rgb(args) => handle_rgb(args.action, &mut service, cli.json),
        Command::Fan(args) => handle_fan(args.action, &mut service, cli.json),
        Command::Board => handle_board(&service, cli.json),
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            generate(shell, &mut cmd, name, &mut std::io::stdout());
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Writable directory for state, mirroring the upstream cascade.
fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("COLORCTL_STATE_DIR") {
        return PathBuf::from(dir);
    }
    colorctl_core::backend::default_data_dir()
}

/// `colorctl board` — SMBIOS identity plus the catalogue verdict.
fn handle_board(service: &Service, json: bool) -> Result<ExitCode> {
    let info = service.board();

    if json {
        println!("{}", serde_json::to_string_pretty(info)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!("Motherboard: {}", info.display_name());
    if let Some(identity) = &info.identity {
        if !identity.version.trim().is_empty() {
            println!("  Board rev:   {}", identity.version);
        }
        if !identity.bios_version.trim().is_empty() {
            println!(
                "  BIOS: {} {}",
                identity.bios_vendor.trim(),
                identity.bios_version.trim()
            );
        }
        if !identity.system_product.trim().is_empty() {
            println!("  System:      {}", identity.system_product.trim());
        }
    }
    println!("  Detected by: {}", info.source);
    println!(
        "  Support: {} ({} of {} catalogue boards)",
        info.confidence.label().to_uppercase(),
        info.catalog_size,
        board::BOARDS.len()
    );
    if let Some(matched) = &info.matched {
        println!("  Matched:     {matched}");
    }
    if let Some(family) = &info.family {
        println!(
            "  Family: {} / {}",
            family,
            info.series.as_deref().unwrap_or("unknown")
        );
    }
    for note in &info.notes {
        println!("  · {note}");
    }
    println!(
        "  Hardware: {}",
        if info.confidence.is_foreign() {
            "disabled — refusing to touch another vendor's Super I/O"
        } else if info.confidence == colorctl_core::BoardConfidence::Verified {
            "fan + lighting control enabled"
        } else {
            "Super I/O probe decides at runtime"
        }
    );
    Ok(ExitCode::SUCCESS)
}

fn handle_rgb(action: RgbAction, service: &mut Service, json: bool) -> Result<ExitCode> {
    match action {
        RgbAction::Status => {
            let snapshot = service.snapshot();
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot.channels)?);
                return Ok(ExitCode::SUCCESS);
            }
            println!(
                "{} ({})",
                snapshot.led_device.name, snapshot.led_device.detail
            );
            if let Some(path) = &snapshot.led_device.path {
                println!("  Device: {path}");
            }
            if let Some(err) = &snapshot.led_device.error {
                println!("  Status: {err}");
            }
            if snapshot.led_device.simulated {
                println!("  Mode:   simulated");
            }
            if let Some(state) = service.state_path() {
                println!("  State:  {}", state.display());
            }
            println!(
                "  Board: {} [{}]",
                snapshot.board.display_name(),
                snapshot.board.confidence.label()
            );
            println!("  Channels:");
            for ch in &snapshot.channels {
                if ch.enabled {
                    println!(
                        "    - {:<28} ({:>2} LEDs) : {} ({}%)",
                        ch.name, ch.leds, ch.hex, ch.brightness
                    );
                } else {
                    println!("    - {:<28} ({:>2} LEDs) : off", ch.name, ch.leds);
                }
            }
        }

        RgbAction::Probe => {
            #[cfg(target_os = "windows")]
            {
                let devices = colorctl_core::backend::windows::led::diagnostics();
                if json {
                    println!("{}", serde_json::to_string_pretty(&devices)?);
                } else if devices.is_empty() {
                    println!("No HID interface with VID 2F4C / PID 1000 is present.");
                } else {
                    println!("Colorful HID interfaces (VID 2F4C, PID 1000):");
                    for d in &devices {
                        let iface = d
                            .interface
                            .map(|n| format!("MI_{n:02}"))
                            .unwrap_or_else(|| "?".to_string());
                        let product = d
                            .product
                            .as_deref()
                            .map(|p| format!("  [{p}]"))
                            .unwrap_or_default();
                        println!(
                            "  {iface}  usage page 0x{}  out {:>3}  feat {:>3}{}{}",
                            d.usage_page,
                            d.output_report_len,
                            d.feature_report_len,
                            if d.selected { "  <= selected" } else { "" },
                            product,
                        );
                        println!("        {}", d.path);
                    }
                }
            }
            #[cfg(target_os = "linux")]
            {
                match colorctl_core::backend::linux::led::LinuxLed::find_device_path() {
                    Ok(path) => println!("{}", path.display()),
                    Err(e) => println!("{e}"),
                }
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            {
                println!("HID probe is not implemented on this platform.");
            }
            return Ok(ExitCode::SUCCESS);
        }

        RgbAction::Set {
            color,
            channel,
            brightness,
        } => {
            let rgb = parse_color(&color)?;
            let is_all = channel.contains(&RgbTarget::All);
            let targets = resolve_channels(&channel);

            for ch in &targets {
                service.stage_channel(*ch, ChannelConfig::rgb(rgb, brightness));
            }
            let outcome = service.apply_led()?;

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&outcome.snapshot.channels)?
                );
            } else if is_all {
                println!(
                    "Set all channels to #{} at {brightness}% brightness",
                    hex(rgb)
                );
            } else {
                for ch in &targets {
                    println!(
                        "Set channel '{ch}' to #{} at {brightness}% brightness",
                        hex(rgb)
                    );
                }
            }
        }

        RgbAction::Off { channel } => {
            let is_all = channel.contains(&RgbTarget::All);
            let targets = resolve_channels(&channel);

            for ch in &targets {
                service.stage_channel(*ch, ChannelConfig::off());
            }
            let outcome = service.apply_led()?;

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&outcome.snapshot.channels)?
                );
            } else if is_all {
                println!("Turned off all RGB channels");
            } else {
                for ch in &targets {
                    println!("Turned off channel '{ch}'");
                }
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn resolve_channels(targets: &[RgbTarget]) -> Vec<colorctl_core::Channel> {
    let mut out = Vec::new();
    for t in targets {
        for ch in t.channels() {
            if !out.contains(ch) {
                out.push(*ch);
            }
        }
    }
    out
}

fn handle_fan(action: FanAction, service: &mut Service, json: bool) -> Result<ExitCode> {
    let snapshot = service.snapshot();
    // A read-only backend (HWiNFO) still has real readings to show, so only
    // the status action is allowed through; mutating actions fall through and
    // fail with the backend's own explanation.
    if matches!(action, FanAction::Status)
        && let Some(err) = &snapshot.fan_device.error
    {
        anyhow::bail!("{err}");
    }

    match action {
        FanAction::Probe => {
            let Some(dump) = service.register_dump() else {
                eprintln!(
                    "no register map on this backend — the probe needs the Super I/O through PawnIO, run elevated"
                );
                return Ok(ExitCode::FAILURE);
            };
            let reads = dump?;
            if json {
                println!("{}", serde_json::to_string_pretty(&reads)?);
            } else {
                println!("{:<28} {:>4}  {:>6}  {:>5}", "what", "bank", "reg", "value");
                for read in &reads {
                    println!(
                        "{:<28} {:>4}  0x{:02X}  {:>5}",
                        read.label, read.bank, read.register, read.value
                    );
                }
                println!();
                println!("read-only: nothing was written");
            }
            return Ok(ExitCode::SUCCESS);
        }
        FanAction::Status => {
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
                return Ok(ExitCode::SUCCESS);
            }
            println!(
                "{} — {}",
                snapshot.fan_device.name, snapshot.fan_device.detail
            );
            println!(
                "  Board: {} [{}]",
                snapshot.board.display_name(),
                snapshot.board.confidence.label()
            );
            if let Some(temp) = snapshot.temperature_c {
                println!("  Motherboard Temp: {temp:.0} °C");
            }
            if snapshot.fan_device.simulated {
                println!("  Mode:   simulated");
            }
            println!("  Fan Speeds:");
            for fan in &snapshot.fans {
                let mode = match fan.mode {
                    FanMode::Manual => "manual",
                    FanMode::SmartFan => "smartfan",
                    FanMode::Unknown => "unknown",
                };
                // A read-only backend (HWiNFO) has no duty to report.
                if fan.mode == FanMode::Unknown {
                    println!("    - {:<10}         {:5} RPM", fan.label, fan.rpm);
                } else {
                    println!(
                        "    - {:<10} {:5} RPM {:>3}% [{}]",
                        fan.label, fan.rpm, fan.percent, mode
                    );
                }
                if let Some(active) = &fan.curve {
                    println!("        curve: {}", curve::format(active));
                }
            }
        }

        FanAction::SetSpeed { fan, percent, pwm } => {
            let pwm_val = match (percent, pwm) {
                (Some(p), None) => colorctl_core::pct(p),
                (None, Some(raw)) => raw,
                _ => anyhow::bail!("provide either --percent or --pwm"),
            };

            for header in fan.headers() {
                service
                    .set_fan_pwm(*header, pwm_val)
                    .with_context(|| format!("failed to set speed on {}", header.label()))?;
                if !json {
                    println!(
                        "Set {} to PWM {} (~{}%)",
                        header.label(),
                        pwm_val,
                        pwm_to_pct(pwm_val)
                    );
                }
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&service.snapshot())?);
            }
        }

        FanAction::SetCurve {
            fan,
            points,
            profile,
        } => {
            let headers = fan.headers();
            match (points, profile) {
                (Some(parsed), None) => {
                    for header in headers {
                        service
                            .set_fan_curve(*header, &parsed)
                            .with_context(|| format!("failed to program {}", header.label()))?;
                        if !json {
                            println!("Configured 4-point SmartFan curve on {}:", header.label());
                            for (i, p) in parsed.iter().enumerate() {
                                println!(
                                    " Point {}: {} °C -> PWM {} (~{}%)",
                                    i + 1,
                                    p.temp_c,
                                    colorctl_core::pct(p.percent),
                                    p.percent
                                );
                            }
                        }
                    }
                    if json {
                        println!("{}", serde_json::to_string_pretty(&service.snapshot())?);
                    }
                }
                (None, Some(arg)) => {
                    let selected: FanProfile = arg.into();
                    let outcome = service
                        .apply_profile(headers, selected)
                        .context("failed to apply profile")?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&outcome.snapshot)?);
                    } else {
                        println!("{}", outcome.message);
                        for header in headers {
                            let programmed = selected.points_for(header.is_pump());
                            println!(" {}: {}", header.label(), curve::format(&programmed));
                        }
                    }
                }
                _ => anyhow::bail!("provide either --points or --profile"),
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// Colour parser matching upstream: names first, then hex.
fn parse_color(raw: &str) -> Result<[u8; 3]> {
    let s = raw.trim();
    match s.to_ascii_lowercase().as_str() {
        "red" => return Ok([255, 0, 0]),
        "green" => return Ok([0, 255, 0]),
        "blue" => return Ok([0, 0, 255]),
        "white" => return Ok([255, 255, 255]),
        "black" | "off" => return Ok([0, 0, 0]),
        "yellow" => return Ok([255, 255, 0]),
        "cyan" => return Ok([0, 255, 255]),
        "magenta" | "purple" => return Ok([255, 0, 255]),
        "orange" => return Ok([255, 128, 0]),
        _ => {}
    }

    let raw_hex = s.trim_start_matches('#').trim_start_matches("0x");
    if raw_hex.len() == 6 && raw_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&raw_hex[0..2], 16)?;
        let g = u8::from_str_radix(&raw_hex[2..4], 16)?;
        let b = u8::from_str_radix(&raw_hex[4..6], 16)?;
        Ok([r, g, b])
    } else {
        anyhow::bail!(
            "invalid colour '{s}'. Expected hex (e.g. 'ff00aa', '#00ff00') or a name \
             ('red', 'blue', 'green', 'white', 'cyan', 'magenta', 'yellow', 'orange', 'off')"
        )
    }
}

fn hex(rgb: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_and_hex_colours() {
        assert_eq!(parse_color("red").unwrap(), [255, 0, 0]);
        assert_eq!(parse_color("#00FF88").unwrap(), [0, 255, 136]);
        assert_eq!(parse_color("0x0a0b0c").unwrap(), [10, 11, 12]);
        assert!(parse_color("nope").is_err());
        assert!(parse_color("#12345").is_err());
    }

    #[test]
    fn resolves_channel_targets_without_duplicates() {
        let resolved = resolve_channels(&[RgbTarget::All]);
        assert_eq!(resolved.len(), 6);
        let resolved = resolve_channels(&[RgbTarget::Led, RgbTarget::Led]);
        assert_eq!(resolved, vec![colorctl_core::Channel::Led]);
    }
}
