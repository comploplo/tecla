//! Action types — what a key does when activated.
//!
//! [`Action`] is the central type connecting input detection to HID output.
//! It is generic over an extension type `Ext` (default `()`) so board crates
//! can add custom actions (MIDI, gamepad, macros) without modifying core.

use super::{ConsumerCode, KeyCode};

// ── ModifierKey ──────────────────────────────────────────────────────────────

/// A bitmask of HID keyboard modifier keys.
///
/// Multiple modifiers can be combined with [`ModifierKey::union`].
/// The bit layout matches the HID modifier byte directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierKey(u8);

impl ModifierKey {
    pub(crate) const fn from_bit_index(index: u8) -> Self {
        Self(1 << index)
    }

    /// Creates a modifier mask from raw HID modifier bits.
    #[inline]
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

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

// ── MouseAction ──────────────────────────────────────────────────────────────

/// A mouse action emitted as a HID mouse report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseAction {
    /// Move the cursor by the given delta.
    Move {
        /// Horizontal movement (positive = right).
        dx: i8,
        /// Vertical movement (positive = down).
        dy: i8,
    },
    /// Press a mouse button (1 = left, 2 = right, 3 = middle).
    Click(u8),
    /// Scroll the wheel (positive = up).
    Scroll(i8),
}

// ── MidiAction ───────────────────────────────────────────────────────────────

/// A MIDI action emitted as a MIDI event report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MidiAction {
    /// Send a MIDI note-on message.
    NoteOn {
        /// MIDI channel (0–15).
        channel: u8,
        /// MIDI note number (0–127).
        note: u8,
        /// Velocity (0–127).
        velocity: u8,
    },
    /// Send a MIDI note-off message.
    NoteOff {
        /// MIDI channel (0–15).
        channel: u8,
        /// MIDI note number (0–127).
        note: u8,
    },
    /// Send a MIDI control-change message.
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

// ── GamepadAction ────────────────────────────────────────────────────────────

/// A gamepad action emitted as a HID gamepad report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadAction {
    /// Press a gamepad button (0–15).
    ButtonPress(u8),
    /// Set a gamepad axis value.
    AxisSet {
        /// Axis index (0–5: x, y, z, rx, ry, rz).
        axis: u8,
        /// Axis value (−128 to 127).
        value: i8,
    },
}

// ── DksKeys ──────────────────────────────────────────────────────────────────

