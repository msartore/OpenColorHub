//! Sleep/resume detection for the long-running frontends.
//!
//! A suspended process does not run its timers; the wall clock keeps moving.
//! A ticker that asked for one second and comes back with minutes missing has
//! therefore been through a sleep cycle — and that is the moment the hardware
//! forgets what it was told: the USB bus is power-cycled, so the RGB controller
//! returns to its own pattern, and the Super I/O reloads the firmware's fan
//! control.
//!
//! `SystemTime` is the clock on purpose: it is the one that keeps counting while
//! the machine is suspended.

use std::time::{Duration, SystemTime};

/// Extra time beyond the poll interval that still counts as ordinary jitter.
///
/// Deliberately short: a suspend on this hardware can be a two-second S3 blip
/// (a wake-armed device ends it), and a threshold that ignores those misses
/// exactly the resumes users notice. A false positive only rewrites values that
/// are already in force.
pub const DEFAULT_GRACE: Duration = Duration::from_millis(500);

/// Watches the gap between successive ticks of a polling loop.
#[derive(Debug, Clone)]
pub struct ResumeWatch {
    last: SystemTime,
    interval: Duration,
    grace: Duration,
}

impl ResumeWatch {
    pub fn new(interval: Duration) -> Self {
        Self::with_grace(interval, DEFAULT_GRACE)
    }

    pub fn with_grace(interval: Duration, grace: Duration) -> Self {
        Self {
            last: SystemTime::now(),
            interval,
            grace,
        }
    }

    /// Follows a settings change, so a longer poll interval is not mistaken for
    /// a sleep the first time it fires.
    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }

    /// Re-synchronises the watch just before the loop sleeps.
    ///
    /// The gap the next [`ResumeWatch::observe_now`] measures then covers only
    /// the sleep — not the work the previous tick did before calling this, which
    /// on a slow driver read would otherwise look like a suspend every second.
    pub fn mark(&mut self) {
        self.mark_at(SystemTime::now());
    }

    fn mark_at(&mut self, now: SystemTime) {
        self.last = now;
    }

    /// Records a tick at `now`; returns the gap when it followed a suspend.
    pub fn observe(&mut self, now: SystemTime) -> Option<Duration> {
        let previous = std::mem::replace(&mut self.last, now);
        // A clock corrected backwards is not a wake; the replace above already
        // re-synchronised, so the next tick measures from here.
        let gap = now.duration_since(previous).ok()?;
        (gap > self.interval + self.grace).then_some(gap)
    }

    pub fn observe_now(&mut self) -> Option<Duration> {
        self.observe(SystemTime::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch() -> ResumeWatch {
        ResumeWatch::with_grace(Duration::from_secs(1), Duration::from_secs(3))
    }

    #[test]
    fn an_ordinary_tick_is_not_a_resume() {
        let mut watch = watch();
        let t0 = SystemTime::now();
        assert_eq!(watch.observe(t0), None);
        assert_eq!(watch.observe(t0 + Duration::from_millis(1_100)), None);
        assert_eq!(
            watch.observe(t0 + Duration::from_secs(3)),
            None,
            "a slow tick is still a tick"
        );
    }

    #[test]
    fn a_gap_beyond_the_grace_is_a_resume_and_reports_its_length() {
        let mut watch = watch();
        let t0 = SystemTime::now();
        watch.observe(t0);
        let gap = watch
            .observe(t0 + Duration::from_secs(600))
            .expect("a ten-minute gap is a sleep");
        assert_eq!(gap, Duration::from_secs(600));
        assert_eq!(
            watch.observe(t0 + Duration::from_secs(601)),
            None,
            "the tick after the wake is ordinary again"
        );
    }

    #[test]
    fn a_backwards_clock_step_only_resynchronises() {
        let mut watch = watch();
        let t0 = SystemTime::now();
        watch.observe(t0);
        assert_eq!(watch.observe(t0 - Duration::from_secs(30)), None);
        assert_eq!(watch.observe(t0 - Duration::from_secs(29)), None);
        assert_eq!(
            watch.observe(t0 - Duration::from_secs(28)),
            None,
            "the watch measures from the corrected clock, not the jump"
        );
    }

    #[test]
    fn a_long_poll_interval_moves_the_threshold_with_it() {
        let mut watch = watch();
        let t0 = SystemTime::now();
        watch.observe(t0);
        watch.set_interval(Duration::from_secs(10));
        assert_eq!(
            watch.observe(t0 + Duration::from_secs(12)),
            None,
            "12 s is one 10 s tick plus grace"
        );
        assert!(
            watch.observe(t0 + Duration::from_secs(30)).is_some(),
            "18 s after the last tick is a suspend"
        );
    }

    #[test]
    fn a_slow_tick_before_the_sleep_is_not_counted_in_the_gap() {
        // The ticker marks the watch, then sleeps. Work done before the mark —
        // a slow register read, say — must not inflate the measured gap.
        let mut watch = ResumeWatch::new(Duration::from_secs(1));
        let t0 = SystemTime::now();
        watch.observe(t0);
        watch.mark_at(t0 + Duration::from_secs(30));
        assert_eq!(
            watch.observe(t0 + Duration::from_secs(31)),
            None,
            "one second after the mark is one ordinary tick"
        );
    }

    #[test]
    fn a_two_second_sleep_is_caught_by_the_default_grace() {
        // This board's S3 blips last about two seconds: a wake-armed device ends
        // the sleep immediately, and that is enough to reset the controller.
        let mut watch = ResumeWatch::new(Duration::from_secs(1));
        let t0 = SystemTime::now();
        watch.observe(t0);
        assert_eq!(
            watch.observe(t0 + Duration::from_millis(1_050)),
            None,
            "an ordinary tick stays ordinary"
        );
        assert!(
            watch.observe(t0 + Duration::from_millis(3_050)).is_some(),
            "2 s of sleep on top of one tick is a suspend, not a slow tick"
        );
    }
}
