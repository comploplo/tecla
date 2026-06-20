//! Live telemetry collection for adaptive input research.
//!
//! The [`TelemetryExt`] trait is the generic extension point — users building
//! novel controllers implement it and get the full collection/snapshot/transport
//! pipeline for free. The `()` impl is a zero-cost no-op.
//!
//! [`CoreTelemetry`] collects the canonical keystroke dynamics features
//! identified by CMU, Aalto (136M keystrokes, CHI 2018), and BiDAlab:
//! per-key hold time, inter-key latencies, rollover ratio, WPM, and error rate.
//!
//! Modality-specific telemetry modules:
//!
//! - [`steno`] — chord accuracy, dictionary hit rates, retry patterns
//! - [`accessibility`] — filter rejection rates, fatigue trend detection
//! - [`midi`] — velocity distributions, polyphony depth, CC utilization
//! - [`gamepad`] — axis range, button combos, simultaneous press tracking
//! - [`pointing`] — movement velocity, click-while-moving, scroll usage
//! - [`encoder`] — rotation velocity, direction reversals, per-encoder balance

use crate::pipeline::event::KeyPos;

/// Shared interface for telemetry modalities.
///
/// Every telemetry collector implements this trait to provide the transport
/// layer with a uniform way to capture snapshots and reset delta counters.
/// Domain-specific recording methods (e.g. `record_hit`, `record_movement`)
/// remain as concrete methods on each type.
pub trait TelemetryExt: Default {
    /// A compact snapshot suitable for transport over USB.
    type Snapshot: Copy + Default;

    /// Produce a compact snapshot of current state.
    fn snapshot_state(&self) -> Self::Snapshot;
    /// Reset delta counters (called after each snapshot is sent).
    fn clear_deltas(&mut self);
    /// Snapshot current state and reset deltas in one call.
    fn collect(&mut self) -> Self::Snapshot {
        let snap = self.snapshot_state();
        self.clear_deltas();
        snap
    }
}

impl TelemetryExt for () {
    type Snapshot = ();

    fn snapshot_state(&self) -> Self::Snapshot {}
    fn clear_deltas(&mut self) {}
}

/// Core keystroke dynamics telemetry.
pub struct CoreTelemetry<const KEYS: usize> {
    press_count: [u32; KEYS],
    error_count: [u16; KEYS],
    hold_time_ema: [u16; KEYS],
    hold_time_var: [u16; KEYS],
    press_timestamps: [u64; KEYS],
    prev_press_pos: Option<u8>,
    last_press_pos: Option<u8>,
    last_press_ms: u64,
    last_release_ms: u64,
    rollover_count: u32,
    total_presses: u32,
    delta_presses: u16,
    wpm_ring: heapless::Deque<u64, 50>,
    bigrams: [BigramStat; 64],
    bigram_count: u8,
    enabled: bool,
}

/// Per-bigram inter-key interval statistics.
#[derive(Clone, Copy, Default)]
pub struct BigramStat {
    /// Source key (flat index).
    pub from: u8,
    /// Destination key (flat index).
    pub to: u8,
    /// Press-to-press latency exponential moving average (ms).
    pub pp_ema: u16,
    /// Observation count.
    pub count: u16,
}

/// Compact snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct CoreSnapshot {
    /// Words per minute (chars/5 per minute).
    pub wpm: u16,
    /// Rollover ratio (0-255, maps to 0-100%).
    pub rollover_ratio: u8,
    /// Total presses this session.
    pub total_presses: u32,
    /// Presses since last snapshot.
    pub delta_presses: u16,
    /// Average hold time across all keys (ms).
    pub avg_hold_ms: u16,
    /// Average press-to-press latency (ms).
    pub avg_pp_ms: u16,
    /// Errors per 100 keypresses.
    pub error_rate: u8,
}

