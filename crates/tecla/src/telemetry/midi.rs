//! MIDI telemetry.
//!
//! Tracks velocity distributions, note durations, polyphony depth, and
//! CC utilization — the metrics that matter for control surface ergonomics
//! and expressive performance analysis.

/// MIDI telemetry collector.
#[derive(Default)]
pub struct MidiTelemetry {
    note_on_count: u32,
    note_off_count: u32,
    velocity_sum: u32,
    velocity_min: u8,
    velocity_max: u8,
    cc_count: u32,
    max_polyphony: u8,
    current_polyphony: u8,
    delta_notes: u16,
    initialized: bool,
}

/// Compact MIDI snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct MidiSnapshot {
    /// Average note velocity (0-127).
    pub avg_velocity: u8,
    /// Minimum velocity seen.
    pub min_velocity: u8,
    /// Maximum velocity seen.
    pub max_velocity: u8,
    /// Peak simultaneous notes.
    pub max_polyphony: u8,
    /// CC messages this session.
    pub cc_count: u32,
    /// Notes since last snapshot.
    pub delta_notes: u16,
}

impl MidiTelemetry {
    /// Record a note-on event.
    pub fn record_note_on(&mut self, velocity: u8) {
        self.note_on_count = self.note_on_count.saturating_add(1);
        self.delta_notes = self.delta_notes.saturating_add(1);
        self.velocity_sum = self.velocity_sum.saturating_add(u32::from(velocity));
        if !self.initialized {
            self.velocity_min = velocity;
            self.velocity_max = velocity;
            self.initialized = true;
        } else {
            if velocity < self.velocity_min {
                self.velocity_min = velocity;
            }
            if velocity > self.velocity_max {
                self.velocity_max = velocity;
            }
        }
        self.current_polyphony = self.current_polyphony.saturating_add(1);
        if self.current_polyphony > self.max_polyphony {
            self.max_polyphony = self.current_polyphony;
        }
    }

    /// Record a note-off event.
    pub fn record_note_off(&mut self) {
        self.note_off_count = self.note_off_count.saturating_add(1);
        self.current_polyphony = self.current_polyphony.saturating_sub(1);
    }

    /// Record a CC (control change) message.
    pub fn record_cc(&mut self) {
        self.cc_count = self.cc_count.saturating_add(1);
    }

    /// Produce a snapshot.
    #[must_use]
    pub fn snapshot(&self) -> MidiSnapshot {
        let avg_velocity = self
            .velocity_sum
            .checked_div(self.note_on_count)
            .unwrap_or(0) as u8;
        MidiSnapshot {
            avg_velocity,
            min_velocity: self.velocity_min,
            max_velocity: self.velocity_max,
            max_polyphony: self.max_polyphony,
            cc_count: self.cc_count,
            delta_notes: self.delta_notes,
        }
    }

    /// Reset delta counters.
    pub fn reset_deltas(&mut self) {
        self.delta_notes = 0;
    }
}

impl super::TelemetryExt for MidiTelemetry {
    type Snapshot = MidiSnapshot;
    fn snapshot_state(&self) -> MidiSnapshot {
        MidiTelemetry::snapshot(self)
    }
    fn clear_deltas(&mut self) {
        MidiTelemetry::reset_deltas(self);
    }
    fn collect(&mut self) -> MidiSnapshot {
        let snap = self.snapshot_state();
        self.clear_deltas();
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_tracking() {
        let mut t = MidiTelemetry::default();
        t.record_note_on(60);
        t.record_note_on(100);
        t.record_note_on(80);
        let snap = t.snapshot();
        assert_eq!(snap.avg_velocity, 80);
        assert_eq!(snap.min_velocity, 60);
        assert_eq!(snap.max_velocity, 100);
    }

    #[test]
    fn polyphony_tracking() {
        let mut t = MidiTelemetry::default();
        t.record_note_on(64);
        t.record_note_on(64);
        t.record_note_on(64);
        assert_eq!(t.max_polyphony, 3);
        t.record_note_off();
        t.record_note_off();
        assert_eq!(t.current_polyphony, 1);
        assert_eq!(t.snapshot().max_polyphony, 3);
    }

    #[test]
    fn cc_counting() {
        let mut t = MidiTelemetry::default();
        t.record_cc();
        t.record_cc();
        assert_eq!(t.snapshot().cc_count, 2);
    }

    #[test]
    fn empty_snapshot() {
        let t = MidiTelemetry::default();
        let snap = t.snapshot();
        assert_eq!(snap.avg_velocity, 0);
        assert_eq!(snap.max_polyphony, 0);
    }

    #[test]
    fn delta_reset() {
        let mut t = MidiTelemetry::default();
        t.record_note_on(64);
        t.reset_deltas();
        assert_eq!(t.delta_notes, 0);
        assert_eq!(t.note_on_count, 1);
    }
}
