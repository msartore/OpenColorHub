//! Four point SmartFan curves.
//!
//! The NCT5584D stores four `(temperature, duty)` pairs per fan bank in
//! registers `0x21..0x24` (temps) and `0x27..0x2A` (duty). Temperatures must be
//! strictly ascending, otherwise the firmware ignores the table — so validation
//! happens here, once, for both the CLI and the GUI.

use serde::{Deserialize, Serialize};

use crate::error::CurveError;
use crate::units::{pct, pwm_to_pct};

/// The chip only understands four points.
pub const CURVE_POINTS: usize = 4;

/// One `(temperature °C, duty %)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub temp_c: u8,
    pub percent: u8,
}

impl CurvePoint {
    pub const fn new(temp_c: u8, percent: u8) -> Self {
        Self { temp_c, percent }
    }

    pub fn pwm(&self) -> u8 {
        pct(self.percent)
    }

    pub fn from_pwm(temp_c: u8, pwm: u8) -> Self {
        Self {
            temp_c,
            percent: pwm_to_pct(pwm),
        }
    }
}

/// Exactly four points, in ascending temperature order.
pub type FanCurve = [CurvePoint; CURVE_POINTS];

/// Built-in profiles shipped by Colorful (see upstream README).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanProfile {
    Quiet,
    Standard,
    Full,
}

impl FanProfile {
    pub fn all() -> &'static [FanProfile] {
        &[FanProfile::Quiet, FanProfile::Standard, FanProfile::Full]
    }

    pub fn id(&self) -> &'static str {
        match self {
            FanProfile::Quiet => "quiet",
            FanProfile::Standard => "standard",
            FanProfile::Full => "full",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FanProfile::Quiet => "Quiet",
            FanProfile::Standard => "Standard",
            FanProfile::Full => "Full speed",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            FanProfile::Quiet => "Lowest noise, higher temperatures under load",
            FanProfile::Standard => "Balanced daily profile",
            FanProfile::Full => "Maximum cooling, loudest",
        }
    }

    /// Curve for a normal header or, for `pump == true`, for the AIO pump.
    pub fn points_for(&self, pump: bool) -> FanCurve {
        match (self, pump) {
            (FanProfile::Quiet, false) => [
                CurvePoint::new(20, 20),
                CurvePoint::new(40, 30),
                CurvePoint::new(60, 50),
                CurvePoint::new(80, 85),
            ],
            (FanProfile::Standard, false) => [
                CurvePoint::new(20, 20),
                CurvePoint::new(40, 40),
                CurvePoint::new(60, 60),
                CurvePoint::new(80, 100),
            ],
            (FanProfile::Quiet, true) => [
                CurvePoint::new(0, 80),
                CurvePoint::new(70, 80),
                CurvePoint::new(75, 100),
                CurvePoint::new(100, 100),
            ],
            (FanProfile::Standard, true) => [
                CurvePoint::new(0, 80),
                CurvePoint::new(65, 80),
                CurvePoint::new(70, 100),
                CurvePoint::new(100, 100),
            ],
            (FanProfile::Full, _) => [
                CurvePoint::new(30, 100),
                CurvePoint::new(50, 100),
                CurvePoint::new(70, 100),
                CurvePoint::new(100, 100),
            ],
        }
    }
}

/// Validates a curve before it reaches the chip.
pub fn validate(curve: &[CurvePoint]) -> Result<(), CurveError> {
    if curve.len() != CURVE_POINTS {
        return Err(CurveError::WrongCount {
            expected: CURVE_POINTS,
            got: curve.len(),
        });
    }
    for p in curve {
        if p.percent > 100 {
            return Err(CurveError::PercentOutOfRange { percent: p.percent });
        }
        if p.temp_c > 120 {
            return Err(CurveError::TempOutOfRange { temp: p.temp_c });
        }
    }
    for i in 1..curve.len() {
        if curve[i].temp_c <= curve[i - 1].temp_c {
            return Err(CurveError::NotAscending {
                temp: curve[i].temp_c,
                previous: curve[i - 1].temp_c,
            });
        }
    }
    Ok(())
}

/// Duty (percent) the curve interpolates to at `temp_c`, clamped at the ends.
///
/// Mirrors how the firmware behaves between points: piecewise linear.
pub fn eval(curve: &[CurvePoint], temp_c: f32) -> u8 {
    if curve.is_empty() {
        return 0;
    }
    let first = curve[0];
    let last = curve[curve.len() - 1];
    if temp_c <= first.temp_c as f32 {
        return first.percent;
    }
    if temp_c >= last.temp_c as f32 {
        return last.percent;
    }
    for pair in curve.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if temp_c >= a.temp_c as f32 && temp_c <= b.temp_c as f32 {
            let span = (b.temp_c as f32 - a.temp_c as f32).max(f32::EPSILON);
            let t = (temp_c - a.temp_c as f32) / span;
            let percent = a.percent as f32 + t * (b.percent as f32 - a.percent as f32);
            return percent.round().clamp(0.0, 100.0) as u8;
        }
    }
    last.percent
}

/// Duty in raw PWM units for the register write.
pub fn pwm_points(curve: &FanCurve) -> [(u8, u8); CURVE_POINTS] {
    [
        (curve[0].temp_c, pct(curve[0].percent)),
        (curve[1].temp_c, pct(curve[1].percent)),
        (curve[2].temp_c, pct(curve[2].percent)),
        (curve[3].temp_c, pct(curve[3].percent)),
    ]
}

/// Rebuilds a curve from raw register values.
pub fn from_pwm_points(points: &[(u8, u8)]) -> FanCurve {
    let mut out = [CurvePoint::new(0, 0); CURVE_POINTS];
    for (i, (temp, pwm)) in points.iter().take(CURVE_POINTS).enumerate() {
        out[i] = CurvePoint::from_pwm(*temp, *pwm);
    }
    out
}

