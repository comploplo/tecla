//! Pointing device telemetry.
//!
//! Tracks movement counts per sample, button presses, and scroll usage for
//! trackballs, trackpads, and other pointing devices.

/// Pointing device telemetry collector.
#[derive(Default)]
pub struct PointingTelemetry {
    total_clicks: u32,
    click_while_moving: u32,
    scroll_count: u32,
    movement_samples: u64,
    movement_sum: u64,
    delta_clicks: u16,
}

/// Compact pointing snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct PointingSnapshot {
    /// Mean `|dx| + |dy|` counts per recorded movement sample, capped at `u16::MAX`.
    pub mean_counts_per_sample: u16,
    /// Fraction of presses marked coincident with movement (0–255), not an error rate.
    pub click_while_moving_rate: u8,
    /// Total button-down edges this session; simultaneous presses count separately.
    pub total_clicks: u32,
    /// Scroll events this session.
    pub scroll_count: u32,
    /// Clicks since last snapshot.
    pub delta_clicks: u16,
}

impl PointingTelemetry {
    /// Record full-width movement counts, including either axis's negative extreme.
    pub fn record_movement(&mut self, dx: i16, dy: i16) {
        let distance = u64::from(dx.unsigned_abs()) + u64::from(dy.unsigned_abs());
        // Preserve the collected mean if either accumulator ever exhausts its range.
        let (Some(sum), Some(samples)) = (
            self.movement_sum.checked_add(distance),
            self.movement_samples.checked_add(1),
        ) else {
            return;
        };
        self.movement_sum = sum;
        self.movement_samples = samples;
    }

    /// Record one button-down edge, rather than a report with a held button.
    /// `during_movement` is a caller-defined coincidence, not a measure of intent.
    pub fn record_click(&mut self, during_movement: bool) {
        self.total_clicks = self.total_clicks.saturating_add(1);
        self.delta_clicks = self.delta_clicks.saturating_add(1);
        if during_movement {
            self.click_while_moving = self.click_while_moving.saturating_add(1);
        }
    }

    /// Record a scroll event.
    pub fn record_scroll(&mut self) {
        self.scroll_count = self.scroll_count.saturating_add(1);
    }

    /// Produce a snapshot.
    #[must_use]
    pub fn snapshot(&self) -> PointingSnapshot {
        let mean = self
            .movement_sum
            .checked_div(self.movement_samples)
            .unwrap_or(0);
        let mean_counts_per_sample = u16::try_from(mean).unwrap_or(u16::MAX);
        let click_while_moving_rate = if self.total_clicks > 0 {
            (u64::from(self.click_while_moving) * 255 / u64::from(self.total_clicks)) as u8
        } else {
            0
        };
        PointingSnapshot {
            mean_counts_per_sample,
            click_while_moving_rate,
            total_clicks: self.total_clicks,
            scroll_count: self.scroll_count,
            delta_clicks: self.delta_clicks,
        }
    }

    /// Reset delta counters.
    pub fn reset_deltas(&mut self) {
        self.delta_clicks = 0;
    }
}

impl super::TelemetryExt for PointingTelemetry {
    type Snapshot = PointingSnapshot;
    fn snapshot_state(&self) -> PointingSnapshot {
        PointingTelemetry::snapshot(self)
    }
    fn clear_deltas(&mut self) {
        PointingTelemetry::reset_deltas(self);
    }
    fn collect(&mut self) -> PointingSnapshot {
        let snap = self.snapshot_state();
        self.clear_deltas();
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_counts_per_movement_sample() {
        let mut t = PointingTelemetry::default();
        t.record_movement(10, 5);
        t.record_movement(-4, 6);
        let snap = t.snapshot();
        assert_eq!(snap.mean_counts_per_sample, 12);
    }

    #[test]
    fn full_width_extremes_are_retained_before_snapshot_clamping() {
        let mut t = PointingTelemetry::default();
        t.record_movement(i16::MIN, i16::MIN);
        assert_eq!(t.snapshot().mean_counts_per_sample, u16::MAX);
        t.record_movement(0, 0);
        assert_eq!(t.snapshot().mean_counts_per_sample, 32_768);
    }

    #[test]
    fn accumulated_counts_can_exceed_u32_without_distorting_the_mean() {
        let mut t = PointingTelemetry::default();
        for _ in 0..100_000 {
            t.record_movement(30_000, -30_000);
        }
        assert_eq!(t.snapshot().mean_counts_per_sample, 60_000);
    }

    #[test]
    fn accumulator_exhaustion_preserves_both_halves_of_the_mean() {
        for (sum, samples) in [(u64::MAX, 1), (1, u64::MAX)] {
            let mut t = PointingTelemetry {
                movement_sum: sum,
                movement_samples: samples,
                ..PointingTelemetry::default()
            };
            t.record_movement(1, 1);
            assert_eq!(t.movement_sum, sum);
            assert_eq!(t.movement_samples, samples);
        }
    }

    #[test]
    fn click_while_moving() {
        let mut t = PointingTelemetry::default();
        t.record_click(true);
        t.record_click(false);
        t.record_click(false);
        let snap = t.snapshot();
        assert!(snap.click_while_moving_rate > 70 && snap.click_while_moving_rate < 100);
    }

    #[test]
    fn scroll_counting() {
        let mut t = PointingTelemetry::default();
        t.record_scroll();
        t.record_scroll();
        assert_eq!(t.snapshot().scroll_count, 2);
    }

    #[test]
    fn empty_snapshot() {
        let t = PointingTelemetry::default();
        let snap = t.snapshot();
        assert_eq!(snap.mean_counts_per_sample, 0);
        assert_eq!(snap.total_clicks, 0);
    }
}
