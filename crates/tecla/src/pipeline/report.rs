//! HID report types.
//!
//! [`HidReport`] is the value emitted by the action pipeline and consumed
//! by a [`Reporter`](crate::pipeline::hid::Reporter) implementation.

use super::modifier::ModifierState;
use crate::action::ConsumerCode;

/// Standard 6KRO boot-protocol keyboard HID report descriptor (63 bytes).
///
/// USB HID spec appendix B.1. Compatible with every OS and BIOS.
pub const KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01,
    0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x05, 0x75, 0x01,
    0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x01, 0x95, 0x06,
    0x75, 0x08, 0x15, 0x00, 0x25, 0xDD, 0x05, 0x07, 0x19, 0x00, 0x29, 0xDD, 0x81, 0x00, 0xC0,
];

/// N-key-rollover keyboard HID report descriptor (bitmap format).
///
/// Reports a 1-byte modifier bitmask followed by a 128-bit usage bitmap, so
/// every key can be held simultaneously. Modern Linux, macOS, and Windows all
/// understand this; older BIOSes may not — see
/// [`RolloverMode`](crate::keyboard::RolloverMode).
///
/// Wire format is [`NKRO_REPORT_SIZE`] bytes: `[modifiers, bitmap[0..16]]`.
pub const NKRO_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x06, // Usage (Keyboard)
    0xA1, 0x01, // Collection (Application)
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0xE0, //   Usage Minimum (LeftControl)
    0x29, 0xE7, //   Usage Maximum (Right GUI)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x08, //   Report Count (8)
    0x81, 0x02, //   Input (Data, Variable, Absolute) — modifier bits
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0x00, //   Usage Minimum (0)
    0x29, 0x7F, //   Usage Maximum (127)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x80, //   Report Count (128)
    0x81, 0x02, //   Input (Data, Variable, Absolute) — the key bitmap
    0xC0, // End Collection
];

/// Consumer-control (media key) HID report descriptor.
///
/// Reports a single 16-bit consumer usage ID, so exactly one media key is
/// active at a time. Wire format is [`CONSUMER_REPORT_SIZE`] bytes,
/// little-endian; all-zero means "nothing pressed".
pub const CONSUMER_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x0C, // Usage Page (Consumer)
    0x09, 0x01, // Usage (Consumer Control)
    0xA1, 0x01, // Collection (Application)
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xFF, 0x03, //   Logical Maximum (1023)
    0x19, 0x00, //   Usage Minimum (0)
    0x2A, 0xFF, 0x03, //   Usage Maximum (1023)
    0x75, 0x10, //   Report Size (16)
    0x95, 0x01, //   Report Count (1)
    0x81, 0x00, //   Input (Data, Array, Absolute)
    0xC0, // End Collection
];

/// Mouse HID report descriptor (3 buttons, X/Y movement, vertical wheel).
///
/// Wire format is [`MOUSE_REPORT_SIZE`] bytes:
/// `[buttons, dx_lo, dx_hi, dy_lo, dy_hi, scroll]`, with signed little-endian
/// 16-bit movement and signed 8-bit scrolling.
pub const MOUSE_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x02, // Usage (Mouse)
    0xA1, 0x01, // Collection (Application)
    0x09, 0x01, //   Usage (Pointer)
    0xA1, 0x00, //   Collection (Physical)
    0x05, 0x09, //     Usage Page (Button)
    0x19, 0x01, //     Usage Minimum (Button 1)
    0x29, 0x03, //     Usage Maximum (Button 3)
    0x15, 0x00, //     Logical Minimum (0)
    0x25, 0x01, //     Logical Maximum (1)
    0x95, 0x03, //     Report Count (3)
    0x75, 0x01, //     Report Size (1)
    0x81, 0x02, //     Input (Data, Variable, Absolute) — button bits
    0x95, 0x01, //     Report Count (1)
    0x75, 0x05, //     Report Size (5)
    0x81, 0x03, //     Input (Constant) — padding to a byte
    0x05, 0x01, //     Usage Page (Generic Desktop)
    0x09, 0x30, //     Usage (X)
    0x09, 0x31, //     Usage (Y)
    // 16-bit X/Y. High-resolution sensors overrun an 8-bit delta on a fast
    // flick — the ZSA Navigator's PAW3805EK is configured for 16-bit motion,
    // so an 8-bit field here would clamp its output and cap the pointer's top
    // speed below the sensor's.
    0x16, 0x01, 0x80, //     Logical Minimum (-32767)
    0x26, 0xFF, 0x7F, //     Logical Maximum (32767)
    0x75, 0x10, //     Report Size (16)
    0x95, 0x02, //     Report Count (2)
    0x81, 0x06, //     Input (Data, Variable, Relative)
    0x09, 0x38, //     Usage (Wheel)
    0x15, 0x81, //     Logical Minimum (-127)
    0x25, 0x7F, //     Logical Maximum (127)
    0x75, 0x08, //     Report Size (8)
    0x95, 0x01, //     Report Count (1)
    0x81, 0x06, //     Input (Data, Variable, Relative)
    0xC0, //   End Collection
    0xC0, // End Collection
];

