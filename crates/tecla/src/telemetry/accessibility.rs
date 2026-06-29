//! Accessibility telemetry.
//!
//! Tracks filter rejection rates, usage patterns, and fatigue indicators
//! for slow keys, bounce keys, and sticky keys. These metrics drive
//! adaptive threshold tuning without inferring health or ability state.

/// Accessibility telemetry collector.
#[derive(Default)]
pub struct AccessibilityTelemetry {
    slow_key_accepts: u32,
    slow_key_rejects: u32,
    bounce_key_rejects: u32,
    sticky_key_uses: u32,
    hold_time_samples: heapless::Deque<u16, 32>,
}

/// Compact accessibility snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct AccessibilitySnapshot {
    /// Slow key rejection rate (0-255 maps to 0-100%).
    pub slow_reject_rate: u8,
    /// Bounce key rejection rate (0-255 maps to 0-100%).
    pub bounce_reject_rate: u8,
    /// Sticky key activations this session.
    pub sticky_uses: u32,
    /// Hold time trend: positive means getting slower (fatigue).
    pub hold_time_trend: i8,
}

impl AccessibilityTelemetry {
    /// Record a slow key press that was accepted (held long enough).
    pub fn record_slow_accept(&mut self) {
        self.slow_key_accepts = self.slow_key_accepts.saturating_add(1);
    }

    /// Record a slow key press that was rejected (released too early).
    pub fn record_slow_reject(&mut self) {
        self.slow_key_rejects = self.slow_key_rejects.saturating_add(1);
    }

    /// Record a bounce key rejection.
    pub fn record_bounce_reject(&mut self) {
        self.bounce_key_rejects = self.bounce_key_rejects.saturating_add(1);
    }

    /// Record a sticky key modifier activation.
    pub fn record_sticky_use(&mut self) {
        self.sticky_key_uses = self.sticky_key_uses.saturating_add(1);
    }

    /// Record a hold time sample for fatigue trend analysis.
    pub fn record_hold_time(&mut self, ms: u16) {
        if self.hold_time_samples.is_full() {
            self.hold_time_samples.pop_front();
        }
        let _ = self.hold_time_samples.push_back(ms);
    }

    /// Produce a snapshot.
    #[must_use]
    pub fn snapshot(&self) -> AccessibilitySnapshot {
        let slow_total = self.slow_key_accepts.saturating_add(self.slow_key_rejects);
        let slow_reject_rate = if slow_total > 0 {
            (u64::from(self.slow_key_rejects) * 255 / u64::from(slow_total)) as u8
        } else {
            0
        };
        let bounce_total = self
            .bounce_key_rejects
            .saturating_add(self.slow_key_accepts);
        let bounce_reject_rate = if bounce_total > 0 {
            (u64::from(self.bounce_key_rejects) * 255 / u64::from(bounce_total)) as u8
        } else {
            0
        };

        AccessibilitySnapshot {
            slow_reject_rate,
            bounce_reject_rate,
            sticky_uses: self.sticky_key_uses,
            hold_time_trend: self.compute_trend(),
        }
    }

    /// Reset delta counters.
    pub fn reset_deltas(&mut self) {}
}

impl super::TelemetryExt for AccessibilityTelemetry {
    type Snapshot = AccessibilitySnapshot;
    fn snapshot_state(&self) -> AccessibilitySnapshot {
        AccessibilityTelemetry::snapshot(self)
    }
    fn clear_deltas(&mut self) {
        AccessibilityTelemetry::reset_deltas(self);
    }
    fn collect(&mut self) -> AccessibilitySnapshot {
        let snap = self.snapshot_state();
        self.clear_deltas();
        snap
    }
}

impl AccessibilityTelemetry {
    fn compute_trend(&self) -> i8 {
        let len = self.hold_time_samples.len();
        if len < 4 {
            return 0;
        }
        let half = len / 2;
        let mut first_sum = 0u32;
        let mut second_sum = 0u32;
        for (i, &sample) in self.hold_time_samples.iter().enumerate() {
            if i < half {
                first_sum += u32::from(sample);
            } else {
                second_sum += u32::from(sample);
            }
        }
        let first_avg = first_sum / half as u32;
        let second_avg = second_sum / (len - half) as u32;
        let diff = second_avg as i32 - first_avg as i32;
        diff.clamp(-128, 127) as i8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_key_rejection_rate() {
        let mut t = AccessibilityTelemetry::default();
        t.record_slow_accept();
        t.record_slow_accept();
        t.record_slow_reject();
        let snap = t.snapshot();
        assert!(snap.slow_reject_rate > 70 && snap.slow_reject_rate < 100);
    }

    #[test]
    fn bounce_rejection_tracking() {
        let mut t = AccessibilityTelemetry::default();
        t.record_bounce_reject();
        t.record_bounce_reject();
        t.record_slow_accept();
        let snap = t.snapshot();
        assert!(snap.bounce_reject_rate > 0);
    }

    #[test]
    fn sticky_key_counting() {
        let mut t = AccessibilityTelemetry::default();
        t.record_sticky_use();
        t.record_sticky_use();
        assert_eq!(t.snapshot().sticky_uses, 2);
    }

    #[test]
    fn fatigue_trend_positive_when_slowing() {
        let mut t = AccessibilityTelemetry::default();
        for ms in [50, 55, 52, 48, 80, 85, 90, 95] {
            t.record_hold_time(ms);
        }
        let trend = t.snapshot().hold_time_trend;
        assert!(trend > 0, "trend should be positive (slowing), got {trend}");
    }

    #[test]
    fn fatigue_trend_negative_when_speeding() {
        let mut t = AccessibilityTelemetry::default();
        for ms in [90, 85, 88, 92, 50, 48, 52, 45] {
            t.record_hold_time(ms);
        }
        let trend = t.snapshot().hold_time_trend;
        assert!(
            trend < 0,
            "trend should be negative (speeding up), got {trend}"
        );
    }

    #[test]
    fn trend_zero_with_few_samples() {
        let mut t = AccessibilityTelemetry::default();
        t.record_hold_time(100);
        assert_eq!(t.snapshot().hold_time_trend, 0);
    }

    #[test]
    fn empty_snapshot_is_zero() {
        let t = AccessibilityTelemetry::default();
        let snap = t.snapshot();
        assert_eq!(snap.slow_reject_rate, 0);
        assert_eq!(snap.bounce_reject_rate, 0);
        assert_eq!(snap.sticky_uses, 0);
        assert_eq!(snap.hold_time_trend, 0);
    }
}
