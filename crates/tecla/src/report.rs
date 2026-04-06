//! HID report types.
//!
//! [`HidReport`] is the value emitted by the action pipeline and consumed
//! by a [`Reporter`](crate::hid::Reporter) implementation.

use crate::action::ConsumerCode;
use crate::modifier::ModifierState;

/// Standard 6KRO boot-protocol keyboard HID report descriptor (63 bytes).
///
/// USB HID spec appendix B.1. Compatible with every OS and BIOS.
pub const KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01,
    0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x05, 0x75, 0x01,
    0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x01, 0x95, 0x06,
    0x75, 0x08, 0x15, 0x00, 0x25, 0xDD, 0x05, 0x07, 0x19, 0x00, 0x29, 0xDD, 0x81, 0x00, 0xC0,
];

/// Maximum number of simultaneous non-modifier keycodes in a 6KRO report.
pub const KRO_KEYS: usize = 6;

/// Byte width of an NKRO bitmap report (128 HID usage IDs / 8 bits).
pub const NKRO_BYTES: usize = 16;

/// A 6KRO (six-key rollover) keyboard HID report.
///
/// Matches the standard boot-protocol keyboard report layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyboardReport {
    /// Active modifier bitmask.
    pub modifiers: u8,
    /// Reserved byte (always 0).
    pub reserved: u8,
    /// Up to 6 simultaneously pressed keycodes (0x00 = empty slot).
    pub keycodes: [u8; KRO_KEYS],
}

impl KeyboardReport {
    /// Creates an empty (all-keys-released) report.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            modifiers: 0,
            reserved: 0,
            keycodes: [0; KRO_KEYS],
        }
    }

    /// Creates a report from a [`ModifierState`] and a slice of HID usage IDs.
    ///
    /// Only the first [`KRO_KEYS`] keycodes are included; extras are silently
    /// dropped.
    #[must_use]
    pub fn from_state(mods: &ModifierState, keys: &[u8]) -> Self {
        let mut report = Self::new();
        report.modifiers = mods.report_byte().bits();
        for (slot, &kc) in report.keycodes.iter_mut().zip(keys.iter()) {
            *slot = kc;
        }
        report
    }
}

/// An N-key rollover keyboard HID report (bitmap format).
///
/// Each bit in `bitmap` corresponds to one HID usage ID (bit `n` = usage `n`).
/// Supports up to 128 simultaneous non-modifier keycodes — all modern keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NkroKeyboardReport {
    /// Active modifier bitmask (same layout as [`KeyboardReport`]).
    pub modifiers: u8,
    /// Bitmap: bit `n` is set when HID usage ID `n` is pressed.
    pub bitmap: [u8; NKRO_BYTES],
}

impl NkroKeyboardReport {
    /// Creates an empty (all-keys-released) NKRO report.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            modifiers: 0,
            bitmap: [0; NKRO_BYTES],
        }
    }

    /// Sets the bit for `keycode` in the bitmap.
    pub fn press(&mut self, keycode: u8) {
        let byte = usize::from(keycode >> 3);
        let bit = keycode & 7;
        if let Some(slot) = self.bitmap.get_mut(byte) {
            *slot |= 1 << bit;
        }
    }

    /// Clears the bit for `keycode` in the bitmap.
    pub fn release(&mut self, keycode: u8) {
        let byte = usize::from(keycode >> 3);
        let bit = keycode & 7;
        if let Some(slot) = self.bitmap.get_mut(byte) {
            *slot &= !(1 << bit);
        }
    }

    /// Builds an NKRO report from a [`ModifierState`] and a slice of HID usage IDs.
    #[must_use]
    pub fn from_state(mods: &ModifierState, keys: &[u8]) -> Self {
        let mut report = Self::new();
        report.modifiers = mods.report_byte().bits();
        for &kc in keys {
            report.press(kc);
        }
        report
    }
}

/// A consumer-control HID report (media keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConsumerReport {
    /// The active consumer keycode, or `None` for an all-released report.
    pub keycode: Option<ConsumerCode>,
}

impl ConsumerReport {
    /// Creates a report with no active consumer key.
    #[must_use]
    pub const fn new() -> Self {
        Self { keycode: None }
    }
}

/// HID mouse report for cursor movement, clicks, and scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseReport {
    /// Button bitmask (bit 0 = left, bit 1 = right, bit 2 = middle).
    pub buttons: u8,
    /// Horizontal movement (positive = right).
    pub dx: i8,
    /// Vertical movement (positive = down).
    pub dy: i8,
    /// Vertical scroll (positive = up).
    pub scroll: i8,
}

/// A MIDI event produced by the action pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiEvent {
    /// Note-on event.
    NoteOn {
        /// MIDI channel (0–15).
        channel: u8,
        /// MIDI note number (0–127).
        note: u8,
        /// Velocity (0–127).
        velocity: u8,
    },
    /// Note-off event.
    NoteOff {
        /// MIDI channel (0–15).
        channel: u8,
        /// MIDI note number (0–127).
        note: u8,
    },
    /// Control-change event.
    Cc {
        /// MIDI channel (0–15).
        channel: u8,
        /// Controller number (0–127).
        controller: u8,
        /// Controller value (0–127).
        value: u8,
    },
}

/// A HID gamepad report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamepadReport {
    /// Button bitmask (up to 16 buttons).
    pub buttons: u16,
    /// Axis values: x, y, z, rx, ry, rz (−128 to 127 each).
    pub axes: [i8; 6],
}