/// Gamepad HID report descriptor (16 buttons, six 8-bit axes).
///
/// Wire format is [`GAMEPAD_REPORT_SIZE`] bytes: two little-endian button
/// bytes followed by the six axes.
pub const GAMEPAD_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x05, // Usage (Game Pad)
    0xA1, 0x01, // Collection (Application)
    0x05, 0x09, //   Usage Page (Button)
    0x19, 0x01, //   Usage Minimum (Button 1)
    0x29, 0x10, //   Usage Maximum (Button 16)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x10, //   Report Count (16)
    0x81, 0x02, //   Input (Data, Variable, Absolute) — button bits
    0x05, 0x01, //   Usage Page (Generic Desktop)
    0x09, 0x30, //   Usage (X)
    0x09, 0x31, //   Usage (Y)
    0x09, 0x32, //   Usage (Z)
    0x09, 0x33, //   Usage (Rx)
    0x09, 0x34, //   Usage (Ry)
    0x09, 0x35, //   Usage (Rz)
    0x15, 0x81, //   Logical Minimum (-127)
    0x25, 0x7F, //   Logical Maximum (127)
    0x75, 0x08, //   Report Size (8)
    0x95, 0x06, //   Report Count (6)
    0x81, 0x02, //   Input (Data, Variable, Absolute) — axes
    0xC0, // End Collection
];

/// Via raw-HID report descriptor (vendor usage page 0xFF60).
///
/// The transport the Via protocol runs over: a vendor-defined interface with a
/// 32-byte input report and a 32-byte output report. The usage page and usage
/// IDs are fixed by Via's host application — it locates the keyboard by
/// scanning for exactly this interface, so these values are not ours to pick.
///
/// Pairs with [`crate::storage::via::ViaProtocol`], which decodes the packets.
pub const VIA_REPORT_DESCRIPTOR: &[u8] = &[
    0x06, 0x60, 0xFF, // Usage Page (Vendor Defined 0xFF60)
    0x09, 0x61, // Usage (0x61)
    0xA1, 0x01, // Collection (Application)
    0x09, 0x62, //   Usage (0x62) — host-to-device data
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xFF, 0x00, //   Logical Maximum (255)
    0x95, 0x20, //   Report Count (32)
    0x75, 0x08, //   Report Size (8)
    0x81, 0x02, //   Input (Data, Variable, Absolute)
    0x09, 0x63, //   Usage (0x63) — device-to-host data
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xFF, 0x00, //   Logical Maximum (255)
    0x95, 0x20, //   Report Count (32)
    0x75, 0x08, //   Report Size (8)
    0x91, 0x02, //   Output (Data, Variable, Absolute)
    0xC0, // End Collection
];

/// Wire size of a Via raw-HID packet, in each direction.
pub const VIA_REPORT_SIZE: usize = 32;

