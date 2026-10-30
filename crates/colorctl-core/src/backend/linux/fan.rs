//! NCT5584D fan backend.

use crate::backend::FanBackend;
use crate::backend::linux::portio::SuperIo;
use crate::curve::{self, FanCurve};
use crate::error::Result;
use crate::model::{DeviceInfo, FAN_HEADERS, FanHeader, FanMode, FanSample, Telemetry};

/// Fan + thermal control on supported Colorful boards.
pub struct LinuxFan {
    sio: SuperIo,
    /// Curves last read or written, indexed by [`FanHeader::index`].
    curves: [Option<FanCurve>; FAN_HEADERS.len()],
}

impl LinuxFan {
    pub fn open() -> Result<Self> {
        let sio = SuperIo::open()?;
        Ok(Self {
            sio,
            curves: [None; FAN_HEADERS.len()],
        })
    }

    pub fn chip_id(&self) -> u16 {
        self.sio.chip_id
    }

    /// Reads the curve for a header once and caches it (the table is 8 register
    /// reads plus a bank switch, which we do not want on every poll).
    fn curve_for(&mut self, header: FanHeader, mode: FanMode) -> Option<FanCurve> {
        if mode != FanMode::SmartFan {
            return None;
        }
        if let Some(curve) = self.curves[header.index()] {
            return Some(curve);
        }
        match self.sio.read_curve(header) {
            Ok(points) => {
                let curve = curve::from_pwm_points(&points);
                self.curves[header.index()] = Some(curve);
                Some(curve)
            }
            Err(_) => None,
        }
    }
}

impl FanBackend for LinuxFan {
    fn info(&self) -> DeviceInfo {
        DeviceInfo::hardware(
            "Nuvoton NCT5584D",
            "Super I/O hardware monitor via /dev/port",
        )
        .with_path("/dev/port")
    }

    fn read(&mut self) -> Result<Telemetry> {
        let temperature_c = self.sio.temperature().ok().map(|t| t as f32);

        let mut fans = Vec::with_capacity(FAN_HEADERS.len());
        for header in FAN_HEADERS {
            let rpm = self.sio.fan_rpm(header)?;
            let pwm = self.sio.read_pwm(header).unwrap_or(0);
            let mode = self
                .sio
                .read_mode(header)
                .map(FanMode::from_register)
                .unwrap_or(FanMode::Unknown);
            let curve = self.curve_for(header, mode);
            fans.push(FanSample {
                header,
                rpm,
                pwm,
                mode,
                curve,
            });
        }

        Ok(Telemetry {
            temperature_c,
            fans,
        })
    }

    fn set_pwm(&mut self, header: FanHeader, pwm: u8) -> Result<()> {
        self.sio.set_manual_pwm(header, pwm)?;
        // A manual write invalidates any cached curve.
        self.curves[header.index()] = None;
        Ok(())
    }

    fn set_curve(&mut self, header: FanHeader, curve: &FanCurve) -> Result<()> {
        curve::validate(curve)?;
        let points = curve::pwm_points(curve);
        self.sio.set_curve(header, &points)?;
        self.curves[header.index()] = Some(*curve);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_are_ordered_for_the_cache() {
        for (i, header) in FAN_HEADERS.iter().enumerate() {
            assert_eq!(header.index(), i);
        }
    }
}