impl<const KEYS: usize> CoreTelemetry<KEYS> {
    /// Creates a new telemetry collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            press_count: [0; KEYS],
            error_count: [0; KEYS],
            hold_time_ema: [0; KEYS],
            hold_time_var: [0; KEYS],
            press_timestamps: [0; KEYS],
            prev_press_pos: None,
            last_press_pos: None,
            last_press_ms: 0,
            last_release_ms: 0,
            rollover_count: 0,
            total_presses: 0,
            delta_presses: 0,
            wpm_ring: heapless::Deque::new(),
            bigrams: [BigramStat::default(); 64],
            bigram_count: 0,
            enabled: true,
        }
    }

    /// Returns whether telemetry collection is enabled.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Toggles telemetry collection on or off.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Record a key press event.
    pub fn record_press(&mut self, pos: KeyPos, now_ms: u64, cols: usize) {
        if !self.enabled {
            return;
        }
        let flat = pos.flat(cols);
        if flat >= KEYS {
            return;
        }

        self.press_count[flat] = self.press_count[flat].saturating_add(1);
        self.total_presses = self.total_presses.saturating_add(1);
        self.delta_presses = self.delta_presses.saturating_add(1);
        self.press_timestamps[flat] = now_ms;

        if self.wpm_ring.is_full() {
            self.wpm_ring.pop_front();
        }
        let _ = self.wpm_ring.push_back(now_ms);

        if self.last_release_ms > 0 && now_ms.saturating_sub(self.last_release_ms) == 0 {
            self.rollover_count = self.rollover_count.saturating_add(1);
        }

        if let Some(prev_flat) = self.last_press_pos {
            let pp_ms = now_ms.saturating_sub(self.last_press_ms);
            if pp_ms < u64::from(u16::MAX) {
                self.update_bigram(prev_flat, flat as u8, pp_ms as u16);
            }
        }

        self.prev_press_pos = self.last_press_pos;
        self.last_press_pos = Some(flat as u8);
        self.last_press_ms = now_ms;
    }

    /// Returns the hold time in ms for the given key, if a press was recorded.
    #[must_use]
    pub fn last_hold_time(&self, pos: KeyPos, now_ms: u64, cols: usize) -> Option<u16> {
        if !self.enabled {
            return None;
        }
        let flat = pos.flat(cols);
        if flat >= KEYS {
            return None;
        }
        let press_time = self.press_timestamps[flat];
        if press_time == 0 || now_ms < press_time {
            return None;
        }
        let hold_ms = now_ms.saturating_sub(press_time);
        if hold_ms < u64::from(u16::MAX) {
            Some(hold_ms as u16)
        } else {
            None
        }
    }

    /// Record a key release event.
    pub fn record_release(&mut self, pos: KeyPos, now_ms: u64, cols: usize) {
        if !self.enabled {
            return;
        }
        let flat = pos.flat(cols);
        if flat >= KEYS {
            return;
        }

        let press_time = self.press_timestamps[flat];
        if press_time > 0 && now_ms >= press_time {
            let hold_ms = now_ms.saturating_sub(press_time);
            if hold_ms < u64::from(u16::MAX) {
                let sample = hold_ms as u16;
                let prev = self.hold_time_ema[flat];
                self.hold_time_ema[flat] = ema_update(prev, sample);
                let diff = sample.abs_diff(prev);
                self.hold_time_var[flat] = ema_update(self.hold_time_var[flat], diff);
            }
        }

        if now_ms > self.last_press_ms {
            self.last_release_ms = now_ms;
        }
    }

    /// Record a backspace (error indicator for the previously pressed key).
    pub fn record_error(&mut self) {
        if !self.enabled {
            return;
        }
        if let Some(prev) = self.prev_press_pos {
            let idx = usize::from(prev);
            if idx < KEYS {
                self.error_count[idx] = self.error_count[idx].saturating_add(1);
            }
        }
    }

    /// Compute current words per minute from the sliding window.
    #[must_use]
    pub fn wpm(&self) -> u16 {
        let count = self.wpm_ring.len();
        if count < 2 {
            return 0;
        }
        let Some(&newest) = self.wpm_ring.back() else {
            return 0;
        };
        let Some(&oldest) = self.wpm_ring.front() else {
            return 0;
        };
        let elapsed_ms = newest.saturating_sub(oldest);
        if elapsed_ms == 0 {
            return 0;
        }
        let chars = (count as u64).saturating_sub(1);
        (chars * 60_000 / 5 / elapsed_ms) as u16
    }

    /// Produce a compact snapshot.
    #[must_use]
    pub fn snapshot(&self) -> CoreSnapshot {
        let rollover_ratio = if self.total_presses > 0 {
            (u64::from(self.rollover_count) * 255 / u64::from(self.total_presses)) as u8
        } else {
            0
        };

        let mut hold_sum = 0u32;
        let mut hold_count = 0u32;
        for &ema in &self.hold_time_ema {
            if ema > 0 {
                hold_sum += u32::from(ema);
                hold_count += 1;
            }
        }
        let avg_hold_ms = hold_sum.checked_div(hold_count).unwrap_or(0) as u16;

        let mut pp_sum = 0u32;
        let mut pp_count = 0u32;
        for bigram in &self.bigrams[..usize::from(self.bigram_count)] {
            if bigram.count > 0 {
                pp_sum += u32::from(bigram.pp_ema);
                pp_count += 1;
            }
        }
        let avg_pp_ms = pp_sum.checked_div(pp_count).unwrap_or(0) as u16;

        let total_errors: u32 = self.error_count.iter().map(|&e| u32::from(e)).sum();
        let error_rate = (total_errors * 100)
            .checked_div(self.total_presses)
            .unwrap_or(0)
            .min(255) as u8;

        CoreSnapshot {
            wpm: self.wpm(),
            rollover_ratio,
            total_presses: self.total_presses,
            delta_presses: self.delta_presses,
            avg_hold_ms,
            avg_pp_ms,
            error_rate,
        }
    }

    /// Reset delta counters after a snapshot has been sent.
    pub fn reset_deltas(&mut self) {
        self.delta_presses = 0;
    }

    /// Returns the per-key press counts.
    #[must_use]
    pub fn press_counts(&self) -> &[u32; KEYS] {
        &self.press_count
    }

    /// Returns the per-key hold time EMAs.
    #[must_use]
    pub fn hold_time_emas(&self) -> &[u16; KEYS] {
        &self.hold_time_ema
    }

    /// Returns the per-key hold time variance estimates.
    #[must_use]
    pub fn hold_time_vars(&self) -> &[u16; KEYS] {
        &self.hold_time_var
    }

    /// Returns the per-key error counts.
    #[must_use]
    pub fn error_counts(&self) -> &[u16; KEYS] {
        &self.error_count
    }

    /// Returns the bigram statistics slice.
    #[must_use]
    pub fn bigrams(&self) -> &[BigramStat] {
        &self.bigrams[..usize::from(self.bigram_count)]
    }

    fn update_bigram(&mut self, from: u8, to: u8, pp_ms: u16) {
        for bigram in &mut self.bigrams[..usize::from(self.bigram_count)] {
            if bigram.from == from && bigram.to == to {
                bigram.pp_ema = ema_update(bigram.pp_ema, pp_ms);
                bigram.count = bigram.count.saturating_add(1);
                return;
            }
        }
        if (self.bigram_count as usize) < self.bigrams.len() {
            let idx = usize::from(self.bigram_count);
            self.bigrams[idx] = BigramStat {
                from,
                to,
                pp_ema: pp_ms,
                count: 1,
            };
            self.bigram_count += 1;
        }
    }
}