/// Maximum number of simultaneous non-modifier keycodes in a 6KRO report.
pub const KRO_KEYS: usize = 6;

/// Byte width of an NKRO bitmap report (128 HID usage IDs / 8 bits).
pub const NKRO_BYTES: usize = 16;

/// Wire size of a 6KRO boot-protocol keyboard report.
pub const KEYBOARD_REPORT_SIZE: usize = 8;

/// Wire size of an NKRO keyboard report (1 modifier byte + 16 bitmap bytes).
pub const NKRO_REPORT_SIZE: usize = 1 + NKRO_BYTES;

/// Wire size of a consumer-control report (one little-endian u16).
pub const CONSUMER_REPORT_SIZE: usize = 2;

/// Wire size of a mouse report (buttons, signed 16-bit dx/dy, signed 8-bit scroll).
pub const MOUSE_REPORT_SIZE: usize = 6;

/// Wire size of a gamepad report (two button bytes + six axes).
pub const GAMEPAD_REPORT_SIZE: usize = 8;

/// Wire size of a USB-MIDI event packet (cable/CIN header + 3 data bytes).
pub const MIDI_PACKET_SIZE: usize = 4;

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

    /// Encodes this report in the 8-byte boot-protocol wire format.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; KEYBOARD_REPORT_SIZE] {
        [
            self.modifiers,
            self.reserved,
            self.keycodes[0],
            self.keycodes[1],
            self.keycodes[2],
            self.keycodes[3],
            self.keycodes[4],
            self.keycodes[5],
        ]
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

    /// Encodes this report in the wire format described by
    /// [`NKRO_REPORT_DESCRIPTOR`]: the modifier byte followed by the bitmap.
    #[must_use]
    pub fn to_bytes(self) -> [u8; NKRO_REPORT_SIZE] {
        let mut out = [0u8; NKRO_REPORT_SIZE];
        out[0] = self.modifiers;
        out[1..].copy_from_slice(&self.bitmap);
        out
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

    /// Encodes this report as a little-endian consumer usage ID.
    ///
    /// `None` encodes as `0x0000`, which the host reads as "released".
    #[must_use]
    pub fn to_bytes(self) -> [u8; CONSUMER_REPORT_SIZE] {
        self.keycode
            .map_or(0u16, ConsumerCode::usage_id)
            .to_le_bytes()
    }
}

/// HID mouse report for cursor movement, clicks, and scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseReport {
    /// Button bitmask (bit 0 = left, bit 1 = right, bit 2 = middle).
    pub buttons: u8,
    /// Horizontal movement (positive = right).
    ///
    /// 16-bit: an optical sensor at speed produces deltas well past 127 in a
    /// single poll, and narrowing here would cap the pointer's top speed.
    pub dx: i16,
    /// Vertical movement (positive = down).
    pub dy: i16,
    /// Vertical scroll (positive = up).
    pub scroll: i8,
}

impl MouseReport {
    /// Encodes this report in the wire format described by
    /// [`MOUSE_REPORT_DESCRIPTOR`].
    ///
    /// X and Y go out little-endian, which is what HID expects for a
    /// multi-byte field.
    #[must_use]
    #[expect(
        clippy::cast_sign_loss,
        reason = "i8 to u8 is the HID wire representation, two's complement preserved"
    )]
    #[expect(clippy::as_conversions, reason = "i8 -> u8 bit reinterpretation")]
    pub const fn to_bytes(self) -> [u8; MOUSE_REPORT_SIZE] {
        let [dx_lo, dx_hi] = self.dx.to_le_bytes();
        let [dy_lo, dy_hi] = self.dy.to_le_bytes();
        [self.buttons, dx_lo, dx_hi, dy_lo, dy_hi, self.scroll as u8]
    }
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
    /// Polyphonic key pressure (per-note aftertouch).
    PolyPressure {
        /// MIDI channel (0–15).
        channel: u8,
        /// MIDI note number (0–127).
        note: u8,
        /// Pressure (0–127).
        pressure: u8,
    },
    /// Channel pressure (single aftertouch value for the whole channel).
    ChannelPressure {
        /// MIDI channel (0–15).
        channel: u8,
        /// Pressure (0–127).
        pressure: u8,
    },
}

