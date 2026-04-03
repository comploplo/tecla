//! Action types — what a key does when activated.
//!
//! [`Action`] is the central type connecting input detection to HID output.
//! It is generic over an extension type `Ext` (default `()`) so board crates
//! can add custom actions (MIDI, gamepad, macros) without modifying core.

// ── ModifierKey ──────────────────────────────────────────────────────────────

/// A bitmask of HID keyboard modifier keys.
///
/// Multiple modifiers can be combined with [`ModifierKey::union`].
/// The bit layout matches the HID modifier byte directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierKey(u8);

impl ModifierKey {
    /// Left Alt.
    pub const LALT: Self = Self(0x04);
    /// Left Control.
    pub const LCTRL: Self = Self(0x01);
    /// Left GUI (Win / Cmd / Super).
    pub const LGUI: Self = Self(0x08);
    /// Left Shift.
    pub const LSHIFT: Self = Self(0x02);
    /// No modifiers active.
    pub const NONE: Self = Self(0x00);
    /// Right Alt (AltGr).
    pub const RALT: Self = Self(0x40);
    /// Right Control.
    pub const RCTRL: Self = Self(0x10);
    /// Right GUI.
    pub const RGUI: Self = Self(0x80);
    /// Right Shift.
    pub const RSHIFT: Self = Self(0x20);

    /// Returns the raw bitmask byte, suitable for the HID modifier field.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Returns `true` if `self` contains all bits of `other`.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.intersection(other).0 == other.0
    }

    /// Returns the intersection of `self` and `other` (bitwise AND).
    #[inline]
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Returns `true` if no modifier bits are set.
    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns the union of `self` and `other` (bitwise OR).
    #[inline]
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns `self` with all bits of `other` cleared.
    #[inline]
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

// ── KeyCode ───────────────────────────────────────────────────────────────────

