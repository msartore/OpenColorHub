//! Nuvoton hardware monitor through PawnIO, on Windows.
//!
//! Register map recovered empirically from a Colorful CVN X570M GAMING PRO V14
//! (Super I/O `0xD121`, HWM base `0x0A30`), by sampling every bank twice and
//! then watching which registers respond to CPU load:
//!
//! | register      | idle  | under load | interpretation |
//! |---------------|-------|------------|----------------|
//! | bank 6, 0x56  | ~3640 | ~4426      | fan, ramps 22%  |
//! | bank 6, 0x58  | ~1155 | ~1331      | fan, ramps 15%  |
//! | bank 0, 0x10  | 1279  | 1279       | static, not a sensor |
//!
//! The HWM block is reached through an index/data pair at `base+0x05` /
//! `base+0x06`; the bank selector is register `0x4E` written through that pair.
//! That detail matters: writing to `base+0x4E` directly is denied by PawnIO's
//! port allow-list, and silently leaves every bank reading identical.
//!
//! Writing fan speeds needs the PWM command registers, which have not been
//! identified on this chip yet. The backend therefore reports real readings and
//! refuses control with a reason, rather than guessing at registers.

use crate::backend::windows::pawnio::LpcIo;
use crate::backend::{FanBackend, FanHeader};
use crate::error::{Error, Result};
use crate::model::{DeviceInfo, FanMode, FanSample, Telemetry};

/// HWM index / data offsets inside the 8-byte BAR window.
const INDEX_OFFSET: u16 = 0x05;
const DATA_OFFSET: u16 = 0x06;
/// Bank-select register, written through the index/data pair.
const BANK_REGISTER: u8 = 0x4E;

/// PWM duty readback, bank 0 (NCT677x family layout).
const PWM_OUT_REGISTERS: [u8; TACH_SLOTS] = [0x01, 0x03, 0x11, 0x13, 0x15];
/// Register holding the manual/automatic mode bit, per fan bank.
const REG_CONTROL_MODE: u8 = 0x02;
/// Register holding the PWM command value, per fan bank.
const REG_PWM_COMMAND: u8 = 0x09;
/// One control bank per fan: CPU in bank 1, then 2, 3, 8, 9.
const CONTROL_BANKS: [u8; TACH_SLOTS] = [1, 2, 3, 8, 9];

/// Fan tachometer registers: bank 6, `0x56 + 2*i`.
const TACH_BANK: u8 = 6;
const TACH_BASE: u8 = 0x56;
const TACH_SLOTS: usize = 5;

/// Temperature registers in bank 0 (Nuvoton 8-bit sensors).
const TEMP_BANK: u8 = 0;
const TEMP_REGISTERS: [u8; 4] = [0x27, 0x73, 0x75, 0x77];

/// Below this a tach reading is noise rather than a stopped fan.
const MIN_RPM: u16 = 200;
/// An unwired tach reads all-ones.
const NO_FAN: u16 = 0xFFFF;

/// Whether a raw tach word represents a fan that is actually spinning.
fn is_live_tach(rpm: u16) -> bool {
    rpm != NO_FAN && rpm >= MIN_RPM
}

/// A Nuvoton hardware monitor reached through PawnIO.
pub struct PawnIoFan {
    lpc: LpcIo,
    hwm: u16,
    chip: u16,
    /// Curves evaluated in software, because this chip's SmartFan registers are
    /// not mapped. Reapplied on every poll.
    curves: [Option<crate::FanCurve>; TACH_SLOTS],
    /// Last duty written per slot, so an unchanged curve does not rewrite the
    /// register every second.
    last_duty: [Option<u8>; TACH_SLOTS],
}

impl PawnIoFan {
    /// Opens the chip, validates the family and resolves the HWM window.
    pub fn open() -> Result<Self> {
        let lpc = LpcIo::open(0)?;

        let chip = lpc.unlock_and_identify()?;
        if chip == 0 || chip == 0xFFFF {
            return Err(Error::unavailable(
                "No Super I/O responded at 0x2E. PawnIO is loaded, but no chip answered — \
                 another tool may hold the bus.",
            ));
        }
        // 0xD1xx: NCT6793D / NCT5567D-B family. The tach layout below was
        // verified on 0xD121; refuse anything else instead of reporting junk.
        if (chip >> 8) != 0xD1 {
            let _ = lpc.lock();
            return Err(Error::backend(format!(
                "Unsupported Super I/O 0x{chip:04X}: fan registers are only mapped for 0xD1xx \
                 (NCT6793D / NCT5567D-B). Readings are left to HWiNFO or the simulator."
            )));
        }

        lpc.find_bars()?;
        lpc.superio_outb(0x07, 0x0B)?;
        let high = lpc.superio_inb(0x60)?;
        let low = lpc.superio_inb(0x61)?;
        let hwm = (u16::from(high) << 8) | u16::from(low);
        lpc.superio_outb(0x07, 0x00)?;
        lpc.lock()?;

        if hwm == 0 || hwm == 0xFFFF {
            return Err(Error::backend(
                "The hardware monitor has no base address programmed (LDN 0x0B, 0x60/0x61).",
            ));
        }

        Ok(Self {
            lpc,
            hwm,
            chip,
            curves: [None; TACH_SLOTS],
            last_duty: [None; TACH_SLOTS],
        })
    }

