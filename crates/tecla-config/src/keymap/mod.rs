//! QMK `keymap.c` transpiler — converts QMK keycodes to tecla `layer!` syntax.

mod includes;
mod parser;
mod transpiler;

pub use includes::QmkDefines;
pub use parser::{
    parse_keycode, parse_keymap_c, parse_keymap_c_with_includes, parse_keymap_c_with_shared,
    parse_layer_body,
};
pub use transpiler::{keycode_to_tecla, transpile, validate_keycodes};

/// A parsed layer from a keymap.c file.
#[derive(Debug, Clone)]
pub struct ParsedLayer {
    /// Layer index.
    pub index: usize,
    /// Layer name (e.g. "_BASE", "_LOWER").
    pub name: String,
    /// Parsed keycodes in row-major order.
    pub keys: Vec<ResolvedKeycode>,
}

/// Errors from keymap parsing or transpilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeymapError {
    /// No layers found in the input.
    NoLayers,
    /// Layer key counts are inconsistent.
    InconsistentKeyCount {
        /// Layer name.
        layer: String,
        /// Expected key count (from the first layer).
        expected: usize,
        /// Actual key count.
        got: usize,
    },
    /// An unrecognized keycode was found.
    UnrecognizedKeycode(String),
    /// Tree-sitter failed to parse the C source.
    ParseFailure(String),
}

impl std::fmt::Display for KeymapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLayers => write!(f, "no layers found in keymap"),
            Self::InconsistentKeyCount {
                layer,
                expected,
                got,
            } => {
                write!(f, "layer {layer} has {got} keys, expected {expected}")
            }
            Self::UnrecognizedKeycode(kc) => write!(f, "unrecognized keycode: {kc}"),
            Self::ParseFailure(msg) => write!(f, "failed to parse C source: {msg}"),
        }
    }
}

#[expect(clippy::missing_trait_methods, reason = "default impls are sufficient")]
impl std::error::Error for KeymapError {}

/// A fully resolved QMK keycode, classified by its numeric value into a typed variant.
///
/// Every QMK keycode is a 16-bit number. The high bits determine the category
/// (modifier combo, layer-tap, momentary layer, etc.) and the low bits carry
/// the payload (HID usage, layer number, modifier mask). This enum captures
/// that structure so downstream code never needs to string-match keycode names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedKeycode {
    /// `KC_NO` (0x0000) — no action.
    NoOp,
    /// `KC_TRANSPARENT` (0x0001) — falls through to the layer below.
    Transparent,
    /// HID keyboard usage ID (0x02–0xFF).
    HidKey(u8),
    /// HID consumer usage (reserved for future use).
    Consumer(u16),
    /// `QK_MODS` range — a key with baked-in modifiers.
    Modified {
        /// QMK 5-bit modifier mask (bit 0 = Ctrl, 1 = Shift, 2 = Alt, 3 = GUI, 4 = right-side).
        mods: u8,
        /// HID usage ID of the base key.
        key: u8,
    },
    /// `MO(layer)` — activate layer while held.
    Momentary(u8),
    /// `TG(layer)` — toggle layer on/off.
    Toggle(u8),
    /// `TO(layer)` — turn on layer, turn off all others.
    ToLayer(u8),
    /// `DF(layer)` — set the default layer.
    DefaultLayer(u8),
    /// `PDF(layer)` — set the persistent default layer.
    PersistentDefaultLayer(u8),
    /// `TT(layer)` — tap-toggle layer.
    TapToggle(u8),
    /// `OSL(layer)` — one-shot layer.
    OneShotLayer(u8),
    /// `OSM(mod)` — one-shot modifier.
    OneShotMod(u8),
    /// `LT(layer, kc)` — tap for key, hold for layer.
    LayerTap {
        /// Target layer.
        layer: u8,
        /// HID usage ID sent on tap.
        key: u8,
    },
    /// `MT(mod, kc)` — tap for key, hold for modifier.
    ModTap {
        /// QMK 5-bit modifier mask.
        mods: u8,
        /// HID usage ID sent on tap.
        key: u8,
    },
    /// `LM(layer, mod)` — activate layer with modifier.
    LayerMod {
        /// Target layer.
        layer: u8,
        /// QMK 5-bit modifier mask.
        mods: u8,
    },
    /// `TD(n)` — tap-dance index.
    TapDance(u8),
    /// `SH_T(kc)` / swap-hands index.
    SwapHands(u8),
    /// MIDI keycode index.
    Midi(u8),
    /// Stenography keycode index.
    Steno(u8),
    /// Macro index.
    Macro(u8),
    /// Unicode codepoint (0x8000–0xFFFF range).
    Unicode(u16),
    /// Magic/bootmagic keycode index.
    Magic(u8),
    /// Audio keycode index.
    Audio(u8),
    /// Joystick button index.
    Joystick(u8),
    /// Programmable button index.
    ProgrammableButton(u8),
    /// RGB/backlight keycode index (QK_LIGHTING range).
    Lighting(u8),
    /// System/quantum keycode index (QK_QUANTUM range: boot, NKRO, debug, etc.).
    Quantum(u16),
    /// Keyboard-specific keycode index (QK_KB range).
    Kb(u8),
    /// User-defined keycode index.
    User(u16),
    /// Could not classify this value.
    Unresolved {
        /// The raw value as a hex string.
        raw: String,
        /// Why classification failed.
        reason: String,
    },
}