/// A standard HID keyboard keycode.
///
/// Discriminants match the USB HID keyboard usage page (0x07).
/// Modifier keys (0xE0–0xE7) are included; the report builder will split
/// them into the modifier byte automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum KeyCode {
    /// No key.
    None = 0x00,

    // Letters
    /// Letter A.
    A = 0x04,
    /// Letter B.
    B = 0x05,
    /// Letter C.
    C = 0x06,
    /// Letter D.
    D = 0x07,
    /// Letter E.
    E = 0x08,
    /// Letter F.
    F = 0x09,
    /// Letter G.
    G = 0x0A,
    /// Letter H.
    H = 0x0B,
    /// Letter I.
    I = 0x0C,
    /// Letter J.
    J = 0x0D,
    /// Letter K.
    K = 0x0E,
    /// Letter L.
    L = 0x0F,
    /// Letter M.
    M = 0x10,
    /// Letter N.
    N = 0x11,
    /// Letter O.
    O = 0x12,
    /// Letter P.
    P = 0x13,
    /// Letter Q.
    Q = 0x14,
    /// Letter R.
    R = 0x15,
    /// Letter S.
    S = 0x16,
    /// Letter T.
    T = 0x17,
    /// Letter U.
    U = 0x18,
    /// Letter V.
    V = 0x19,
    /// Letter W.
    W = 0x1A,
    /// Letter X.
    X = 0x1B,
    /// Letter Y.
    Y = 0x1C,
    /// Letter Z.
    Z = 0x1D,

    // Digits
    /// Digit 1.
    N1 = 0x1E,
    /// Digit 2.
    N2 = 0x1F,
    /// Digit 3.
    N3 = 0x20,
    /// Digit 4.
    N4 = 0x21,
    /// Digit 5.
    N5 = 0x22,
    /// Digit 6.
    N6 = 0x23,
    /// Digit 7.
    N7 = 0x24,
    /// Digit 8.
    N8 = 0x25,
    /// Digit 9.
    N9 = 0x26,
    /// Digit 0.
    N0 = 0x27,

    // Common keys
    /// Enter.
    Enter = 0x28,
    /// Escape.
    Escape = 0x29,
    /// Backspace.
    Backspace = 0x2A,
    /// Tab.
    Tab = 0x2B,
    /// Space.
    Space = 0x2C,
    /// Minus / hyphen.
    Minus = 0x2D,
    /// Equal.
    Equal = 0x2E,
    /// Left brace.
    LeftBrace = 0x2F,
    /// Right brace.
    RightBrace = 0x30,
    /// Backslash.
    Backslash = 0x31,
    /// Semicolon.
    Semicolon = 0x33,
    /// Quote.
    Quote = 0x34,
    /// Grave accent.
    Grave = 0x35,
    /// Comma.
    Comma = 0x36,
    /// Period / dot.
    Dot = 0x37,
    /// Forward slash.
    Slash = 0x38,
    /// Caps Lock.
    CapsLock = 0x39,

    // Function keys
    /// F1.
    F1 = 0x3A,
    /// F2.
    F2 = 0x3B,
    /// F3.
    F3 = 0x3C,
    /// F4.
    F4 = 0x3D,
    /// F5.
    F5 = 0x3E,
    /// F6.
    F6 = 0x3F,
    /// F7.
    F7 = 0x40,
    /// F8.
    F8 = 0x41,
    /// F9.
    F9 = 0x42,
    /// F10.
    F10 = 0x43,
    /// F11.
    F11 = 0x44,
    /// F12.
    F12 = 0x45,

    // Navigation & editing
    /// Print Screen.
    PrintScreen = 0x46,
    /// Scroll Lock.
    ScrollLock = 0x47,
    /// Pause / Break.
    Pause = 0x48,
    /// Insert.
    Insert = 0x49,
    /// Home.
    Home = 0x4A,
    /// Page Up.
    PageUp = 0x4B,
    /// Delete.
    Delete = 0x4C,
    /// End.
    End = 0x4D,
    /// Page Down.
    PageDown = 0x4E,
    /// Right arrow.
    Right = 0x4F,
    /// Left arrow.
    Left = 0x50,
    /// Down arrow.
    Down = 0x51,
    /// Up arrow.
    Up = 0x52,

    // Numpad
    /// Num Lock.
    NumLock = 0x53,
    /// Keypad slash.
    KpSlash = 0x54,
    /// Keypad asterisk.
    KpAster = 0x55,
    /// Keypad minus.
    KpMinus = 0x56,
    /// Keypad plus.
    KpPlus = 0x57,
    /// Keypad Enter.
    KpEnter = 0x58,
    /// Keypad 1.
    Kp1 = 0x59,
    /// Keypad 2.
    Kp2 = 0x5A,
    /// Keypad 3.
    Kp3 = 0x5B,
    /// Keypad 4.
    Kp4 = 0x5C,
    /// Keypad 5.
    Kp5 = 0x5D,
    /// Keypad 6.
    Kp6 = 0x5E,
    /// Keypad 7.
    Kp7 = 0x5F,
    /// Keypad 8.
    Kp8 = 0x60,
    /// Keypad 9.
    Kp9 = 0x61,
    /// Keypad 0.
    Kp0 = 0x62,
    /// Keypad dot.
    KpDot = 0x63,

    // Modifiers
    /// Left Control modifier key.
    LCtrl = 0xE0,
    /// Left Shift modifier key.
    LShift = 0xE1,
    /// Left Alt modifier key.
    LAlt = 0xE2,
    /// Left GUI modifier key.
    LGui = 0xE3,
    /// Right Control modifier key.
    RCtrl = 0xE4,
    /// Right Shift modifier key.
    RShift = 0xE5,
    /// Right Alt modifier key.
    RAlt = 0xE6,
    /// Right GUI modifier key.
    RGui = 0xE7,
}

impl KeyCode {
    /// Returns the [`ModifierKey`] bitmask if this is a modifier, otherwise `None`.
    #[inline]
    #[must_use]
    pub const fn as_modifier(self) -> Option<ModifierKey> {
        let id = self.usage_id();
        if id < 0xE0 {
            return None;
        }
        Some(ModifierKey(1 << (id - 0xE0)))
    }

    /// Returns `true` if this keycode is a modifier key (usage 0xE0–0xE7).
    #[inline]
    #[must_use]
    pub const fn is_modifier(self) -> bool {
        self.usage_id() >= 0xE0
    }

    /// Returns the raw HID usage ID.
    #[inline]
    #[must_use]
    pub const fn usage_id(self) -> u8 {
        self as u8
    }
}

// ── ConsumerCode ─────────────────────────────────────────────────────────────

/// A USB HID consumer-control keycode (media, brightness, etc.).
///
/// Discriminants match the HID consumer usage page (0x0C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ConsumerCode {
    /// Brightness decrease.
    BrightnessDown = 0x0070,
    /// Brightness increase.
    BrightnessUp = 0x006F,
    /// Eject disc.
    Eject = 0x00B8,
    /// Fast forward.
    FastForward = 0x00B3,
    /// Mute audio.
    Mute = 0x00E2,
    /// Next track.
    NextTrack = 0x00B5,
    /// Play / pause.
    Play = 0x00CD,
    /// Previous track.
    PrevTrack = 0x00B6,
    /// Rewind.
    Rewind = 0x00B4,
    /// Stop playback.
    Stop = 0x00B7,
    /// Volume decrease.
    VolumeDown = 0x00EA,
    /// Volume increase.
    VolumeUp = 0x00E9,
}