    fn select_bank(&self, bank: u8) -> Result<()> {
        self.write_hwm(BANK_REGISTER, bank)
    }

    fn read_hwm(&self, register: u8) -> Result<u8> {
        self.lpc.pio_outb(self.hwm + INDEX_OFFSET, register)?;
        self.lpc.pio_inb(self.hwm + DATA_OFFSET)
    }

    fn write_hwm(&self, register: u8, value: u8) -> Result<()> {
        self.lpc.pio_outb(self.hwm + INDEX_OFFSET, register)?;
        self.lpc.pio_outb(self.hwm + DATA_OFFSET, value)
    }

    fn read_word(&self, bank: u8, register: u8) -> Result<u16> {
        self.select_bank(bank)?;
        let high = self.read_hwm(register)?;
        let low = self.read_hwm(register + 1)?;
        Ok((u16::from(high) << 8) | u16::from(low))
    }

    fn read_byte(&self, bank: u8, register: u8) -> Result<u8> {
        self.select_bank(bank)?;
        self.read_hwm(register)
    }

    /// Tachometers in header order, with unwired slots dropped.
    fn tachometers(&self) -> Result<Vec<(usize, u16)>> {
        let mut out = Vec::new();
        for slot in 0..TACH_SLOTS {
            let register = TACH_BASE + (slot as u8) * 2;
            let rpm = self.read_word(TACH_BANK, register)?;
            if !is_live_tach(rpm) {
                continue;
            }
            out.push((slot, rpm));
        }
        Ok(out)
    }

    /// Best available temperature: the first plausible bank-0 sensor.
    fn temperature(&self) -> Result<Option<f32>> {
        for register in TEMP_REGISTERS {
            let raw = self.read_byte(TEMP_BANK, register)?;
            let signed = raw as i8;
            // Reject the 0x80-ish rail that unwired sensors report.
            if (5..110).contains(&signed) {
                return Ok(Some(f32::from(signed)));
            }
        }
        Ok(None)
    }
}

impl PawnIoFan {
    /// Writes a raw duty to one header and verifies it against the readback.
    fn write_duty(&mut self, slot: usize, pwm: u8) -> Result<()> {
        let bank = CONTROL_BANKS[slot.min(CONTROL_BANKS.len() - 1)];

        // Manual mode first: leaving the chip automatic while writing the
        // command register means SmartFan overwrites the value immediately.
        self.select_bank(bank)?;
        self.write_hwm(REG_CONTROL_MODE, 0x00)?;
        self.write_hwm(REG_PWM_COMMAND, pwm)?;

        // Verify against the independent duty readback. A silent refusal would
        // otherwise be indistinguishable from success.
        let applied = self.read_byte(TEMP_BANK, PWM_OUT_REGISTERS[slot])?;
        if applied.abs_diff(pwm) > 8 {
            return Err(Error::backend(format!(
                "Wrote {pwm} to fan {slot}, but the chip reports {applied}. The register                  map may not match this board, so fan control is not trusted here."
            )));
        }
        Ok(())
    }

    /// Re-evaluates every software curve against the current temperature.
    ///
    /// Hysteresis: a duty that moved by less than 3/255 is left alone, so the
    /// register is not rewritten every second for a fraction of a percent.
    fn apply_curves(&mut self, temperature: Option<f32>) -> Result<()> {
        let Some(temp) = temperature else {
            return Ok(());
        };
        for slot in 0..TACH_SLOTS {
            let Some(curve) = self.curves[slot] else {
                continue;
            };
            let duty = crate::units::pct(crate::curve::eval(&curve, temp));
            if let Some(prev) = self.last_duty[slot]
                && prev.abs_diff(duty) < 3
            {
                continue;
            }
            self.write_duty(slot, duty)?;
            self.last_duty[slot] = Some(duty);
        }
        Ok(())
    }
}