/// Parses `"30:25,50:45,70:75,85:100"` (the CLI `--points` syntax).
pub fn parse(raw: &str) -> Result<FanCurve, CurveError> {
    let mut curve = [CurvePoint::new(0, 0); CURVE_POINTS];
    let mut parts = raw.split(',').map(str::trim);

    for (i, slot) in curve.iter_mut().enumerate() {
        let part = parts.next().ok_or_else(|| {
            CurveError::Message(format!(
                "expected {CURVE_POINTS} points as 'T:P,...', got fewer ({i})"
            ))
        })?;
        let mut pair = part.split(':').map(str::trim);
        let (temp, percent) = match (pair.next(), pair.next(), pair.next()) {
            (Some(t), Some(p), None) => (t, p),
            _ => {
                return Err(CurveError::Message(format!(
                    "invalid point '{part}': expected 'Temp:Percent' (e.g. '50:40')"
                )));
            }
        };
        let temp_c = temp
            .parse::<u8>()
            .map_err(|e| CurveError::Message(format!("invalid temperature '{temp}': {e}")))?;
        let percent = percent
            .parse::<u8>()
            .map_err(|e| CurveError::Message(format!("invalid speed percent '{percent}': {e}")))?;
        *slot = CurvePoint::new(temp_c, percent);
    }

    if parts.next().is_some() {
        return Err(CurveError::Message(format!(
            "expected exactly {CURVE_POINTS} points, got more"
        )));
    }

    validate(&curve)?;
    Ok(curve)
}

/// Inverse of [`parse`], used by the CLI and the settings export.
pub fn format(curve: &FanCurve) -> String {
    curve
        .iter()
        .map(|p| format!("{}:{}", p.temp_c, p.percent))
        .collect::<Vec<_>>()
        .join(",")
}

/// Helper for the UI: the duty the chip would apply at the last measured temp.
pub fn percent_at(curve: &FanCurve, temperature_c: Option<f32>) -> Option<u8> {
    temperature_c.map(|t| eval(curve, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ascending_curves() {
        let ok = FanProfile::Standard.points_for(false);
        assert!(validate(&ok).is_ok());

        let bad = [
            CurvePoint::new(40, 20),
            CurvePoint::new(40, 40),
            CurvePoint::new(60, 60),
            CurvePoint::new(80, 100),
        ];
        assert_eq!(
            validate(&bad),
            Err(CurveError::NotAscending {
                temp: 40,
                previous: 40
            })
        );

        assert!(matches!(
            validate(&ok[..3]),
            Err(CurveError::WrongCount { .. })
        ));
    }

    #[test]
    fn rejects_out_of_range_values() {
        let too_hot = [
            CurvePoint::new(20, 20),
            CurvePoint::new(40, 40),
            CurvePoint::new(60, 60),
            CurvePoint::new(200, 100),
        ];
        assert!(matches!(
            validate(&too_hot),
            Err(CurveError::TempOutOfRange { .. })
        ));

        let too_fast = [
            CurvePoint::new(20, 20),
            CurvePoint::new(40, 40),
            CurvePoint::new(60, 60),
            CurvePoint::new(80, 101),
        ];
        assert!(matches!(
            validate(&too_fast),
            Err(CurveError::PercentOutOfRange { .. })
        ));
    }

    #[test]
    fn interpolates_linearly() {
        let curve = [
            CurvePoint::new(20, 20),
            CurvePoint::new(40, 40),
            CurvePoint::new(60, 60),
            CurvePoint::new(80, 100),
        ];
        assert_eq!(eval(&curve, 0.0), 20, "clamped below first point");
        assert_eq!(eval(&curve, 20.0), 20);
        assert_eq!(eval(&curve, 30.0), 30);
        assert_eq!(eval(&curve, 50.0), 50);
        assert_eq!(eval(&curve, 70.0), 80);
        assert_eq!(eval(&curve, 90.0), 100, "clamped above last point");
    }

    #[test]
    fn parses_and_formats_cli_syntax() {
        let curve = parse("30:20, 50:40,70:70,85:100").expect("valid");
        assert_eq!(curve[0], CurvePoint::new(30, 20));
        assert_eq!(curve[3], CurvePoint::new(85, 100));
        assert_eq!(format(&curve), "30:20,50:40,70:70,85:100");

        assert!(parse("30:20,50:40,70:70").is_err());
        assert!(parse("30:20,50:40,70:70,85").is_err());
        assert!(parse("85:20,50:40,70:70,90:100").is_err());
        assert!(parse("30-20,50:40,70:70,85:100").is_err());
    }

    #[test]
    fn pwm_points_round_trip() {
        let curve = FanProfile::Quiet.points_for(false);
        let pwm = pwm_points(&curve);
        let back = from_pwm_points(&pwm);
        for (a, b) in curve.iter().zip(back.iter()) {
            assert_eq!(a.temp_c, b.temp_c);
            assert!(a.percent.abs_diff(b.percent) <= 1);
        }
    }

    #[test]
    fn pump_profiles_never_stop_the_pump() {
        for profile in FanProfile::all() {
            let curve = profile.points_for(true);
            for temp in [0.0, 30.0, 60.0, 75.0, 100.0] {
                assert!(
                    eval(&curve, temp) >= 80,
                    "{profile:?} drops the pump below 80% at {temp}°C"
                );
            }
        }
    }

    #[test]
    fn pwm_max_constant_matches_duty_conversion() {
        assert_eq!(pct(100), crate::model::PWM_MAX);
    }
}
