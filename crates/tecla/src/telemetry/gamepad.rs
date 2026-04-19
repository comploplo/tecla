//! Gamepad telemetry.
//!
//! Tracks per-axis utilization, button press patterns, and simultaneous
//! input combinations — useful for controller ergonomics and dead zone
//! calibration.

/// Gamepad telemetry collector.
#[derive(Default)]
pub struct GamepadTelemetry {
    button_counts: [u32; 16],
    axis_min: [i8; 6],
    axis_max: [i8; 6],
    max_simultaneous_buttons: u8,
    current_buttons: u16,
    total_presses: u32,
    delta_presses: u16,
}

/// Compact gamepad snapshot for transport.
#[derive(Clone, Copy, Default)]
pub struct GamepadSnapshot {
    /// Peak simultaneous buttons held.
    pub max_simultaneous: u8,
    /// Per-axis observed min values.
    pub axis_min: [i8; 6],
    /// Per-axis observed max values.
    pub axis_max: [i8; 6],
    /// Total button presses this session.
    pub total_presses: u32,
    /// Presses since last snapshot.
    pub delta_presses: u16,
}

impl GamepadTelemetry {
    /// Record a button press.
    pub fn record_button_press(&mut self, button: u8) {
        let idx = usize::from(button);
        if idx < 16 {
            self.button_counts[idx] = self.button_counts[idx].saturating_add(1);
        }
        self.current_buttons |= 1u16 << (button & 15);
        let count = self.current_buttons.count_ones() as u8;
        if count > self.max_simultaneous_buttons {
            self.max_simultaneous_buttons = count;
        }
        self.total_presses = self.total_presses.saturating_add(1);
        self.delta_presses = self.delta_presses.saturating_add(1);
    }

    /// Record a button release.
    pub fn record_button_release(&mut self, button: u8) {
        self.current_buttons &= !(1u16 << (button & 15));
    }

    /// Record an axis value sample.
    pub fn record_axis(&mut self, axis: u8, value: i8) {
        let idx = usize::from(axis);
        if idx < 6 {
            if value < self.axis_min[idx] {
                self.axis_min[idx] = value;
            }
            if value > self.axis_max[idx] {
                self.axis_max[idx] = value;
            }
        }
    }

    /// Produce a snapshot.
    #[must_use]
    pub fn snapshot(&self) -> GamepadSnapshot {
        GamepadSnapshot {
            max_simultaneous: self.max_simultaneous_buttons,
            axis_min: self.axis_min,
            axis_max: self.axis_max,
            total_presses: self.total_presses,
            delta_presses: self.delta_presses,
        }
    }

    /// Reset delta counters.
    pub fn reset_deltas(&mut self) {
        self.delta_presses = 0;
    }

    /// Returns the per-button press counts.
    #[must_use]
    pub fn button_counts(&self) -> &[u32; 16] {
        &self.button_counts
    }
}

impl super::TelemetryExt for GamepadTelemetry {
    type Snapshot = GamepadSnapshot;
    fn snapshot_state(&self) -> GamepadSnapshot {
        GamepadTelemetry::snapshot(self)
    }
    fn clear_deltas(&mut self) {
        GamepadTelemetry::reset_deltas(self);
    }
    fn collect(&mut self) -> GamepadSnapshot {
        let snap = self.snapshot_state();
        self.clear_deltas();
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_press_counting() {
        let mut t = GamepadTelemetry::default();
        t.record_button_press(0);
        t.record_button_press(0);
        t.record_button_press(3);
        assert_eq!(t.button_counts[0], 2);
        assert_eq!(t.button_counts[3], 1);
    }

    #[test]
    fn simultaneous_buttons() {
        let mut t = GamepadTelemetry::default();
        t.record_button_press(0);
        t.record_button_press(1);
        t.record_button_press(2);
        assert_eq!(t.snapshot().max_simultaneous, 3);
        t.record_button_release(0);
        t.record_button_release(1);
        assert_eq!(t.snapshot().max_simultaneous, 3);
    }

    #[test]
    fn axis_range_tracking() {
        let mut t = GamepadTelemetry::default();
        t.record_axis(0, -50);
        t.record_axis(0, 80);
        t.record_axis(0, 10);
        let snap = t.snapshot();
        assert_eq!(snap.axis_min[0], -50);
        assert_eq!(snap.axis_max[0], 80);
    }

    #[test]
    fn out_of_range_axis_is_safe() {
        let mut t = GamepadTelemetry::default();
        t.record_axis(10, 100);
        t.record_button_press(20);
        assert_eq!(t.total_presses, 1);
    }
}