impl<const KEYS: usize> Default for CoreTelemetry<KEYS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const KEYS: usize> TelemetryExt for CoreTelemetry<KEYS> {
    type Snapshot = CoreSnapshot;

    fn snapshot_state(&self) -> CoreSnapshot {
        CoreTelemetry::snapshot(self)
    }

    fn clear_deltas(&mut self) {
        CoreTelemetry::reset_deltas(self);
    }

    fn collect(&mut self) -> CoreSnapshot {
        let snap = self.snapshot_state();
        self.clear_deltas();
        snap
    }
}

fn ema_update(prev: u16, sample: u16) -> u16 {
    ((u32::from(prev) * 7 + u32::from(sample)) >> 3) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::event::KeyPos;

    fn pos(row: u8, col: u8) -> KeyPos {
        KeyPos::new(row, col)
    }

    const COLS: usize = 3;

    #[test]
    fn press_increments_count() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_press(pos(0, 0), 0, COLS);
        t.record_press(pos(0, 0), 10, COLS);
        assert_eq!(t.press_count[0], 2);
    }

    #[test]
    fn release_updates_hold_time_ema() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_press(pos(0, 0), 100, COLS);
        t.record_release(pos(0, 0), 200, COLS);
        assert!(t.hold_time_ema[0] > 0);
    }

    #[test]
    fn hold_time_ema_converges() {
        let mut t = CoreTelemetry::<6>::new();
        for i in 0..20u64 {
            t.record_press(pos(0, 0), i * 200, COLS);
            t.record_release(pos(0, 0), i * 200 + 100, COLS);
        }
        let ema = t.hold_time_ema[0];
        assert!(
            ema > 80 && ema < 110,
            "EMA should converge near 100, got {ema}"
        );
    }

    #[test]
    fn wpm_calculation() {
        let mut t = CoreTelemetry::<6>::new();
        for i in 0..50u64 {
            t.record_press(pos(0, (i % 3) as u8), i * 100, COLS);
        }
        let wpm = t.wpm();
        assert!(wpm > 0, "WPM should be non-zero");
    }

    #[test]
    fn wpm_zero_with_no_data() {
        let t = CoreTelemetry::<6>::new();
        assert_eq!(t.wpm(), 0);
    }

    #[test]
    fn wpm_zero_with_one_press() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_press(pos(0, 0), 0, COLS);
        assert_eq!(t.wpm(), 0);
    }

    #[test]
    fn error_count_tracks_backspace() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_press(pos(0, 1), 0, COLS);
        t.record_press(pos(0, 2), 50, COLS);
        t.record_error();
        assert_eq!(t.error_count[1], 1);
    }

    #[test]
    fn error_with_no_prior_press_is_safe() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_error();
        assert!(t.error_count.iter().all(|&c| c == 0));
    }

    #[test]
    fn bigram_tracking() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_press(pos(0, 0), 0, COLS);
        t.record_press(pos(0, 1), 50, COLS);
        assert_eq!(t.bigram_count, 1);
        assert_eq!(t.bigrams[0].from, 0);
        assert_eq!(t.bigrams[0].to, 1);
        assert_eq!(t.bigrams[0].pp_ema, 50);
    }

    #[test]
    fn bigram_ema_updates_on_repeat() {
        let mut t = CoreTelemetry::<6>::new();
        for i in 0..10u64 {
            t.record_press(pos(0, 0), i * 200, COLS);
            t.record_press(pos(0, 1), i * 200 + 100, COLS);
        }
        assert_eq!(t.bigram_count, 2);
        let stat = &t.bigrams[0];
        assert!(stat.pp_ema > 90 && stat.pp_ema < 110);
    }

    #[test]
    fn snapshot_produces_valid_data() {
        let mut t = CoreTelemetry::<6>::new();
        for i in 0..20u64 {
            t.record_press(pos(0, (i % 3) as u8), i * 100, COLS);
            t.record_release(pos(0, (i % 3) as u8), i * 100 + 50, COLS);
        }
        t.record_error();
        let snap = t.snapshot();
        assert!(snap.wpm > 0);
        assert!(snap.total_presses == 20);
        assert!(snap.avg_hold_ms > 0);
        assert!(snap.error_rate > 0);
    }

    #[test]
    fn reset_deltas_clears_delta_presses() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_press(pos(0, 0), 0, COLS);
        assert_eq!(t.delta_presses, 1);
        t.reset_deltas();
        assert_eq!(t.delta_presses, 0);
        assert_eq!(t.total_presses, 1);
    }

    #[test]
    fn disabled_telemetry_collects_nothing() {
        let mut t = CoreTelemetry::<6>::new();
        t.set_enabled(false);
        t.record_press(pos(0, 0), 0, COLS);
        t.record_release(pos(0, 0), 50, COLS);
        t.record_error();
        assert_eq!(t.total_presses, 0);
        assert_eq!(t.hold_time_ema[0], 0);
    }

    #[test]
    fn out_of_bounds_key_is_safe() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_press(pos(5, 5), 0, COLS);
        assert_eq!(t.total_presses, 0);
    }

    #[test]
    fn telemetry_ext_unit_impl_is_noop() {
        let mut ext = ();
        assert_eq!(ext.snapshot_state(), ());
        ext.clear_deltas();
    }

    #[test]
    fn hold_time_variance_tracks_jitter() {
        let mut t = CoreTelemetry::<6>::new();
        t.record_press(pos(0, 0), 0, COLS);
        t.record_release(pos(0, 0), 100, COLS);
        t.record_press(pos(0, 0), 200, COLS);
        t.record_release(pos(0, 0), 350, COLS);
        assert!(
            t.hold_time_var[0] > 0,
            "variance should be non-zero with jittery hold times"
        );
    }

    #[test]
    fn bigram_slots_saturate_gracefully() {
        let mut t = CoreTelemetry::<6>::new();
        for i in 0..70u64 {
            let from = (i % 6) as u8;
            let to = ((i + 1) % 6) as u8;
            t.record_press(KeyPos::new(from / 3, from % 3), i * 100, COLS);
            t.record_press(KeyPos::new(to / 3, to % 3), i * 100 + 50, COLS);
        }
        assert!(t.bigram_count <= 64);
    }
}
