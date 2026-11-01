//! Where each fan channel lives, in one table.
//!
//! The bank numbers come from Colorful's own `NCT5567D_B_Service.GetBank()`:
//! SYS 1, CPU 2, AUX0 3, AUX1 8, AUX2 9, AUX3 10. This table is the only copy
//! in the tree on purpose — a second one inside a backend is how the Windows
//! and Linux paths drifted apart and started commanding the wrong header.
//!
//! Tachometers are the one place the platforms disagree: this board answers in
//! bank 6 at `0x56 + 2i` (measured: 3640 rpm idle, 4426 under load), while the
//! vendor's service reads bank 4 at `0xC0 + 2i`. Both are printed by
//! `colorctl fan probe`, so the disagreement can be settled by ear rather than
//! by argument.

use crate::model::FanHeader;

/// Bank holding each channel's control registers (`0x02` mode, `0x09` duty).
pub const BANKS: [u8; 6] = [1, 2, 3, 8, 9, 10];

/// Super I/O bank whose registers drive `header`.
pub fn bank(header: FanHeader) -> u8 {
    BANKS[header.index()]
}

/// Tachometer window the running platform reads.
#[cfg(target_os = "windows")]
pub const TACH_BANK: u8 = 6;
#[cfg(target_os = "windows")]
pub const TACH_BASE: u8 = 0x56;

#[cfg(not(target_os = "windows"))]
pub const TACH_BANK: u8 = 4;
#[cfg(not(target_os = "windows"))]
pub const TACH_BASE: u8 = 0xC0;

/// The channel the vendor's table marks as a pump, when it has one.
///
/// This board's table does not: its channels are SYS, CPU and AUX0-AUX3, and
/// an `AIO_PUMP` card was an invention inherited from another board. Settings
/// start from this value; the user binds a channel when their cooler has one.
pub const PUMP_CHANNEL: Option<FanHeader> = None;

/// Register holding `header`'s 16-bit tachometer value, high byte first.
pub fn tach_register(header: FanHeader) -> u8 {
    TACH_BASE + (header.index() as u8) * 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FAN_HEADERS;

    #[test]
    fn every_channel_has_a_bank_and_a_tachometer() {
        assert_eq!(BANKS.len(), FAN_HEADERS.len());
        for header in FAN_HEADERS {
            assert!(bank(header) >= 1);
            assert!(tach_register(header) >= TACH_BASE);
        }
    }

    #[test]
    fn banks_match_the_vendors_service() {
        assert_eq!(bank(FanHeader::Sys), 1);
        assert_eq!(bank(FanHeader::Cpu), 2);
        assert_eq!(bank(FanHeader::Aux0), 3);
        assert_eq!(bank(FanHeader::Aux1), 8);
        assert_eq!(bank(FanHeader::Aux2), 9);
        assert_eq!(bank(FanHeader::Aux3), 10);
    }

    #[test]
    fn tachometers_do_not_overlap() {
        let mut seen = Vec::new();
        for header in FAN_HEADERS {
            let register = tach_register(header);
            assert!(!seen.contains(&register), "{header} shares {register:#x}");
            seen.push(register);
        }
    }
}