impl MidiEvent {
    /// Encodes this event as a 4-byte USB-MIDI event packet.
    ///
    /// Byte 0 is the cable number (always 0 — tecla exposes a single virtual
    /// cable) in the high nibble and the Code Index Number in the low nibble;
    /// the CIN always matches the high nibble of the status byte for these
    /// channel-voice messages. Bytes 1–3 are the MIDI message itself, padded
    /// with 0 for two-byte messages.
    ///
    /// Channel is masked to 0–15 and data bytes to 0–127, so an out-of-range
    /// value corrupts one note rather than the whole MIDI stream.
    #[must_use]
    pub const fn to_usb_packet(self) -> [u8; MIDI_PACKET_SIZE] {
        // (status high nibble, channel, data1, data2)
        let (status, channel, d1, d2) = match self {
            Self::NoteOff { channel, note } => (0x8, channel, note, 0),
            Self::NoteOn {
                channel,
                note,
                velocity,
            } => (0x9, channel, note, velocity),
            Self::PolyPressure {
                channel,
                note,
                pressure,
            } => (0xA, channel, note, pressure),
            Self::Cc {
                channel,
                controller,
                value,
            } => (0xB, channel, controller, value),
            Self::ChannelPressure { channel, pressure } => (0xD, channel, pressure, 0),
        };
        [
            status, // cable 0, CIN == status nibble
            (status << 4) | (channel & 0x0F),
            d1 & 0x7F,
            d2 & 0x7F,
        ]
    }
}

/// A HID gamepad report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamepadReport {
    /// Button bitmask (up to 16 buttons).
    pub buttons: u16,
    /// Axis values: x, y, z, rx, ry, rz (−128 to 127 each).
    pub axes: [i8; 6],
}

impl GamepadReport {
    /// Encodes this report in the wire format described by
    /// [`GAMEPAD_REPORT_DESCRIPTOR`]: little-endian buttons, then the axes.
    #[must_use]
    #[expect(
        clippy::cast_sign_loss,
        reason = "i8 to u8 is the HID wire representation, two's complement preserved"
    )]
    #[expect(clippy::as_conversions, reason = "i8 -> u8 bit reinterpretation")]
    pub const fn to_bytes(self) -> [u8; GAMEPAD_REPORT_SIZE] {
        let [b0, b1] = self.buttons.to_le_bytes();
        [
            b0,
            b1,
            self.axes[0] as u8,
            self.axes[1] as u8,
            self.axes[2] as u8,
            self.axes[3] as u8,
            self.axes[4] as u8,
            self.axes[5] as u8,
        ]
    }
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
}

#[cfg(test)]
mod wire_format_tests {
    use super::*;
    use crate::action::ModifierKey;

    // ── Report descriptors ───────────────────────────────────────────────────
    //
    // A descriptor that does not parse makes the host silently ignore the
    // interface — the device enumerates and then simply never works. These
    // walk the item stream to catch the mistakes that cause that.

    /// Walks a HID report descriptor, returning (collection depth balance,
    /// total input-report bits declared at the top level).
    fn walk(desc: &[u8]) -> (i32, u32) {
        let mut i = 0;
        let mut depth = 0i32;
        let mut bits = 0u32;
        let (mut size, mut count) = (0u32, 0u32);
        while i < desc.len() {
            let prefix = desc[i];
            let len = match prefix & 0x03 {
                3 => 4,
                n => n as usize,
            };
            let data: u32 = desc[i + 1..i + 1 + len]
                .iter()
                .enumerate()
                .map(|(n, &b)| u32::from(b) << (8 * n))
                .sum();
            match prefix & 0xFC {
                0xA0 => depth += 1,           // Collection
                0xC0 => depth -= 1,           // End Collection
                0x74 => size = data,          // Report Size
                0x94 => count = data,         // Report Count
                0x80 => bits += size * count, // Input
                _ => {}
            }
            i += 1 + len;
        }
        (depth, bits)
    }

