//! Raw Super I/O access through `/dev/port`.
//!
//! The NCT5584D is programmed through the classic index/data port pair
//! (0x4E/0x4F, falling back to 0x2E/0x2F). Reads and writes are ordinary
//! `seek`+`read`/`write` calls on `/dev/port`, which requires root (or
//! `CAP_SYS_RAWIO`).

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

use crate::error::{Error, Result};
use crate::model::NCT5584D_CHIP_ID;

/// Index opening key for the extended-function registers.
const SIO_UNLOCK: u8 = 0x87;
/// Index closing key.
const SIO_LOCK: u8 = 0xAA;

/// Logical Device Number 11 — the hardware monitor block.
const LDN_HWM: u8 = 0x0B;

/// Byte-level access to the I/O port space.
pub struct Backend {
    file: File,
}

impl Backend {
    pub fn new() -> Result<Self> {
        match OpenOptions::new().read(true).write(true).open("/dev/port") {
            Ok(file) => Ok(Self { file }),
            Err(e) => Err(Error::unavailable(format!(
                "Port I/O access failed: cannot open /dev/port ({e}). Root privileges (sudo/doas) \
                 or CAP_SYS_RAWIO are required."
            ))),
        }
    }

    pub fn read_byte(&mut self, port: u16) -> Result<u8> {
        self.file.seek(SeekFrom::Start(port as u64))?;
        let mut buf = [0u8; 1];
        self.file.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn write_byte(&mut self, port: u16, val: u8) -> Result<()> {
        self.file.seek(SeekFrom::Start(port as u64))?;
        self.file.write_all(&[val])?;
        Ok(())
    }
}

/// An opened, validated NCT5584D hardware monitor.
pub struct SuperIo {
    backend: Backend,
    pub reg_port: u16,
    pub val_port: u16,
    pub hwm_index_port: u16,
    pub hwm_data_port: u16,
    pub chip_id: u16,
}

impl SuperIo {
    /// Probes both index/data port pairs, validates the chip family, unlocks
    /// IO space and reads the HWM base address.
    pub fn open() -> Result<Self> {
        let mut backend = Backend::new()?;

        let candidates = [(0x4Eu16, 0x4Fu16), (0x2Eu16, 0x2Fu16)];
        let mut selected = None;
        for (reg, val) in candidates {
            if let Ok(id) = Self::read_chip_id(&mut backend, reg, val)
                && id != 0
                && id != 0xFFFF
            {
                selected = Some((reg, val, id));
                break;
            }
        }

        let (reg_port, val_port, chip_id) = selected.ok_or_else(|| {
            Error::unavailable(
                "Super I/O chip not detected at ports 0x4E or 0x2E (read returned 0xFFFF / no \
                 response). Ensure the kernel allows port I/O.",
            )
        })?;

        if (chip_id >> 4) != (NCT5584D_CHIP_ID >> 4) {
            return Err(Error::backend(format!(
                "Unsupported Super I/O chip detected (ID: 0x{chip_id:04X}). opencolorhub requires a \
                 Nuvoton NCT5584D (ID: 0x{NCT5584D_CHIP_ID:04X}). Writing registers on an \
                 incompatible chip could corrupt hardware."
            )));
        }

        Self::disable_io_space_lock(&mut backend, reg_port, val_port)?;

        // Query the Hardware Monitor base address from LDN 0x0B.
        backend.write_byte(reg_port, SIO_UNLOCK)?;
        backend.write_byte(reg_port, SIO_UNLOCK)?;
        backend.write_byte(reg_port, 0x07)?;
        backend.write_byte(val_port, LDN_HWM)?;
        backend.write_byte(reg_port, 0x60)?;
        let base_hi = backend.read_byte(val_port)?;
        backend.write_byte(reg_port, 0x61)?;
        let base_lo = backend.read_byte(val_port)?;
        backend.write_byte(reg_port, SIO_LOCK)?;

        let hwm_base = ((base_hi as u16) << 8) | base_lo as u16;
        if hwm_base == 0 || hwm_base == 0xFFFF {
            return Err(Error::backend(format!(
                "Failed to read Hardware Monitor base address from Super I/O LDN 0x0B \
                 (got 0x{hwm_base:04X})"
            )));
        }

        Ok(Self {
            backend,
            reg_port,
            val_port,
            hwm_index_port: hwm_base + 5,
            hwm_data_port: hwm_base + 6,
            chip_id,
        })
    }

    fn read_chip_id(backend: &mut Backend, reg_port: u16, val_port: u16) -> Result<u16> {
        backend.write_byte(reg_port, SIO_UNLOCK)?;
        backend.write_byte(reg_port, SIO_UNLOCK)?;
        backend.write_byte(reg_port, 0x20)?;
        let hi = backend.read_byte(val_port)?;
        backend.write_byte(reg_port, 0x21)?;
        let lo = backend.read_byte(val_port)?;
        backend.write_byte(reg_port, SIO_LOCK)?;
        Ok(((hi as u16) << 8) | lo as u16)
    }