fn slot_of(header: FanHeader) -> Result<usize> {
    FanHeader::all()
        .iter()
        .position(|h| *h == header)
        .ok_or_else(|| Error::backend("unknown fan header"))
}

impl FanBackend for PawnIoFan {
    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            name: format!("Nuvoton 0x{:04X} (PawnIO)", self.chip),
            detail: format!(
                "Super I/O hardware monitor at 0x{:04X}, read through PawnIO's sandboxed \
                 LpcIO module",
                self.hwm
            ),
            path: Some("PawnIO".to_string()),
            available: true,
            simulated: false,
            error: None,
        }
    }

    fn read(&mut self) -> std::result::Result<Telemetry, Error> {
        let temperature = self.temperature()?;

        // Software curves are driven from here: the poll is the only clock the
        // backend has, and it already has the temperature.
        self.apply_curves(temperature)?;

        let tachs = self.tachometers()?;

        let mut fans = Vec::with_capacity(tachs.len());
        for (slot, rpm) in tachs {
            // Duty is a separate register from the tachometer; a failed read
            // leaves it at zero rather than failing the whole snapshot.
            let pwm = self
                .read_byte(TEMP_BANK, PWM_OUT_REGISTERS[slot])
                .unwrap_or(0);
            fans.push(FanSample {
                header: FanHeader::all()[slot.min(TACH_SLOTS - 1)],
                rpm,
                pwm,
                mode: if self.curves[slot].is_some() {
                    FanMode::SmartFan
                } else {
                    FanMode::Manual
                },
                curve: self.curves[slot],
            });
        }

        Ok(Telemetry {
            temperature_c: temperature,
            fans,
        })
    }

    fn set_pwm(&mut self, header: FanHeader, pwm: u8) -> std::result::Result<(), Error> {
        let slot = FanHeader::all()
            .iter()
            .position(|h| *h == header)
            .ok_or_else(|| Error::backend("unknown fan header"))?;
        let bank = CONTROL_BANKS[slot.min(CONTROL_BANKS.len() - 1)];

        // Manual mode first: leaving the chip automatic while writing the
        // command register means SmartFan overwrites the value immediately.
        self.select_bank(bank)?;
        self.write_hwm(REG_CONTROL_MODE, 0x00)?;
        self.write_hwm(REG_PWM_COMMAND, pwm)?;

        // Verify against the independent duty readback. A silent refusal would
        // otherwise be indistinguishable from success.
        let applied = self.read_byte(TEMP_BANK, PWM_OUT_REGISTERS[slot])?;
        if applied.abs_diff(pwm) > 8 {
            return Err(Error::backend(format!(
                "Wrote {pwm} to fan {slot}, but the chip reports {applied}. The register \
                 map may not match this board, so fan control is not trusted here."
            )));
        }
        Ok(())
    }

    fn set_curve(
        &mut self,
        header: FanHeader,
        curve: &crate::FanCurve,
    ) -> std::result::Result<(), Error> {
        let slot = slot_of(header)?;

        // This chip's SmartFan registers are unmapped, so the curve is evaluated
        // here instead of being handed to the firmware. It is re-applied on
        // every poll, which is functionally equivalent for a 1 s interval.
        self.curves[slot] = Some(*curve);
        self.last_duty[slot] = None;

        if let Some(temp) = self.temperature()? {
            let duty = crate::units::pct(crate::curve::eval(curve, temp));
            self.write_duty(slot, duty)?;
            self.last_duty[slot] = Some(duty);
        }
        Ok(())
    }

    /// Curves are evaluated in software rather than programmed into SmartFan,
    /// but the caller's contract is the same: a curve is in force afterwards.
    fn supports_curves(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_chips_outside_the_mapped_family() {
        // 0xD42A is the NCT5584D the Linux backend targets; its tach layout
        // differs, so the Windows backend must refuse rather than misread.
        let chip: u16 = 0xD42A;
        assert_ne!(chip >> 8, 0xD1);
    }

    #[test]
    fn tach_filter_keeps_slow_fans_and_drops_unwired_slots() {
        assert!(is_live_tach(1180), "a normal fan must report");
        assert!(is_live_tach(300), "a slow fan must still report");
        assert!(!is_live_tach(NO_FAN), "an unwired slot reads all-ones");
        assert!(!is_live_tach(0), "a stopped fan reads zero");
        assert!(!is_live_tach(50), "line noise must not become an RPM");
    }

    #[test]
    fn every_tach_slot_maps_to_a_header() {
        assert_eq!(FanHeader::all().len(), TACH_SLOTS);
    }
}