    #[test]
    fn every_descriptor_is_well_formed_and_matches_its_wire_size() {
        let cases: [(&str, &[u8], usize); 6] = [
            ("keyboard", KEYBOARD_REPORT_DESCRIPTOR, KEYBOARD_REPORT_SIZE),
            ("nkro", NKRO_REPORT_DESCRIPTOR, NKRO_REPORT_SIZE),
            ("consumer", CONSUMER_REPORT_DESCRIPTOR, CONSUMER_REPORT_SIZE),
            ("mouse", MOUSE_REPORT_DESCRIPTOR, MOUSE_REPORT_SIZE),
            ("gamepad", GAMEPAD_REPORT_DESCRIPTOR, GAMEPAD_REPORT_SIZE),
            ("via", VIA_REPORT_DESCRIPTOR, VIA_REPORT_SIZE),
        ];
        for (name, desc, wire_size) in cases {
            let (depth, bits) = walk(desc);
            assert_eq!(depth, 0, "{name}: unbalanced Collection/End Collection");
            assert_eq!(
                bits / 8,
                wire_size as u32,
                "{name}: descriptor declares {bits} input bits but the wire size is {wire_size} bytes"
            );
        }
    }

    // ── Keyboard ─────────────────────────────────────────────────────────────

    #[test]
    fn keyboard_report_wire_layout() {
        let mut r = KeyboardReport::new();
        r.modifiers = ModifierKey::LSHIFT.bits();
        r.keycodes = [4, 5, 6, 0, 0, 0];
        assert_eq!(r.to_bytes(), [r.modifiers, 0, 4, 5, 6, 0, 0, 0]);
    }

    #[test]
    fn empty_keyboard_report_is_all_zeros() {
        assert_eq!(KeyboardReport::new().to_bytes(), [0; KEYBOARD_REPORT_SIZE]);
    }

    // ── NKRO ─────────────────────────────────────────────────────────────────

    #[test]
    fn nkro_report_puts_modifiers_first_then_the_bitmap() {
        let mut r = NkroKeyboardReport::new();
        r.modifiers = 0x02;
        r.press(4); // byte 0, bit 4
        r.press(9); // byte 1, bit 1
        let bytes = r.to_bytes();
        assert_eq!(bytes[0], 0x02);
        assert_eq!(bytes[1], 1 << 4);
        assert_eq!(bytes[2], 1 << 1);
        assert!(bytes[3..].iter().all(|&b| b == 0));
    }

    #[test]
    fn nkro_can_hold_far_more_than_six_keys() {
        let mut r = NkroKeyboardReport::new();
        for kc in 4..=40u8 {
            r.press(kc);
        }
        let set: u32 = r.to_bytes()[1..].iter().map(|b| b.count_ones()).sum();
        assert_eq!(set, 37, "all 37 keys should survive the encoding");
    }

    // ── Consumer ─────────────────────────────────────────────────────────────

    #[test]
    fn consumer_report_is_a_little_endian_usage_id() {
        use crate::action::ConsumerCode;
        let r = ConsumerReport {
            keycode: Some(ConsumerCode::VolumeUp),
        };
        assert_eq!(r.to_bytes(), 0x00E9u16.to_le_bytes());
    }

    #[test]
    fn empty_consumer_report_releases_the_key() {
        assert_eq!(ConsumerReport::new().to_bytes(), [0, 0]);
    }

    // ── Mouse ────────────────────────────────────────────────────────────────

    #[test]
    fn mouse_report_preserves_negative_movement() {
        let r = MouseReport {
            buttons: 0b101,
            dx: -1,
            dy: -128,
            scroll: 3,
        };
        // -1 and -128 must round-trip as two's complement, not saturate to 0.
        // X/Y are little-endian 16-bit, so the sign lives in the high byte.
        assert_eq!(r.to_bytes(), [0b101, 0xFF, 0xFF, 0x80, 0xFF, 3]);
    }

