//! Standard HID keyboard device.
//!
//! Keycodes, reports, and input recognizers (tap-hold, combo, tap-dance,
//! leader) for conventional keyboard input.

pub mod combo;
pub mod leader;
pub mod tap_dance;
pub mod tapping;
