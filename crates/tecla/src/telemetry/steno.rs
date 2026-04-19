//! Steno-specific telemetry.
//!
//! Tracks chord accuracy, dictionary hit rates, retry patterns, and
//! multi-stroke completion — the metrics that matter for steno learning
//! and dictionary optimization.

/// Steno telemetry collector.
pub struct StenoTelemetry {
    dict_hits: u32,
    dict_misses: u32,
    retries: u32,
    undos: u32,
    multi_stroke_starts: u32,
    multi_stroke_completions: u32,
    total_strokes: u32,
    delta_strokes: u16,
    spm_ring: heapless::Deque<u64, 30>,
}

impl Default for StenoTelemetry {
    fn default() -> Self {
        Self {
            dict_hits: 0,
            dict_misses: 0,
            retries: 0,
            undos: 0,
            multi_stroke_starts: 0,
            multi_stroke_completions: 0,
            total_strokes: 0,
            delta_strokes: 0,
            spm_ring: heapless::Deque::new(),
        }
    }
}

/// Compact steno snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct StenoSnapshot {
    /// Strokes per minute.
    pub spm: u16,
    /// Dictionary hit rate (0-255 maps to 0-100%).
    pub hit_rate: u8,
    /// Retry rate (0-255 maps to 0-100%).
    pub retry_rate: u8,
    /// Multi-stroke completion rate (0-255 maps to 0-100%).
    pub completion_rate: u8,
    /// Undo count this session.
    pub undos: u32,
    /// Strokes since last snapshot.
    pub delta_strokes: u16,
}

impl StenoTelemetry {
    /// Record a stroke that matched a dictionary entry.
    pub fn record_hit(&mut self) {
        self.dict_hits = self.dict_hits.saturating_add(1);
        self.record_stroke();
    }

    /// Record a stroke that did not match any dictionary entry.
    pub fn record_miss(&mut self) {
        self.dict_misses = self.dict_misses.saturating_add(1);
        self.record_stroke();
    }

    /// Record a stroke timestamp for SPM calculation.
    pub fn record_stroke_time(&mut self, now_ms: u64) {
        if self.spm_ring.is_full() {
            self.spm_ring.pop_front();
        }
        let _ = self.spm_ring.push_back(now_ms);
    }

    /// Compute strokes per minute from the sliding window.
    #[must_use]
    pub fn spm(&self) -> u16 {
        let count = self.spm_ring.len();
        if count < 2 {
            return 0;
        }
        let Some(&newest) = self.spm_ring.back() else {
            return 0;
        };
        let Some(&oldest) = self.spm_ring.front() else {
            return 0;
        };
        let elapsed_ms = newest.saturating_sub(oldest);
        if elapsed_ms == 0 {
            return 0;
        }
        ((count as u64).saturating_sub(1) * 60_000 / elapsed_ms) as u16
    }

    fn record_stroke(&mut self) {
        self.total_strokes = self.total_strokes.saturating_add(1);
        self.delta_strokes = self.delta_strokes.saturating_add(1);
    }

    /// Record an undo action (user correcting a mistroke).
    pub fn record_undo(&mut self) {
        self.undos = self.undos.saturating_add(1);
        self.retries = self.retries.saturating_add(1);
    }

    /// Record the start of a multi-stroke sequence.
    pub fn record_multi_stroke_start(&mut self) {
        self.multi_stroke_starts = self.multi_stroke_starts.saturating_add(1);
    }

    /// Record a multi-stroke sequence completing successfully.
    pub fn record_multi_stroke_complete(&mut self) {
        self.multi_stroke_completions = self.multi_stroke_completions.saturating_add(1);
    }

    /// Produce a snapshot.
    #[must_use]
    pub fn snapshot(&self) -> StenoSnapshot {
        let total = self.dict_hits.saturating_add(self.dict_misses);
        let hit_rate = if total > 0 {
            (u64::from(self.dict_hits) * 255 / u64::from(total)) as u8
        } else {
            0
        };
        let retry_rate = if self.total_strokes > 0 {
            (u64::from(self.retries) * 255 / u64::from(self.total_strokes)) as u8
        } else {
            0
        };
        let completion_rate = if self.multi_stroke_starts > 0 {
            (u64::from(self.multi_stroke_completions) * 255 / u64::from(self.multi_stroke_starts))
                as u8
        } else {
            0
        };
        StenoSnapshot {
            spm: self.spm(),
            hit_rate,
            retry_rate,
            completion_rate,
            undos: self.undos,
            delta_strokes: self.delta_strokes,
        }
    }

    /// Reset delta counters after snapshot is sent.
    pub fn reset_deltas(&mut self) {
        self.delta_strokes = 0;
    }
}

impl super::TelemetryExt for StenoTelemetry {
    type Snapshot = StenoSnapshot;

    fn snapshot_state(&self) -> StenoSnapshot {
        StenoTelemetry::snapshot(self)
    }

    fn clear_deltas(&mut self) {
        StenoTelemetry::reset_deltas(self);
    }

    fn collect(&mut self) -> StenoSnapshot {
        let snap = self.snapshot_state();
        self.clear_deltas();
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_miss_tracking() {
        let mut t = StenoTelemetry::default();
        t.record_hit();
        t.record_hit();
        t.record_miss();
        let snap = t.snapshot();
        assert!(snap.hit_rate > 150);
        assert_eq!(snap.delta_strokes, 3);
    }

    #[test]
    fn undo_increments_retry() {
        let mut t = StenoTelemetry::default();
        t.record_hit();
        t.record_undo();
        let snap = t.snapshot();
        assert!(snap.retry_rate > 0);
        assert_eq!(snap.undos, 1);
    }

    #[test]
    fn multi_stroke_completion() {
        let mut t = StenoTelemetry::default();
        t.record_multi_stroke_start();
        t.record_multi_stroke_start();
        t.record_multi_stroke_complete();
        let snap = t.snapshot();
        assert!(snap.completion_rate > 100 && snap.completion_rate < 150);
    }

    #[test]
    fn empty_snapshot_is_zero() {
        let t = StenoTelemetry::default();
        let snap = t.snapshot();
        assert_eq!(snap.hit_rate, 0);
        assert_eq!(snap.retry_rate, 0);
        assert_eq!(snap.completion_rate, 0);
    }

    #[test]
    fn reset_clears_deltas_only() {
        let mut t = StenoTelemetry::default();
        t.record_hit();
        t.reset_deltas();
        assert_eq!(t.delta_strokes, 0);
        assert_eq!(t.dict_hits, 1);
    }
}