    #[test]
    fn mouse_report_carries_full_width_deltas() {
        // An optical sensor at speed can exceed 127 counts in one poll.
        let r = MouseReport {
            buttons: 0,
            dx: 1000,
            dy: -1000,
            scroll: 0,
        };
        let bytes = r.to_bytes();
        assert_eq!(i16::from_le_bytes([bytes[1], bytes[2]]), 1000);
        assert_eq!(i16::from_le_bytes([bytes[3], bytes[4]]), -1000);
    }

    // ── Gamepad ──────────────────────────────────────────────────────────────

    #[test]
    fn gamepad_report_encodes_buttons_little_endian_then_axes() {
        let r = GamepadReport {
            buttons: 0x8001, // button 1 and button 16
            axes: [127, -128, 0, 1, -1, 42],
        };
        assert_eq!(r.to_bytes(), [0x01, 0x80, 127, 0x80, 0, 1, 0xFF, 42]);
    }

    // ── MIDI ─────────────────────────────────────────────────────────────────

    #[test]
    fn midi_note_on_matches_the_usb_midi_packet_format() {
        let p = MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        }
        .to_usb_packet();
        // CIN 0x9 on cable 0; status 0x90 (note-on, channel 0); note; velocity.
        assert_eq!(p, [0x09, 0x90, 60, 100]);
    }

    #[test]
    fn midi_status_nibble_and_channel_are_packed_into_byte_one() {
        let p = MidiEvent::Cc {
            channel: 5,
            controller: 7,
            value: 64,
        }
        .to_usb_packet();
        assert_eq!(p, [0x0B, 0xB5, 7, 64]);
    }

    #[test]
    fn midi_two_byte_messages_pad_with_zero() {
        assert_eq!(
            MidiEvent::NoteOff {
                channel: 1,
                note: 60
            }
            .to_usb_packet(),
            [0x08, 0x81, 60, 0]
        );
        assert_eq!(
            MidiEvent::ChannelPressure {
                channel: 2,
                pressure: 90
            }
            .to_usb_packet(),
            [0x0D, 0xD2, 90, 0]
        );
    }

    #[test]
    fn midi_aftertouch_uses_the_poly_pressure_status() {
        assert_eq!(
            MidiEvent::PolyPressure {
                channel: 0,
                note: 60,
                pressure: 77
            }
            .to_usb_packet(),
            [0x0A, 0xA0, 60, 77]
        );
    }

    #[test]
    fn midi_out_of_range_values_are_masked_not_allowed_to_corrupt_the_stream() {
        // A data byte with bit 7 set would be read by the host as a *status*
        // byte and desynchronise everything after it.
        let p = MidiEvent::NoteOn {
            channel: 0xFF,
            note: 0xFF,
            velocity: 0xFF,
        }
        .to_usb_packet();
        assert_eq!(p, [0x09, 0x9F, 0x7F, 0x7F]);
        assert_eq!(p[2] & 0x80, 0, "data byte must not look like a status byte");
        assert_eq!(p[3] & 0x80, 0, "data byte must not look like a status byte");
    }

    #[test]
    fn every_midi_variant_encodes_a_distinct_status() {
        let events = [
            MidiEvent::NoteOff {
                channel: 0,
                note: 1,
            },
            MidiEvent::NoteOn {
                channel: 0,
                note: 1,
                velocity: 1,
            },
            MidiEvent::PolyPressure {
                channel: 0,
                note: 1,
                pressure: 1,
            },
            MidiEvent::Cc {
                channel: 0,
                controller: 1,
                value: 1,
            },
            MidiEvent::ChannelPressure {
                channel: 0,
                pressure: 1,
            },
        ];
        let mut statuses = [0u8; 5];
        for (slot, e) in statuses.iter_mut().zip(events) {
            *slot = e.to_usb_packet()[1];
        }
        for i in 0..statuses.len() {
            for j in (i + 1)..statuses.len() {
                assert_ne!(statuses[i], statuses[j], "duplicate MIDI status byte");
            }
        }
    }
}
