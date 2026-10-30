//! Percentage ⇄ PWM conversions used by the Super I/O layer.
//!
//! The NCT5584D takes a raw 8-bit duty cycle. The UI and the CLI speak percent,
//! so the conversions live in one place and are covered by tests.

/// Converts a percentage (0-100) to a PWM value (0-255).
///
/// Values above 100 are clamped, matching the upstream behaviour.
#[inline]
pub const fn pct(p: u8) -> u8 {
    let p = if p > 100 { 100 } else { p };
    (p as u32 * 255).div_ceil(100) as u8
}

/// Converts a raw PWM value (0-255) back to an approximate percentage (0-100).
#[inline]
pub const fn pwm_to_pct(pwm: u8) -> u8 {
    ((pwm as u16 * 100 + 127) / 255) as u8
}

/// Formats a temperature for display in the requested unit.
pub fn format_temp(celsius: f32, unit: crate::settings::TempUnit) -> String {
    match unit {
        crate::settings::TempUnit::Celsius => format!("{celsius:.0}°C"),
        crate::settings::TempUnit::Fahrenheit => format!("{:.0}°F", celsius * 9.0 / 5.0 + 32.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_round_trips_within_one_point() {
        for p in 0..=100u8 {
            let back = pwm_to_pct(pct(p));
            assert!(back.abs_diff(p) <= 1, "percent {p} round-tripped to {back}");
        }
    }

    #[test]
    fn endpoints_are_exact() {
        assert_eq!(pct(0), 0);
        assert_eq!(pct(100), 255);
        assert_eq!(pwm_to_pct(0), 0);
        assert_eq!(pwm_to_pct(255), 100);
    }

    #[test]
    fn over_hundred_is_clamped() {
        assert_eq!(pct(101), 255);
        assert_eq!(pct(255), 255);
    }

    #[test]
    fn mid_values_match_upstream_rounding() {
        assert_eq!(pct(50), 128); // (50*255+99)/100 = 128
        assert_eq!(pwm_to_pct(128), 50);
    }
}
