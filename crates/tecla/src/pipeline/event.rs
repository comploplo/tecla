//! Core event types produced by the matrix scanner.
//!
//! Every key interaction in the pipeline starts as a [`KeyEvent`].

/// A key's location in the switch matrix, identified by row and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyPos {
    /// Zero-based row index.
    pub row: u8,
    /// Zero-based column index.
    pub col: u8,
}

impl KeyPos {
    /// Creates a new [`KeyPos`].
    #[must_use]
    pub const fn new(row: u8, col: u8) -> Self {
        Self { row, col }
    }

    /// Converts this position to a flat index into a row-major array.
    ///
    /// `cols` is the total number of columns in the matrix.
    #[must_use]
    pub fn flat(self, cols: usize) -> usize {
        usize::from(self.row) * cols + usize::from(self.col)
    }
}

/// A key press or release event produced by the matrix scanner after debouncing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    /// The key's position in the matrix.
    pub pos: KeyPos,
    /// `true` if the key was pressed; `false` if it was released.
    pub pressed: bool,
    /// Timestamp in milliseconds since firmware start.
    pub timestamp_ms: u64,
}

impl KeyEvent {
    /// Creates a press event at the given position and time.
    #[must_use]
    pub const fn press(pos: KeyPos, timestamp_ms: u64) -> Self {
        Self {
            pos,
            pressed: true,
            timestamp_ms,
        }
    }

    /// Creates a release event at the given position and time.
    #[must_use]
    pub const fn release(pos: KeyPos, timestamp_ms: u64) -> Self {
        Self {
            pos,
            pressed: false,
            timestamp_ms,
        }
    }

    /// Returns `true` if this is a press event.
    #[must_use]
    pub const fn is_press(&self) -> bool {
        self.pressed
    }

    /// Returns `true` if this is a release event.
    #[must_use]
    pub const fn is_release(&self) -> bool {
        !self.pressed
    }
}

/// Rotation direction of a rotary encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncoderDirection {
    /// Clockwise rotation.
    Clockwise,
    /// Counter-clockwise rotation.
    CounterClockwise,
}

/// A single step event from a rotary encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderEvent {
    /// Zero-based encoder index.
    pub encoder_id: u8,
    /// Direction of the step.
    pub direction: EncoderDirection,
    /// Timestamp in milliseconds since firmware start.
    pub timestamp_ms: u64,
}

/// A pointing-device event (trackball, trackpad, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointingEvent {
    /// Horizontal movement (positive = right).
    pub dx: i16,
    /// Vertical movement (positive = down).
    pub dy: i16,
    /// Button bitmask (bit 0 = left, bit 1 = right, bit 2 = middle).
    pub buttons: u8,
    /// Vertical scroll (positive = up).
    pub scroll: i8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_pos_stores_row_and_col() {
        let pos = KeyPos::new(2, 5);
        assert_eq!(pos.row, 2);
        assert_eq!(pos.col, 5);
    }

    #[test]
    fn key_pos_ordered_by_row_then_col() {
        assert!(KeyPos::new(0, 0) < KeyPos::new(0, 1));
        assert!(KeyPos::new(0, 9) < KeyPos::new(1, 0));
    }

    #[test]
    fn flat_index_origin_is_zero() {
        assert_eq!(KeyPos::new(0, 0).flat(14), 0);
    }

    #[test]
    fn flat_index_second_row_starts_at_cols() {
        assert_eq!(KeyPos::new(1, 0).flat(14), 14);
    }

    #[test]
    fn flat_index_arbitrary_position() {
        // row 2, col 3 in a 14-wide matrix → 2*14 + 3 = 31
        assert_eq!(KeyPos::new(2, 3).flat(14), 31);
    }

    #[test]
    fn press_event_is_marked_as_press() {
        let event = KeyEvent::press(KeyPos::new(0, 0), 100);
        assert!(event.is_press());
        assert!(!event.is_release());
    }

    #[test]
    fn press_event_carries_position_and_timestamp() {
        let pos = KeyPos::new(3, 7);
        let event = KeyEvent::press(pos, 42);
        assert_eq!(event.pos, pos);
        assert_eq!(event.timestamp_ms, 42);
    }

    #[test]
    fn release_event_is_marked_as_release() {
        let event = KeyEvent::release(KeyPos::new(0, 0), 200);
        assert!(event.is_release());
        assert!(!event.is_press());
    }

    #[test]
    fn release_event_carries_position_and_timestamp() {
        let pos = KeyPos::new(1, 2);
        let event = KeyEvent::release(pos, 99);
        assert_eq!(event.pos, pos);
        assert_eq!(event.timestamp_ms, 99);
    }

    #[test]
    fn events_differ_by_position() {
        assert_ne!(
            KeyEvent::press(KeyPos::new(0, 0), 0),
            KeyEvent::press(KeyPos::new(0, 1), 0),
        );
    }

    #[test]
    fn encoder_event_stores_fields() {
        let e = EncoderEvent {
            encoder_id: 2,
            direction: EncoderDirection::Clockwise,
            timestamp_ms: 100,
        };
        assert_eq!(e.encoder_id, 2);
        assert_eq!(e.direction, EncoderDirection::Clockwise);
        assert_eq!(e.timestamp_ms, 100);
    }

    #[test]
    fn pointing_event_stores_fields() {
        let e = PointingEvent {
            dx: 10,
            dy: -5,
            buttons: 0x01,
            scroll: 3,
        };
        assert_eq!(e.dx, 10);
        assert_eq!(e.dy, -5);
        assert_eq!(e.buttons, 0x01);
        assert_eq!(e.scroll, 3);
    }
}
