//! Accessibility telemetry.
//!
//! Tracks filter rejection rates, usage patterns, and hold-time changes
//! for slow keys, bounce keys, and sticky keys. Timing changes alone do not
//! establish fatigue or a change in ability.

/// Accessibility telemetry collector.
///
/// Each filter outcome is counted twice: a cumulative total for the session
/// and a delta that [`reset_deltas`](Self::reset_deltas) clears after every
/// snapshot. Rates are reported over the delta window, so adaptation sees
/// recent behavior rather than a lifetime average that stops moving.
#[derive(Default)]
pub struct AccessibilityTelemetry {
    slow_key_accepts: u32,
    slow_key_rejects: u32,
    bounce_key_rejects: u32,
    sticky_key_uses: u32,
    delta_slow_accepts: u16,
    delta_slow_rejects: u16,
    delta_bounce_rejects: u16,
    delta_sticky_uses: u16,
    hold_time_samples: heapless::Deque<u16, 32>,
}

/// Compact accessibility snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct AccessibilitySnapshot {
    /// Slow key rejection rate since the last snapshot (0-255 maps to 0-100%).
    pub slow_reject_rate: u8,
    /// Bounce key rejection rate since the last snapshot (0-255 maps to 0-100%).
    pub bounce_reject_rate: u8,
    /// Sticky key activations this session.
    pub sticky_uses: u32,
    /// Sticky key activations since the last snapshot.
    pub delta_sticky_uses: u16,
    /// Hold-time trend: positive means longer recent holds than earlier holds.
    pub hold_time_trend: i8,
}

impl AccessibilityTelemetry {
    /// Record a slow key press that was accepted (held long enough).
    pub fn record_slow_accept(&mut self) {
        self.slow_key_accepts = self.slow_key_accepts.saturating_add(1);
        self.delta_slow_accepts = self.delta_slow_accepts.saturating_add(1);
    }

    /// Record a slow key press that was rejected (released too early).
    pub fn record_slow_reject(&mut self) {
        self.slow_key_rejects = self.slow_key_rejects.saturating_add(1);
        self.delta_slow_rejects = self.delta_slow_rejects.saturating_add(1);
    }

    /// Record a bounce key rejection.
    pub fn record_bounce_reject(&mut self) {
        self.bounce_key_rejects = self.bounce_key_rejects.saturating_add(1);
        self.delta_bounce_rejects = self.delta_bounce_rejects.saturating_add(1);
    }

    /// Record a sticky key modifier activation.
    pub fn record_sticky_use(&mut self) {
        self.sticky_key_uses = self.sticky_key_uses.saturating_add(1);
        self.delta_sticky_uses = self.delta_sticky_uses.saturating_add(1);
    }

    /// Record a hold-time sample for timing trend analysis.
    pub fn record_hold_time(&mut self, ms: u16) {
        if self.hold_time_samples.is_full() {
            self.hold_time_samples.pop_front();
        }
        let _ = self.hold_time_samples.push_back(ms);
    }

    /// Produce a snapshot.
    #[must_use]
    pub fn snapshot(&self) -> AccessibilitySnapshot {
        // Slow-key denominator: presses that reached the slow filter and
        // resolved either way. A bounce reject never got that far.
        let slow_total = self
            .delta_slow_accepts
            .saturating_add(self.delta_slow_rejects);

        // Bounce denominator: every press attempt. The bounce filter runs
        // first, so a press that went on to be slow-rejected is still one the
        // bounce filter let through — omitting those understates the rate.
        let bounce_total = slow_total.saturating_add(self.delta_bounce_rejects);

        AccessibilitySnapshot {
            slow_reject_rate: rate(self.delta_slow_rejects, slow_total),
            bounce_reject_rate: rate(self.delta_bounce_rejects, bounce_total),
            sticky_uses: self.sticky_key_uses,
            delta_sticky_uses: self.delta_sticky_uses,
            hold_time_trend: self.compute_trend(),
        }
    }

    /// Reset delta counters after a snapshot has been sent.
    ///
    /// Cumulative totals are left alone; only the rate windows restart.
    pub fn reset_deltas(&mut self) {
        self.delta_slow_accepts = 0;
        self.delta_slow_rejects = 0;
        self.delta_bounce_rejects = 0;
        self.delta_sticky_uses = 0;
    }
}

/// Scales `part / total` into 0-255, returning 0 for an empty window.
fn rate(part: u16, total: u16) -> u8 {
    if total == 0 {
        return 0;
    }
    (u32::from(part) * 255 / u32::from(total)) as u8
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
        // 1 reject of 3 slow-filtered presses -> 1 * 255 / 3.
        assert_eq!(t.snapshot().slow_reject_rate, 85);
    }

    #[test]
    fn bounce_rejection_tracking() {
        let mut t = AccessibilityTelemetry::default();
        t.record_bounce_reject();
        t.record_bounce_reject();
        t.record_slow_accept();
        // 2 rejects of 3 press attempts -> 2 * 255 / 3.
        assert_eq!(t.snapshot().bounce_reject_rate, 170);
    }

    #[test]
    fn bounce_denominator_counts_slow_rejects_as_attempts() {
        // A slow-rejected press cleared the bounce filter, so it belongs in
        // the bounce denominator. Leaving it out inflated the rate.
        let mut t = AccessibilityTelemetry::default();
        t.record_bounce_reject();
        t.record_slow_accept();
        t.record_slow_reject();
        // 1 of 3, not 1 of 2.
        assert_eq!(t.snapshot().bounce_reject_rate, 85);
    }

    #[test]
    fn reset_deltas_restarts_the_rate_window() {
        let mut t = AccessibilityTelemetry::default();
        t.record_slow_reject();
        t.record_sticky_use();
        assert_eq!(t.snapshot().slow_reject_rate, 255);

        t.reset_deltas();
        let snap = t.snapshot();
        assert_eq!(snap.slow_reject_rate, 0, "empty window reads 0");
        assert_eq!(snap.delta_sticky_uses, 0);
        assert_eq!(snap.sticky_uses, 1, "cumulative total survives the reset");

        // A clean window reflects only what happened after the reset.
        t.record_slow_accept();
        t.record_slow_accept();
        t.record_slow_reject();
        assert_eq!(t.snapshot().slow_reject_rate, 85);
    }

    #[test]
    fn collect_snapshots_then_clears() {
        use crate::telemetry::TelemetryExt;
        let mut t = AccessibilityTelemetry::default();
        t.record_sticky_use();
        assert_eq!(t.collect().delta_sticky_uses, 1);
        assert_eq!(t.collect().delta_sticky_uses, 0);
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