impl ConsumerCode {
    /// Returns the raw HID usage ID.
    #[inline]
    #[must_use]
    pub const fn usage_id(self) -> u16 {
        self as u16
    }
}

// ── LayerOp ───────────────────────────────────────────────────────────────────

/// An operation to perform on the layer stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerOp {
    /// Activate while held; deactivate on release (QMK `MO`).
    Momentary(u8),
    /// Unconditionally deactivate.
    Off(u8),
    /// Unconditionally activate (QMK `TO`).
    On(u8),
    /// Change the default base layer (QMK `DF`).
    SetDefault(u8),
    /// Toggle on or off (QMK `TG`).
    Toggle(u8),
}

// ── Action ────────────────────────────────────────────────────────────────────

/// What a key does when activated.
///
/// Generic over `Ext` (defaults to `()`) — board crates supply their own
/// type here for MIDI, gamepad events, or anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action<Ext = ()>
where
    Ext: Copy + PartialEq,
{
    /// Emit a consumer-control keycode.
    Consumer(ConsumerCode),
    /// Custom extension action (MIDI, gamepad, macro, etc.).
    Ext(Ext),
    /// Emit a standard HID keycode.
    Key(KeyCode),
    /// Perform a layer stack operation.
    Layer(LayerOp),
    /// Layer-tap: hold = momentary layer, tap = keycode.
    LayerTap {
        /// Layer activated while held.
        layer: u8,
        /// Keycode emitted on tap.
        tap: KeyCode,
    },
    /// Apply or release a modifier.
    Modifier(ModifierKey),
    /// Mod-tap: hold = modifier, tap = keycode.
    ModTap {
        /// Modifier applied while held.
        hold: ModifierKey,
        /// Keycode emitted on tap.
        tap: KeyCode,
    },
    /// Do nothing; blocks transparent fallthrough (QMK `KC_NO`).
    NoOp,
    /// One-shot layer: active for the next keypress only.
    OneShotLayer(u8),
    /// One-shot modifier: applies once to the next keypress, then clears.
    OneShotMod(ModifierKey),
    /// Fall through to the next active layer (QMK `KC_TRNS`).
    Transparent,
}