/// Classifies a raw 16-bit QMK keycode into a typed [`ResolvedKeycode`].
///
/// QMK keycodes are 16-bit values where the high bits encode the category
/// (basic HID key, modifier combo, layer-tap, momentary layer, etc.) and the
/// low bits carry the payload (HID usage ID, layer number, modifier mask).
/// This function maps every defined range from QMK's `quantum/keycodes.h`
/// (`enum qk_keycode_ranges`) to the corresponding [`ResolvedKeycode`] variant.
/// Values that fall in gaps between defined ranges produce
/// [`ResolvedKeycode::Unresolved`].
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "match arms guarantee values are in range for u8"
)]
#[expect(
    clippy::as_conversions,
    reason = "extracting byte fields from u16 QMK keycode"
)]
pub fn classify(value: u16) -> ResolvedKeycode {
    match value {
        0x0000 => ResolvedKeycode::NoOp,
        0x0001 => ResolvedKeycode::Transparent,
        0x0002..=0x00FF => ResolvedKeycode::HidKey(value as u8),
        0x0100..=0x1FFF => ResolvedKeycode::Modified {
            mods: ((value >> 8) & 0x1F) as u8,
            key: (value & 0xFF) as u8,
        },
        0x2000..=0x3FFF => ResolvedKeycode::ModTap {
            mods: ((value >> 8) & 0x1F) as u8,
            key: (value & 0xFF) as u8,
        },
        0x4000..=0x4FFF => ResolvedKeycode::LayerTap {
            layer: ((value >> 8) & 0x0F) as u8,
            key: (value & 0xFF) as u8,
        },
        0x5000..=0x51FF => ResolvedKeycode::LayerMod {
            layer: ((value >> 4) & 0x0F) as u8,
            mods: (value & 0x1F) as u8,
        },
        0x5200..=0x521F => ResolvedKeycode::ToLayer((value & 0x1F) as u8),
        0x5220..=0x523F => ResolvedKeycode::Momentary((value & 0x1F) as u8),
        0x5240..=0x525F => ResolvedKeycode::DefaultLayer((value & 0x1F) as u8),
        0x5260..=0x527F => ResolvedKeycode::Toggle((value & 0x1F) as u8),
        0x5280..=0x529F => ResolvedKeycode::OneShotLayer((value & 0x1F) as u8),
        0x52A0..=0x52BF => ResolvedKeycode::OneShotMod((value & 0x1F) as u8),
        0x52C0..=0x52DF => ResolvedKeycode::TapToggle((value & 0x1F) as u8),
        0x52E0..=0x52FF => ResolvedKeycode::PersistentDefaultLayer((value & 0x1F) as u8),
        0x5600..=0x56FF => ResolvedKeycode::SwapHands((value & 0xFF) as u8),
        0x5700..=0x57FF => ResolvedKeycode::TapDance((value & 0xFF) as u8),
        0x7000..=0x70FF => ResolvedKeycode::Magic((value & 0xFF) as u8),
        0x7100..=0x71FF => ResolvedKeycode::Midi((value & 0xFF) as u8),
        0x7400..=0x743F => ResolvedKeycode::Joystick((value & 0x3F) as u8),
        0x7440..=0x747F => ResolvedKeycode::ProgrammableButton((value & 0x3F) as u8),
        0x7480..=0x74BF => ResolvedKeycode::Audio((value & 0x3F) as u8),
        0x74C0..=0x74FF => ResolvedKeycode::Steno((value & 0x3F) as u8),
        0x7700..=0x777F => ResolvedKeycode::Macro((value & 0x7F) as u8),
        0x7800..=0x78FF => ResolvedKeycode::Lighting((value & 0xFF) as u8),
        0x7C00..=0x7DFF => ResolvedKeycode::Quantum(value - 0x7C00),
        0x7E00..=0x7E3F => ResolvedKeycode::Kb((value - 0x7E00) as u8),
        0x7E40..=0x7FFF => ResolvedKeycode::User(value - 0x7E40),
        0x8000..=0xFFFF => ResolvedKeycode::Unicode(value),
        _ => ResolvedKeycode::Unresolved {
            raw: format!("0x{value:04X}"),
            reason: "unknown range".into(),
        },
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn classify_no_op() {
        assert_eq!(classify(0x0000), ResolvedKeycode::NoOp);
    }

    #[test]
    fn classify_transparent() {
        assert_eq!(classify(0x0001), ResolvedKeycode::Transparent);
    }

    #[test]
    fn classify_hid_key_a() {
        assert_eq!(classify(0x0004), ResolvedKeycode::HidKey(4));
    }

    #[test]
    fn classify_modified_shift_a() {
        assert_eq!(
            classify(0x0204),
            ResolvedKeycode::Modified { mods: 2, key: 4 },
        );
    }

    #[test]
    fn classify_momentary_1() {
        assert_eq!(classify(0x5221), ResolvedKeycode::Momentary(1));
    }

    #[test]
    fn classify_toggle_1() {
        assert_eq!(classify(0x5261), ResolvedKeycode::Toggle(1));
    }

    #[test]
    fn classify_layer_tap() {
        assert_eq!(
            classify(0x4104),
            ResolvedKeycode::LayerTap { layer: 1, key: 4 },
        );
    }

    #[test]
    fn classify_mod_tap() {
        assert_eq!(
            classify(0x2204),
            ResolvedKeycode::ModTap { mods: 2, key: 4 },
        );
    }

    #[test]
    fn classify_unknown_range() {
        let result = classify(0x5300);
        assert_eq!(
            result,
            ResolvedKeycode::Unresolved {
                raw: "0x5300".into(),
                reason: "unknown range".into(),
            },
        );
    }

    #[test]
    fn classify_quantum_boundary_no_truncation() {
        assert_eq!(classify(0x7DFF), ResolvedKeycode::Quantum(0x01FF));
        assert_eq!(classify(0x7D00), ResolvedKeycode::Quantum(0x0100));
        assert_eq!(classify(0x7C00), ResolvedKeycode::Quantum(0));
    }

    #[test]
    fn classify_user_boundary_no_truncation() {
        assert_eq!(classify(0x7FFF), ResolvedKeycode::User(0x7FFF - 0x7E40));
        assert_eq!(classify(0x7F40), ResolvedKeycode::User(0x0100));
        assert_eq!(classify(0x7E40), ResolvedKeycode::User(0));
    }

    #[test]
    fn classify_hid_key_max() {
        assert_eq!(classify(0x00FF), ResolvedKeycode::HidKey(0xFF));
    }

    #[test]
    fn classify_modified_max() {
        assert_eq!(
            classify(0x1FFF),
            ResolvedKeycode::Modified {
                mods: 0x1F,
                key: 0xFF
            },
        );
    }

    #[test]
    fn classify_mod_tap_max() {
        assert_eq!(
            classify(0x3FFF),
            ResolvedKeycode::ModTap {
                mods: 0x1F,
                key: 0xFF
            },
        );
    }

    #[test]
    fn classify_via_qmk_defines() {
        let defines = QmkDefines::from_quantum_dir(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/quantum"),
        );

        let mo_base = defines.resolve_value("QK_MOMENTARY").unwrap();
        let mo_1 = mo_base + 1;
        assert_eq!(classify(mo_1), ResolvedKeycode::Momentary(1));

        let lt_base = defines.resolve_value("QK_LAYER_TAP").unwrap();
        let lt_1_a = lt_base | (1 << 8) | 4;
        assert_eq!(
            classify(lt_1_a),
            ResolvedKeycode::LayerTap { layer: 1, key: 4 },
        );
    }
}
