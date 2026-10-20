//! Encoder telemetry.
//!
//! Tracks rotation velocity, direction reversals, and per-encoder usage
//! balance for rotary encoders.

/// Encoder telemetry collector.
pub struct EncoderTelemetry<const ENCODERS: usize> {
    cw_count: [u32; ENCODERS],
    ccw_count: [u32; ENCODERS],
    reversals: [u16; ENCODERS],
    last_direction: [Option<bool>; ENCODERS],
    delta_ticks: u16,
}

/// Compact encoder snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct EncoderSnapshot {
    /// Total clockwise ticks across all encoders.
    pub total_cw: u32,
    /// Total counter-clockwise ticks across all encoders.
    pub total_ccw: u32,
    /// Total direction reversals across all encoders.
    pub total_reversals: u16,
    /// Ticks since last snapshot.
    pub delta_ticks: u16,
}

impl<const ENCODERS: usize> EncoderTelemetry<ENCODERS> {
    /// Creates a new encoder telemetry collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cw_count: [0; ENCODERS],
            ccw_count: [0; ENCODERS],
            reversals: [0; ENCODERS],
            last_direction: [None; ENCODERS],
            delta_ticks: 0,
        }
    }

    /// Record a clockwise tick on the given encoder.
    pub fn record_cw(&mut self, encoder_id: u8) {
        let idx = usize::from(encoder_id);
        if idx >= ENCODERS {
            return;
        }
        self.cw_count[idx] = self.cw_count[idx].saturating_add(1);
        self.delta_ticks = self.delta_ticks.saturating_add(1);
        if self.last_direction[idx] == Some(false) {
            self.reversals[idx] = self.reversals[idx].saturating_add(1);
        }
        self.last_direction[idx] = Some(true);
    }

    /// Record a counter-clockwise tick on the given encoder.
    pub fn record_ccw(&mut self, encoder_id: u8) {
        let idx = usize::from(encoder_id);
        if idx >= ENCODERS {
            return;
        }
        self.ccw_count[idx] = self.ccw_count[idx].saturating_add(1);
        self.delta_ticks = self.delta_ticks.saturating_add(1);
        if self.last_direction[idx] == Some(true) {
            self.reversals[idx] = self.reversals[idx].saturating_add(1);
        }
        self.last_direction[idx] = Some(false);
    }

    /// Produce a snapshot.
    #[must_use]
    pub fn snapshot(&self) -> EncoderSnapshot {
        EncoderSnapshot {
            total_cw: self.cw_count.iter().sum(),
            total_ccw: self.ccw_count.iter().sum(),
            total_reversals: self.reversals.iter().sum(),
            delta_ticks: self.delta_ticks,
        }
    }

    /// Reset delta counters.
    pub fn reset_deltas(&mut self) {
        self.delta_ticks = 0;
    }

    /// Returns per-encoder clockwise counts.
    #[must_use]
    pub fn cw_counts(&self) -> &[u32; ENCODERS] {
        &self.cw_count
    }

    /// Returns per-encoder counter-clockwise counts.
    #[must_use]
    pub fn ccw_counts(&self) -> &[u32; ENCODERS] {
        &self.ccw_count
    }
}

impl<const ENCODERS: usize> Default for EncoderTelemetry<ENCODERS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const ENCODERS: usize> super::TelemetryExt for EncoderTelemetry<ENCODERS> {
    type Snapshot = EncoderSnapshot;
    fn snapshot_state(&self) -> EncoderSnapshot {
        EncoderTelemetry::snapshot(self)
    }
    fn clear_deltas(&mut self) {
        EncoderTelemetry::reset_deltas(self);
    }
    fn collect(&mut self) -> EncoderSnapshot {
        let snap = self.snapshot_state();
        self.clear_deltas();
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_counting() {
        let mut t = EncoderTelemetry::<2>::new();
        t.record_cw(0);
        t.record_cw(0);
        t.record_ccw(1);
        let snap = t.snapshot();
        assert_eq!(snap.total_cw, 2);
        assert_eq!(snap.total_ccw, 1);
        assert_eq!(snap.delta_ticks, 3);
    }

    #[test]
    fn reversal_detection() {
        let mut t = EncoderTelemetry::<1>::new();
        t.record_cw(0);
        t.record_cw(0);
        t.record_ccw(0);
        t.record_cw(0);
        assert_eq!(t.snapshot().total_reversals, 2);
    }

    #[test]
    fn no_reversal_on_same_direction() {
        let mut t = EncoderTelemetry::<1>::new();
        t.record_cw(0);
        t.record_cw(0);
        t.record_cw(0);
        assert_eq!(t.snapshot().total_reversals, 0);
    }

    #[test]
    fn out_of_range_encoder_is_safe() {
        let mut t = EncoderTelemetry::<2>::new();
        t.record_cw(5);
        assert_eq!(t.snapshot().total_cw, 0);
    }

    #[test]
    fn delta_reset() {
        let mut t = EncoderTelemetry::<1>::new();
        t.record_cw(0);
        t.reset_deltas();
        assert_eq!(t.delta_ticks, 0);
        assert_eq!(t.cw_count[0], 1);
    }

    #[test]
    fn per_encoder_counts_are_kept_separately() {
        let mut t = EncoderTelemetry::<3>::default();
        t.record_cw(0);
        t.record_cw(0);
        t.record_ccw(2);
        assert_eq!(t.cw_counts(), &[2, 0, 0]);
        assert_eq!(t.ccw_counts(), &[0, 0, 1]);
    }

    #[test]
    fn out_of_range_encoder_ids_are_ignored() {
        let mut t = EncoderTelemetry::<2>::new();
        t.record_cw(9);
        t.record_ccw(9);
        assert_eq!(t.cw_counts(), &[0, 0]);
        assert_eq!(t.ccw_counts(), &[0, 0]);
    }

    #[test]
    fn default_matches_new() {
        let a = EncoderTelemetry::<4>::new();
        let b = EncoderTelemetry::<4>::default();
        assert_eq!(a.cw_counts(), b.cw_counts());
        assert_eq!(a.ccw_counts(), b.ccw_counts());
    }
}
