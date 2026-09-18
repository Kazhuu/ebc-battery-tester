use crate::device::OutboundFrame;
use web_time::{Duration, Instant};

/// Decides when a `TimerSync` frame is due, based on real elapsed time.
/// Driven directly by a worker's own tick loop, independent of whatever the
/// UI's paint loop is doing. Mirrors the start/continue/stop lifecycle of
/// `DeviceSession`'s mode tracking, but with its own clock.
pub struct TimerSync {
    /// When the current run segment started. `None` while stopped, so a
    /// `check()` during a stop can't mistake real elapsed time for running
    /// time.
    start_time: Option<Instant>,
    /// Running time already banked from segments before this one, i.e. time
    /// accumulated across a stop/continue cycle.
    accumulated: Duration,
    /// Highest minute count already sent to the device.
    last_synced_min: u64,
}

impl TimerSync {
    pub(crate) fn new() -> Self {
        Self {
            start_time: None,
            accumulated: Duration::ZERO,
            last_synced_min: 0,
        }
    }

    /// Mode started fresh, reset the timers.
    pub(crate) fn mode_started(&mut self, now: Instant) {
        self.start_time = Some(now);
        self.accumulated = Duration::ZERO;
        self.last_synced_min = 0;
    }

    /// Mode resumed keep the banked time, just restart the running clock.
    pub(crate) fn mode_continued(&mut self, now: Instant) {
        self.start_time = Some(now);
    }

    /// Mode stopped, fold the segment that just ended into the banked total, so
    /// time spent stopped afterwards never counts as elapsed.
    pub(crate) fn mode_stopped(&mut self, now: Instant) {
        if let Some(start) = self.start_time.take() {
            self.accumulated += now.saturating_duration_since(start);
        }
    }

    /// Call on every worker tick while a mode is running. Returns the frame
    /// to send if a new minute boundary has been crossed since the last call.
    pub(crate) fn check(&mut self, now: Instant) -> Option<OutboundFrame> {
        let running_segment = self
            .start_time
            .map_or(Duration::ZERO, |start| now.saturating_duration_since(start));
        let elapsed = self.accumulated + running_segment;
        let elapsed_mins = elapsed.as_secs() / 60;
        if elapsed_mins > self.last_synced_min {
            self.last_synced_min = elapsed_mins;
            Some(OutboundFrame::TimerSync(elapsed_mins as u16))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use web_time::Duration;

    #[test]
    fn check_does_not_fire_before_a_minute_has_elapsed() {
        let mut timer_sync = TimerSync::new();
        let start = Instant::now();
        timer_sync.mode_started(start);

        let result = timer_sync.check(start + Duration::from_secs(30));

        assert!(result.is_none());
    }

    #[test]
    fn check_fires_once_a_minute_boundary_is_crossed() {
        let mut timer_sync = TimerSync::new();
        let start = Instant::now();
        timer_sync.mode_started(start);

        let result = timer_sync.check(start + Duration::from_secs(61));

        assert!(matches!(result, Some(OutboundFrame::TimerSync(1))));
    }

    #[test]
    fn check_does_not_fire_twice_for_the_same_minute() {
        let mut timer_sync = TimerSync::new();
        let start = Instant::now();
        timer_sync.mode_started(start);
        timer_sync.check(start + Duration::from_secs(61));

        let result = timer_sync.check(start + Duration::from_secs(65));

        assert!(result.is_none());
    }

    #[test]
    fn check_advances_to_the_next_minute_boundary() {
        let mut timer_sync = TimerSync::new();
        let start = Instant::now();
        timer_sync.mode_started(start);
        timer_sync.check(start + Duration::from_secs(61));

        let result = timer_sync.check(start + Duration::from_secs(125));

        assert!(matches!(result, Some(OutboundFrame::TimerSync(2))));
    }

    /// Regression test: however long the mode stays stopped, elapsed running
    /// time must not include that gap, so the first sync after resuming
    /// should just be "last synced + 1", not a jump reflecting the real
    /// wall-clock time spent stopped.
    #[test]
    fn stopping_freezes_progress_no_matter_how_long_it_stays_stopped() {
        let mut timer_sync = TimerSync::new();
        let start = Instant::now();
        timer_sync.mode_started(start);

        // 1 minute, 30 seconds of running time before stopping.
        let stop_time = start + Duration::from_secs(90);
        timer_sync.check(stop_time);
        timer_sync.mode_stopped(stop_time);

        // Stay stopped for 2 hours of real time. Nothing should ever fire
        // while stopped.
        let still_stopped = stop_time + Duration::from_secs(2 * 60 * 60);
        assert!(timer_sync.check(still_stopped).is_none());

        // Resume. Real wall-clock time has moved far ahead, but only 90
        // seconds of *running* time has actually elapsed so far.
        timer_sync.mode_continued(still_stopped);

        // Not yet another 30s of running time since resuming (90 + 29 = 119s),
        // so still short of the 2-minute mark.
        let too_soon = timer_sync.check(still_stopped + Duration::from_secs(29));
        assert!(too_soon.is_none());

        // Now 120s of running time has passed (90 + 31 = 121s) -> minute 2,
        // i.e. exactly "last synced (1) + 1", regardless of the 2 real hours
        // spent stopped in between.
        let result = timer_sync.check(still_stopped + Duration::from_secs(31));
        assert!(matches!(result, Some(OutboundFrame::TimerSync(2))));
    }

    #[test]
    fn starting_a_fresh_mode_resets_the_synced_minute_counter() {
        let mut timer_sync = TimerSync::new();
        let start = Instant::now();
        timer_sync.mode_started(start);
        timer_sync.check(start + Duration::from_secs(61)); // minute 1 synced

        let restart = start + Duration::from_secs(500);
        timer_sync.mode_started(restart);

        // Barely any time has passed since the fresh start, so nothing
        // should fire yet even though minute 1 was already synced before.
        let result = timer_sync.check(restart + Duration::from_secs(5));
        assert!(result.is_none());
    }
}