    /// Clears bit 4 of register 0x28 (IO space lock).
    fn disable_io_space_lock(backend: &mut Backend, reg_port: u16, val_port: u16) -> Result<()> {
        backend.write_byte(reg_port, SIO_UNLOCK)?;
        backend.write_byte(reg_port, SIO_UNLOCK)?;
        backend.write_byte(reg_port, 0x28)?;
        let val = backend.read_byte(val_port)?;
        if (val & 0x10) != 0 {
            backend.write_byte(val_port, val & !0x10)?;
        }
        backend.write_byte(reg_port, SIO_LOCK)?;
        Ok(())
    }

    /// Selects a HWM bank (register 0x4E keeps the high nibble).
    pub fn set_bank(&mut self, bank: u8) -> Result<()> {
        self.backend.write_byte(self.hwm_index_port, 0x4E)?;
        let cur = self.backend.read_byte(self.hwm_data_port)?;
        self.backend
            .write_byte(self.hwm_data_port, (cur & 0xF0) | (bank & 0x0F))?;
        Ok(())
    }

    pub fn read_hwm_reg(&mut self, bank: u8, reg: u8) -> Result<u8> {
        self.set_bank(bank)?;
        self.backend.write_byte(self.hwm_index_port, reg)?;
        self.backend.read_byte(self.hwm_data_port)
    }

    pub fn write_hwm_reg(&mut self, bank: u8, reg: u8, val: u8) -> Result<()> {
        self.set_bank(bank)?;
        self.backend.write_byte(self.hwm_index_port, reg)?;
        self.backend.write_byte(self.hwm_data_port, val)?;
        Ok(())
    }

    /// Reads the 16-bit tachometer; values ≥ 50000 mean "no fan attached".
    pub fn fan_rpm(&mut self, header: crate::model::FanHeader) -> Result<u16> {
        let base = header.rpm_reg();
        let hi = self.read_hwm_reg(4, base)?;
        let lo = self.read_hwm_reg(4, base.wrapping_add(1))?;
        let rpm = ((hi as u16) << 8) | lo as u16;
        Ok(if rpm >= 50_000 { 0 } else { rpm })
    }

    /// Board temperature in °C from bank 1 register 0x50.
    pub fn temperature(&mut self) -> Result<u8> {
        let temp = self.read_hwm_reg(1, 0x50)?;
        Ok(if temp >= 150 { 0 } else { temp })
    }

    /// Manual duty cycle (bank per header).
    pub fn set_manual_pwm(&mut self, header: crate::model::FanHeader, pwm: u8) -> Result<()> {
        let bank = header.bank();
        // Register 2: 0x00 = manual mode.
        self.write_hwm_reg(bank, 0x02, 0x00)?;
        // Register 9: PWM duty.
        self.write_hwm_reg(bank, 0x09, pwm)?;
        Ok(())
    }

    /// Programs a 4-point SmartFan table.
    pub fn set_curve(
        &mut self,
        header: crate::model::FanHeader,
        points: &[(u8, u8); 4],
    ) -> Result<()> {
        let bank = header.bank();

        // SmartFan configuration straight from NCT5584D_Service.cs.
        self.write_hwm_reg(bank, 0x66, 0xAA)?; // unlock curve registers
        self.write_hwm_reg(bank, 0x03, 0x01)?;
        self.write_hwm_reg(bank, 0x04, 0x01)?;
        self.write_hwm_reg(bank, 0x02, 0x40)?; // SmartFan mode

        for (i, (temp_c, _)) in points.iter().enumerate() {
            self.write_hwm_reg(bank, 0x21 + i as u8, *temp_c)?;
        }
        for (i, (_, pwm)) in points.iter().enumerate() {
            self.write_hwm_reg(bank, 0x27 + i as u8, *pwm)?;
        }
        Ok(())
    }

    /// Reads back the programmed curve (temps 0x21..0x24, duty 0x27..0x2A).
    pub fn read_curve(&mut self, header: crate::model::FanHeader) -> Result<[(u8, u8); 4]> {
        let bank = header.bank();
        let mut points = [(0u8, 0u8); 4];
        for i in 0..4u8 {
            points[i as usize].0 = self.read_hwm_reg(bank, 0x21 + i)?;
        }
        for i in 0..4u8 {
            points[i as usize].1 = self.read_hwm_reg(bank, 0x27 + i)?;
        }
        Ok(points)
    }

    /// Reads back the manual duty register.
    pub fn read_pwm(&mut self, header: crate::model::FanHeader) -> Result<u8> {
        self.read_hwm_reg(header.bank(), 0x09)
    }

    /// Reads back the control mode register.
    pub fn read_mode(&mut self, header: crate::model::FanHeader) -> Result<u8> {
        self.read_hwm_reg(header.bank(), 0x02)
    }
}
