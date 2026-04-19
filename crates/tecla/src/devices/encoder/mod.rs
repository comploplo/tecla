//! Rotary encoder support — mapping rotation steps to keymap actions.
//!
//! Each physical encoder is described by an [`EncoderMapping`] that pairs
//! clockwise and counter-clockwise rotation with an [`Action`]. Encoders
//! are addressed via virtual [`KeyPos`] values that sit just past the
//! physical matrix (see [`encoder_key_pos`]).
//!
//! On Embassy targets, implement `AsyncEncoderScanner` to feed encoder
//! events into the pipeline.
//!
//! See also: [`pointing`](crate::devices::mouse::pointing) for trackball/trackpad input.

use crate::action::Action;
use crate::pipeline::event::{EncoderDirection, KeyPos};

/// Maps a rotary encoder to clockwise and counter-clockwise actions.
pub struct EncoderMapping<Ext = ()>
where
    Ext: Copy + PartialEq,
{
    /// Zero-based encoder index.
    pub encoder_id: u8,
    /// Action triggered on clockwise rotation.
    pub cw: Action<Ext>,
    /// Action triggered on counter-clockwise rotation.
    pub ccw: Action<Ext>,
}

/// Returns the virtual [`KeyPos`] for a rotary encoder direction.
///
/// Encoder positions sit in a virtual row just past the physical matrix.
/// All encoders share `row = rows` (the first row index beyond the matrix),
/// and each encoder occupies two columns: CW at `encoder_id * 2`, CCW at
/// `encoder_id * 2 + 1`.
///
/// This means `pos.flat(cols)` produces `rows * cols + encoder_id * 2 + dir`,
/// which the user must include in their per-layer keymap by sizing `KEYS`
/// to `rows * cols + num_encoders * 2`.
#[must_use]
pub const fn encoder_key_pos(encoder_id: u8, direction: EncoderDirection, rows: u8) -> KeyPos {
    let col = match direction {
        EncoderDirection::Clockwise => 0,
        EncoderDirection::CounterClockwise => 1,
    };
    KeyPos::new(rows, encoder_id.wrapping_mul(2).wrapping_add(col))
}

/// Async scanner trait for rotary encoders.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait AsyncEncoderScanner {
    /// Waits for and returns the next encoder event.
    async fn scan(&mut self) -> crate::pipeline::event::EncoderEvent;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::KeyCode;
    use crate::pipeline::event::EncoderDirection;

    #[test]
    fn encoder_mapping_stores_actions() {
        let m = EncoderMapping::<()> {
            encoder_id: 0,
            cw: Action::Key(KeyCode::A),
            ccw: Action::Key(KeyCode::B),
        };
        assert_eq!(m.encoder_id, 0);
        assert_eq!(m.cw, Action::Key(KeyCode::A));
        assert_eq!(m.ccw, Action::Key(KeyCode::B));
    }

    #[test]
    fn encoder_event_direction_variants() {
        let cw = EncoderDirection::Clockwise;
        let ccw = EncoderDirection::CounterClockwise;
        assert_ne!(cw, ccw);
    }

    #[test]
    fn encoder_key_pos_cw_uses_even_col() {
        let pos = encoder_key_pos(0, EncoderDirection::Clockwise, 4);
        assert_eq!(pos.row, 4);
        assert_eq!(pos.col, 0);
    }

    #[test]
    fn encoder_key_pos_ccw_uses_odd_col() {
        let pos = encoder_key_pos(0, EncoderDirection::CounterClockwise, 4);
        assert_eq!(pos.row, 4);
        assert_eq!(pos.col, 1);
    }

    #[test]
    fn encoder_key_pos_second_encoder_offset() {
        let cw = encoder_key_pos(1, EncoderDirection::Clockwise, 4);
        let ccw = encoder_key_pos(1, EncoderDirection::CounterClockwise, 4);
        assert_eq!(cw.col, 2);
        assert_eq!(ccw.col, 3);
    }

    #[test]
    fn encoder_key_pos_flat_index_follows_matrix() {
        let pos = encoder_key_pos(0, EncoderDirection::Clockwise, 2);
        assert_eq!(pos.flat(6), 2 * 6);
        let pos_ccw = encoder_key_pos(0, EncoderDirection::CounterClockwise, 2);
        assert_eq!(pos_ccw.flat(6), 2 * 6 + 1);
    }
}