/// The up-to-4 keycodes fired across one key's physical travel by
/// [`Action::Dks`] (Dynamic Keystroke).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DksKeys {
    /// Fired when the key crosses its actuation threshold.
    pub press: KeyCode,
    /// Fired when the key reaches the deep "bottomed out" threshold.
    pub bottom_out: KeyCode,
    /// Fired when the key backs off the bottom-out threshold while still held.
    pub release_from_bottom: KeyCode,
    /// Fired when the key crosses its release threshold.
    pub release: KeyCode,
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
    /// Activate the next layer above the current highest active layer.
    Next,
    /// Activate the previous layer below the current highest active layer.
    Prev,
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
    /// Accept the current autocomplete suggestion.
    AutocompleteAccept,
    /// Toggle autocomplete on/off.
    AutocompleteToggle,
    /// Toggle autocorrect on/off.
    AutocorrectToggle,
    /// Toggle bounce keys on/off (ignore rapid re-presses of the same key).
    BounceKeysToggle,
    /// Emit an audio control command (play tone, melody, stop, toggle).
    Audio(crate::audio::AudioCommand),
    /// Start BLE advertising.
    BleAdvertise,
    /// Disconnect the active BLE connection.
    BleDisconnect,
    /// Switch to a bonded BLE host by index.
    BleHostSwitch(u8),
    /// Enter bootloader or wipe config (board crate handles the actual jump).
    Bootmagic,
    /// Activate Caps Word mode: auto-shifts A–Z, deactivates on non-continue keys.
    CapsWord,
    /// Emit a consumer-control keycode.
    Consumer(ConsumerCode),
    /// Dynamic Keystroke: up to 4 distinct keycodes across one key's physical
    /// travel (press, bottom-out, release-from-bottom, release) — for
    /// analog/Hall-effect switches. Only Press and Release fire on ordinary
    /// digital switches; BottomOut/ReleaseFromBottom require an analog
    /// scanner feeding [`crate::keyboard::KeyboardProcessor::process_dks_phase`].
    Dks(DksKeys),
    /// Custom extension action (MIDI, gamepad, macro, etc.).
    Ext(Ext),
    /// Emit a gamepad button press or axis change.
    Gamepad(GamepadAction),
    /// Emit a haptic feedback command.
    Haptic(crate::haptic::HapticCommand),
    /// Emit a standard HID keycode.
    Key(KeyCode),
    /// Perform a layer stack operation.
    Layer(LayerOp),
    /// Layer + modifier: activates layer and holds modifier while pressed.
    LayerMod {
        /// Layer to activate.
        layer: u8,
        /// Modifier to hold.
        modifier: ModifierKey,
    },
    /// Lock or unlock a layer (persistent until toggled off).
    LayerLock(u8),
    /// Layer-tap: hold = momentary layer, tap = keycode.
    LayerTap {
        /// Layer activated while held.
        layer: u8,
        /// Keycode emitted on tap.
        tap: KeyCode,
    },
    /// Start recording a dynamic macro into the given slot.
    MacroRecord(u8),
    /// Play back the dynamic macro stored in the given slot.
    MacroPlay(u8),
    /// Stop recording the current dynamic macro.
    MacroStop,
    /// Emit a MIDI event (note on/off, CC).
    Midi(MidiAction),
    /// Emit a modifier + keycode together (e.g. Shift+Grave for tilde).
    ModifiedKey {
        /// Modifier to apply.
        modifier: ModifierKey,
        /// Key to press.
        key: KeyCode,
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
    /// Emit a HID mouse action (movement, click, or scroll).
    Mouse(MouseAction),
    /// Do nothing; blocks transparent fallthrough (QMK `KC_NO`).
    NoOp,
    /// One-shot layer: active for the next keypress only.
    OneShotLayer(u8),
    /// One-shot modifier: applies once to the next keypress, then clears.
    OneShotMod(ModifierKey),
    /// Switch between USB and BLE output.
    OutputSelect(crate::pipeline::hid::OutputMode),
    /// Emit the last non-repeat keycode again.
    RepeatKey,
    /// Emit an RGB control command (toggle, cycle animation, brightness).
    Rgb(crate::rgb::RgbCommand),
    /// Tap-toggle: tap toggles layer on/off, hold is momentary.
    TapToggle(u8),
    /// Emit a static sequence of keycodes as if typed.
    SendString(&'static [KeyCode]),
    /// Toggle slow keys on/off (require key held for threshold before accepting).
    SlowKeysToggle,
    /// Enter deep sleep (board crate handles via PowerManager).
    Sleep,
    /// Toggle sticky keys on/off (modifiers apply to next keypress then clear).
    StickyKeysToggle,
    /// Contribute to a steno chord with the given bit index (0–63).
    Steno(u8),
    /// Fall through to the next active layer (QMK `KC_TRNS`).
    Transparent,
}

impl<Ext: Copy + PartialEq> Action<Ext> {
    /// Returns `true` if this action requires tap-vs-hold disambiguation.
    #[inline]
    #[must_use]
    pub const fn needs_tap_hold(&self) -> bool {
        matches!(
            self,
            Self::ModTap { .. } | Self::LayerTap { .. } | Self::TapToggle(_)
        )
    }
}

/// Remaps a key when specific modifiers are held.
///
/// When the processor sees `Action::Key(trigger)` and the active modifier
/// state contains `required_mods`, the action is replaced with `replacement`
/// and the bits in `suppress_mods` are temporarily cleared from the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyOverride<Ext = ()>
where
    Ext: Copy + PartialEq,
{
    /// The keycode that triggers the override.
    pub trigger: KeyCode,
    /// Modifiers that must be held for the override to activate.
    pub required_mods: ModifierKey,
    /// Modifiers to remove from the report when the override fires.
    pub suppress_mods: ModifierKey,
    /// The replacement action to execute instead.
    pub replacement: Action<Ext>,
}

/// Defines an automatic tri-layer activation rule.
///
/// When both `layer1` and `layer2` are active, `target` is automatically
/// activated. When either is deactivated, `target` is deactivated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriLayerDef {
    /// First required layer.
    pub layer1: u8,
    /// Second required layer.
    pub layer2: u8,
    /// Layer activated when both are held.
    pub target: u8,
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
    fn tap_toggle_needs_tap_hold() {
        let a = Action::<()>::TapToggle(1);
        assert!(a.needs_tap_hold());
    }

    #[test]
    fn all_other_actions_do_not_need_tap_hold() {
        let actions: &[Action<()>] = &[
            Action::AutocompleteAccept,
            Action::AutocompleteToggle,
            Action::AutocorrectToggle,
            Action::Transparent,
            Action::NoOp,
            Action::Key(KeyCode::A),
            Action::Consumer(ConsumerCode::VolumeUp),
            Action::Modifier(ModifierKey::LSHIFT),
            Action::Layer(LayerOp::Momentary(1)),
            Action::OneShotMod(ModifierKey::LCTRL),
            Action::OneShotLayer(2),
            Action::Bootmagic,
            Action::CapsWord,
            Action::LayerLock(1),
            Action::MacroRecord(0),
            Action::MacroPlay(0),
            Action::MacroStop,
            Action::Midi(MidiAction::NoteOn {
                channel: 0,
                note: 60,
                velocity: 127,
            }),
            Action::Mouse(MouseAction::Click(1)),
            Action::Gamepad(GamepadAction::ButtonPress(0)),
            Action::LayerMod {
                layer: 1,
                modifier: ModifierKey::LSHIFT,
            },
            Action::ModifiedKey {
                modifier: ModifierKey::LSHIFT,
                key: KeyCode::Grave,
            },
            Action::RepeatKey,
            Action::Rgb(crate::rgb::RgbCommand::Toggle),
            Action::SendString(&[KeyCode::A]),
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
