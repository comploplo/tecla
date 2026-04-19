//! Gamepad state and dispatch.
//!
//! [`GamepadState`] owns the button bitmask and axis values for a HID
//! gamepad. It is driven by [`GamepadAction`] events from the keyboard
//! pipeline and produces [`GamepadReport`]s for the HID transport layer.
//!
//! See also: [`action::GamepadAction`](crate::action::GamepadAction) for
//! the keymap-level action type and
//! [`pipeline::report::GamepadReport`](crate::pipeline::report::GamepadReport)
//! for the HID report struct.

use crate::action::GamepadAction;
use crate::pipeline::report::GamepadReport;

/// Gamepad state: button bitmask and axis values.
pub struct GamepadState {
    buttons: u16,
    axes: [i8; 6],
}

impl GamepadState {
    /// Creates a new gamepad state with all buttons released and axes centered.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buttons: 0,
            axes: [0; 6],
        }
    }

    /// Applies a gamepad press action and returns the updated report.
    pub fn press(&mut self, action: GamepadAction) -> GamepadReport {
        match action {
            GamepadAction::ButtonPress(btn) => {
                self.buttons |= 1 << u16::from(btn);
            }
            GamepadAction::AxisSet { axis, value } => {
                if let Some(slot) = self.axes.get_mut(usize::from(axis)) {
                    *slot = value;
                }
            }
        }
        self.report()
    }

    /// Applies a gamepad release action.
    ///
    /// Returns `Some(report)` for button releases (bit cleared). Axis
    /// actions have no release semantic and return `None`.
    pub fn release(&mut self, action: GamepadAction) -> Option<GamepadReport> {
        match action {
            GamepadAction::ButtonPress(btn) => {
                self.buttons &= !(1 << u16::from(btn));
                Some(self.report())
            }
            GamepadAction::AxisSet { .. } => None,
        }
    }

    fn report(&self) -> GamepadReport {
        GamepadReport {
            buttons: self.buttons,
            axes: self.axes,
        }
    }
}

impl Default for GamepadState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_zero_buttons_and_axes() {
        let s = GamepadState::new();
        let r = s.report();
        assert_eq!(r.buttons, 0);
        assert_eq!(r.axes, [0; 6]);
    }

    #[test]
    fn button_press_sets_bit() {
        let mut s = GamepadState::new();
        let r = s.press(GamepadAction::ButtonPress(3));
        assert_eq!(r.buttons, 1 << 3);
    }

    #[test]
    fn button_release_clears_bit() {
        let mut s = GamepadState::new();
        s.press(GamepadAction::ButtonPress(3));
        let r = s.release(GamepadAction::ButtonPress(3)).unwrap();
        assert_eq!(r.buttons, 0);
    }

    #[test]
    fn axis_set_writes_value() {
        let mut s = GamepadState::new();
        let r = s.press(GamepadAction::AxisSet { axis: 1, value: -42 });
        assert_eq!(r.axes[1], -42);
    }

    #[test]
    fn axis_release_returns_none() {
        let mut s = GamepadState::new();
        s.press(GamepadAction::AxisSet { axis: 0, value: 64 });
        assert!(s.release(GamepadAction::AxisSet { axis: 0, value: 64 }).is_none());
    }

    #[test]
    fn axis_out_of_range_ignored() {
        let mut s = GamepadState::new();
        let r = s.press(GamepadAction::AxisSet { axis: 9, value: 100 });
        assert_eq!(r.axes, [0; 6]);
    }

    #[test]
    fn multiple_buttons_compose_in_bitmask() {
        let mut s = GamepadState::new();
        s.press(GamepadAction::ButtonPress(0));
        s.press(GamepadAction::ButtonPress(2));
        let r = s.press(GamepadAction::ButtonPress(5));
        assert_eq!(r.buttons, (1 << 0) | (1 << 2) | (1 << 5));
    }
}