impl<Ext: Copy + PartialEq> Action<Ext> {
    /// Returns `true` if this action requires tap-vs-hold disambiguation.
    #[inline]
    #[must_use]
    pub const fn needs_tap_hold(&self) -> bool {
        matches!(self, Self::ModTap { .. } | Self::LayerTap { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_none_is_empty() {
        assert!(ModifierKey::NONE.is_empty());
    }

    #[test]
    fn modifier_lctrl_is_not_empty() {
        assert!(!ModifierKey::LCTRL.is_empty());
    }

    #[test]
    fn modifier_default_equals_none() {
        assert_eq!(ModifierKey::default(), ModifierKey::NONE);
    }

    #[test]
    fn modifier_constants_are_all_distinct() {
        let all = [
            ModifierKey::LCTRL,
            ModifierKey::LSHIFT,
            ModifierKey::LALT,
            ModifierKey::LGUI,
            ModifierKey::RCTRL,
            ModifierKey::RSHIFT,
            ModifierKey::RALT,
            ModifierKey::RGUI,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn modifier_union_sets_both_bits() {
        let both = ModifierKey::LCTRL.union(ModifierKey::LSHIFT);
        assert_eq!(both.bits(), 0x03);
    }

    #[test]
    fn modifier_contains_its_own_bits() {
        let both = ModifierKey::LCTRL.union(ModifierKey::LSHIFT);
        assert!(both.contains(ModifierKey::LCTRL));
        assert!(both.contains(ModifierKey::LSHIFT));
    }

    #[test]
    fn modifier_does_not_contain_absent_bit() {
        assert!(!ModifierKey::LCTRL.contains(ModifierKey::LSHIFT));
    }

    #[test]
    fn modifier_without_removes_specified_bit() {
        let both = ModifierKey::LCTRL.union(ModifierKey::LSHIFT);
        assert_eq!(both.without(ModifierKey::LSHIFT), ModifierKey::LCTRL);
    }

    #[test]
    fn modifier_intersection_returns_shared_bits() {
        let a = ModifierKey::LCTRL.union(ModifierKey::LSHIFT);
        let b = ModifierKey::LSHIFT.union(ModifierKey::LALT);
        assert_eq!(a.intersection(b), ModifierKey::LSHIFT);
    }

    #[test]
    fn keycode_a_usage_id_is_four() {
        assert_eq!(KeyCode::A.usage_id(), 0x04);
    }

    #[test]
    fn keycode_letters_span_twenty_six_consecutive_ids() {
        assert_eq!(KeyCode::Z.usage_id(), KeyCode::A.usage_id() + 25);
    }

    #[test]
    fn keycode_none_usage_id_is_zero() {
        assert_eq!(KeyCode::None.usage_id(), 0x00);
    }

    #[test]
    fn modifier_keycodes_are_detected_as_modifiers() {
        assert!(KeyCode::LCtrl.is_modifier());
        assert!(KeyCode::LShift.is_modifier());
        assert!(KeyCode::RAlt.is_modifier());
        assert!(KeyCode::RGui.is_modifier());
    }

    #[test]
    fn non_modifier_keycodes_are_not_modifiers() {
        assert!(!KeyCode::A.is_modifier());
        assert!(!KeyCode::Space.is_modifier());
        assert!(!KeyCode::F12.is_modifier());
    }

    #[test]
    fn all_modifier_keycodes_map_to_correct_bitmasks() {
        let pairs = [
            (KeyCode::LCtrl, ModifierKey::LCTRL),
            (KeyCode::LShift, ModifierKey::LSHIFT),
            (KeyCode::LAlt, ModifierKey::LALT),
            (KeyCode::LGui, ModifierKey::LGUI),
            (KeyCode::RCtrl, ModifierKey::RCTRL),
            (KeyCode::RShift, ModifierKey::RSHIFT),
            (KeyCode::RAlt, ModifierKey::RALT),
            (KeyCode::RGui, ModifierKey::RGUI),
        ];
        for (kc, expected) in pairs {
            assert_eq!(kc.as_modifier(), Some(expected));
        }
    }

    #[test]
    fn non_modifier_keycode_as_modifier_is_none() {
        assert_eq!(KeyCode::A.as_modifier(), None);
        assert_eq!(KeyCode::Space.as_modifier(), None);
    }

    #[test]
    fn consumer_volume_up_has_correct_usage_id() {
        assert_eq!(ConsumerCode::VolumeUp.usage_id(), 0x00E9);
    }

    #[test]
    fn consumer_codes_are_distinct_from_each_other() {
        assert_ne!(
            ConsumerCode::VolumeUp.usage_id(),
            ConsumerCode::VolumeDown.usage_id()
        );
        assert_ne!(ConsumerCode::Play.usage_id(), ConsumerCode::Stop.usage_id());
    }

    #[test]
    fn layer_ops_carry_their_index() {
        assert_eq!(LayerOp::Momentary(3), LayerOp::Momentary(3));
        assert_ne!(LayerOp::Momentary(1), LayerOp::Momentary(2));
        assert_ne!(LayerOp::Toggle(1), LayerOp::Momentary(1));
        assert_ne!(LayerOp::On(1), LayerOp::Off(1));
    }

    #[test]
    fn mod_tap_needs_tap_hold() {
        let a = Action::<()>::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        };
        assert!(a.needs_tap_hold());
    }

    #[test]
    fn layer_tap_needs_tap_hold() {
        let a = Action::<()>::LayerTap {
            layer: 1,
            tap: KeyCode::Space,
        };
        assert!(a.needs_tap_hold());
    }

    #[test]
    fn all_other_actions_do_not_need_tap_hold() {
        let actions: &[Action<()>] = &[
            Action::Transparent,
            Action::NoOp,
            Action::Key(KeyCode::A),
            Action::Consumer(ConsumerCode::VolumeUp),
            Action::Modifier(ModifierKey::LSHIFT),
            Action::Layer(LayerOp::Momentary(1)),
            Action::OneShotMod(ModifierKey::LCTRL),
            Action::OneShotLayer(2),
            Action::Ext(()),
        ];
        for a in actions {
            assert!(!a.needs_tap_hold(), "{a:?} should not need tap-hold");
        }
    }

    #[test]
    fn action_ext_equality_uses_inner_type() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        struct MidiNote(u8);
        assert_eq!(Action::Ext(MidiNote(60)), Action::Ext(MidiNote(60)));
        assert_ne!(Action::Ext(MidiNote(60)), Action::Ext(MidiNote(61)));
    }
}