/// A HID report produced by the action pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidReport {
    /// Standard keyboard report (6KRO).
    Keyboard(KeyboardReport),
    /// Consumer-control (media key) report.
    Consumer(ConsumerReport),
    /// N-key rollover keyboard report (bitmap format).
    NkroKeyboard(NkroKeyboardReport),
    /// Mouse report (movement, clicks, scroll).
    Mouse(MouseReport),
    /// MIDI event (note on/off, CC).
    Midi(MidiEvent),
    /// Gamepad report (buttons + axes).
    Gamepad(GamepadReport),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ModifierKey;

    #[test]
    fn keyboard_report_new_is_all_zeros() {
        let r = KeyboardReport::new();
        assert_eq!(r.modifiers, 0);
        assert_eq!(r.reserved, 0);
        assert_eq!(r.keycodes, [0; KRO_KEYS]);
    }

    #[test]
    fn keyboard_report_default_equals_new() {
        assert_eq!(KeyboardReport::default(), KeyboardReport::new());
    }

    #[test]
    fn from_state_sets_modifier_byte() {
        let mut mods = ModifierState::new();
        mods.press(ModifierKey::LSHIFT);
        let r = KeyboardReport::from_state(&mods, &[]);
        assert_eq!(r.modifiers, ModifierKey::LSHIFT.bits());
    }

    #[test]
    fn from_state_fills_keycodes_up_to_six() {
        let mods = ModifierState::new();
        let r = KeyboardReport::from_state(&mods, &[0x04, 0x05, 0x06]);
        assert_eq!(r.keycodes[0], 0x04);
        assert_eq!(r.keycodes[1], 0x05);
        assert_eq!(r.keycodes[2], 0x06);
        assert_eq!(r.keycodes[3], 0x00);
    }

    #[test]
    fn from_state_truncates_excess_keycodes() {
        let mods = ModifierState::new();
        let keys = [0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];
        let r = KeyboardReport::from_state(&mods, &keys);
        assert_eq!(r.keycodes[5], 0x09);
        // Seventh key is dropped
    }

    #[test]
    fn consumer_report_new_has_no_keycode() {
        assert_eq!(ConsumerReport::new().keycode, None);
    }

    #[test]
    fn consumer_report_default_equals_new() {
        assert_eq!(ConsumerReport::default(), ConsumerReport::new());
    }

    #[test]
    fn hid_report_keyboard_variant() {
        let r = HidReport::Keyboard(KeyboardReport::new());
        assert!(matches!(r, HidReport::Keyboard(_)));
    }

    #[test]
    fn hid_report_consumer_variant() {
        let r = HidReport::Consumer(ConsumerReport::new());
        assert!(matches!(r, HidReport::Consumer(_)));
    }

    #[test]
    fn nkro_report_new_is_all_zeros() {
        let r = NkroKeyboardReport::new();
        assert_eq!(r.modifiers, 0);
        assert_eq!(r.bitmap, [0; NKRO_BYTES]);
    }

    #[test]
    fn nkro_press_sets_correct_bit() {
        let mut r = NkroKeyboardReport::new();
        r.press(0x04); // HID usage 4 = A key, byte 0, bit 4
        assert_eq!(r.bitmap[0], 1 << 4);
    }

    #[test]
    fn nkro_release_clears_bit() {
        let mut r = NkroKeyboardReport::new();
        r.press(0x04);
        r.release(0x04);
        assert_eq!(r.bitmap[0], 0);
    }

    #[test]
    fn nkro_from_state_sets_modifier_and_bitmap() {
        let mut mods = ModifierState::new();
        mods.press(ModifierKey::LSHIFT);
        let r = NkroKeyboardReport::from_state(&mods, &[0x04]);
        assert_eq!(r.modifiers, ModifierKey::LSHIFT.bits());
        assert!(r.bitmap[0] & (1 << 4) != 0);
    }

    #[test]
    fn nkro_out_of_range_keycode_does_not_panic() {
        let mut r = NkroKeyboardReport::new();
        r.press(0xFF); // byte 31, out of range for 16-byte bitmap
        assert_eq!(r.bitmap, [0; NKRO_BYTES]); // unchanged
    }

    #[test]
    fn hid_report_nkro_variant() {
        let r = HidReport::NkroKeyboard(NkroKeyboardReport::new());
        assert!(matches!(r, HidReport::NkroKeyboard(_)));
    }

    #[test]
    fn midi_event_note_on_variant() {
        let e = MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 127,
        };
        assert!(matches!(e, MidiEvent::NoteOn { .. }));
    }

    #[test]
    fn midi_event_note_off_variant() {
        let e = MidiEvent::NoteOff {
            channel: 0,
            note: 60,
        };
        assert!(matches!(e, MidiEvent::NoteOff { .. }));
    }

    #[test]
    fn midi_event_cc_variant() {
        let e = MidiEvent::Cc {
            channel: 0,
            controller: 1,
            value: 64,
        };
        assert!(matches!(e, MidiEvent::Cc { .. }));
    }

    #[test]
    fn gamepad_report_default_is_zeroed() {
        let r = GamepadReport::default();
        assert_eq!(r.buttons, 0);
        assert_eq!(r.axes, [0; 6]);
    }

    #[test]
    fn hid_report_midi_variant() {
        let r = HidReport::Midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 127,
        });
        assert!(matches!(r, HidReport::Midi(_)));
    }

    #[test]
    fn hid_report_gamepad_variant() {
        let r = HidReport::Gamepad(GamepadReport::default());
        assert!(matches!(r, HidReport::Gamepad(_)));
    }
}
