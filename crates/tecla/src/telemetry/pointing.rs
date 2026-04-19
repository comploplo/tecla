//! Pointing device telemetry.
//!
//! Tracks movement velocity, click patterns, and scroll usage for
//! trackballs, trackpads, and other pointing devices.

/// Pointing device telemetry collector.
#[derive(Default)]
pub struct PointingTelemetry {
    total_clicks: u32,
    click_while_moving: u32,
    scroll_count: u32,
    movement_samples: u32,
    velocity_sum: u32,
    delta_clicks: u16,
}

/// Compact pointing snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct PointingSnapshot {
    /// Average movement velocity (arbitrary units).
    pub avg_velocity: u16,
    /// Click-while-moving rate (0-255 maps to 0-100%).
    pub click_while_moving_rate: u8,
    /// Total clicks this session.
    pub total_clicks: u32,
    /// Scroll events this session.
    pub scroll_count: u32,
    /// Clicks since last snapshot.
    pub delta_clicks: u16,
}

impl PointingTelemetry {
    /// Record a movement sample.
    pub fn record_movement(&mut self, dx: i8, dy: i8) {
        let speed = dx.unsigned_abs().saturating_add(dy.unsigned_abs());
        self.velocity_sum = self.velocity_sum.saturating_add(u32::from(speed));
        self.movement_samples = self.movement_samples.saturating_add(1);
    }

    /// Record a click event.
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
        let avg_velocity = if self.movement_samples > 0 {
            (self.velocity_sum / self.movement_samples) as u16
        } else {
            0
        };
        let click_while_moving_rate = if self.total_clicks > 0 {
            (u64::from(self.click_while_moving) * 255 / u64::from(self.total_clicks)) as u8
        } else {
            0
        };
        PointingSnapshot {
            avg_velocity,
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
    fn movement_velocity() {
        let mut t = PointingTelemetry::default();
        t.record_movement(10, 5);
        t.record_movement(-4, 6);
        let snap = t.snapshot();
        assert_eq!(snap.avg_velocity, 12);
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
        assert_eq!(snap.avg_velocity, 0);
        assert_eq!(snap.total_clicks, 0);
    }
}
