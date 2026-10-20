use std::fmt::Write as _;

use super::{KeymapError, ParsedLayer, ResolvedKeycode};

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match over all HID usage IDs"
)]
fn hid_key_name(id: u8) -> Option<&'static str> {
    match id {
        0x00 => Some("None"),
        0x04 => Some("A"),
        0x05 => Some("B"),
        0x06 => Some("C"),
        0x07 => Some("D"),
        0x08 => Some("E"),
        0x09 => Some("F"),
        0x0A => Some("G"),
        0x0B => Some("H"),
        0x0C => Some("I"),
        0x0D => Some("J"),
        0x0E => Some("K"),
        0x0F => Some("L"),
        0x10 => Some("M"),
        0x11 => Some("N"),
        0x12 => Some("O"),
        0x13 => Some("P"),
        0x14 => Some("Q"),
        0x15 => Some("R"),
        0x16 => Some("S"),
        0x17 => Some("T"),
        0x18 => Some("U"),
        0x19 => Some("V"),
        0x1A => Some("W"),
        0x1B => Some("X"),
        0x1C => Some("Y"),
        0x1D => Some("Z"),
        0x1E => Some("N1"),
        0x1F => Some("N2"),
        0x20 => Some("N3"),
        0x21 => Some("N4"),
        0x22 => Some("N5"),
        0x23 => Some("N6"),
        0x24 => Some("N7"),
        0x25 => Some("N8"),
        0x26 => Some("N9"),
        0x27 => Some("N0"),
        0x28 => Some("Enter"),
        0x29 => Some("Escape"),
        0x2A => Some("Backspace"),
        0x2B => Some("Tab"),
        0x2C => Some("Space"),
        0x2D => Some("Minus"),
        0x2E => Some("Equal"),
        0x2F => Some("LeftBrace"),
        0x30 => Some("RightBrace"),
        0x31 => Some("Backslash"),
        0x32 => Some("NonUsHash"),
        0x33 => Some("Semicolon"),
        0x34 => Some("Quote"),
        0x35 => Some("Grave"),
        0x36 => Some("Comma"),
        0x37 => Some("Dot"),
        0x38 => Some("Slash"),
        0x39 => Some("CapsLock"),
        0x3A => Some("F1"),
        0x3B => Some("F2"),
        0x3C => Some("F3"),
        0x3D => Some("F4"),
        0x3E => Some("F5"),
        0x3F => Some("F6"),
        0x40 => Some("F7"),
        0x41 => Some("F8"),
        0x42 => Some("F9"),
        0x43 => Some("F10"),
        0x44 => Some("F11"),
        0x45 => Some("F12"),
        0x46 => Some("PrintScreen"),
        0x47 => Some("ScrollLock"),
        0x48 => Some("Pause"),
        0x49 => Some("Insert"),
        0x4A => Some("Home"),
        0x4B => Some("PageUp"),
        0x4C => Some("Delete"),
        0x4D => Some("End"),
        0x4E => Some("PageDown"),
        0x4F => Some("Right"),
        0x50 => Some("Left"),
        0x51 => Some("Down"),
        0x52 => Some("Up"),
        0x53 => Some("NumLock"),
        0x54 => Some("KpSlash"),
        0x55 => Some("KpAster"),
        0x56 => Some("KpMinus"),
        0x57 => Some("KpPlus"),
        0x58 => Some("KpEnter"),
        0x59 => Some("Kp1"),
        0x5A => Some("Kp2"),
        0x5B => Some("Kp3"),
        0x5C => Some("Kp4"),
        0x5D => Some("Kp5"),
        0x5E => Some("Kp6"),
        0x5F => Some("Kp7"),
        0x60 => Some("Kp8"),
        0x61 => Some("Kp9"),
        0x62 => Some("Kp0"),
        0x63 => Some("KpDot"),
        0x64 => Some("NonUsBackslash"),
        0x65 => Some("Application"),
        0x66 => Some("Power"),
        0x67 => Some("KpEqual"),
        0x68 => Some("F13"),
        0x69 => Some("F14"),
        0x6A => Some("F15"),
        0x6B => Some("F16"),
        0x6C => Some("F17"),
        0x6D => Some("F18"),
        0x6E => Some("F19"),
        0x6F => Some("F20"),
        0x70 => Some("F21"),
        0x71 => Some("F22"),
        0x72 => Some("F23"),
        0x73 => Some("F24"),
        0x74 => Some("Execute"),
        0x75 => Some("Help"),
        0x77 => Some("Select"),
        0x78 => Some("Stop"),
        0x79 => Some("Again"),
        0x7A => Some("Undo"),
        0x7B => Some("Cut"),
        0x7C => Some("Copy"),
        0x7D => Some("Paste"),
        0x7E => Some("Find"),
        0x85 => Some("KpComma"),
        0x87 => Some("International1"),
        0x88 => Some("International2"),
        0x89 => Some("International3"),
        0x8A => Some("International4"),
        0x8B => Some("International5"),
        0x90 => Some("Language1"),
        0x91 => Some("Language2"),
        0x92 => Some("Language3"),
        0x93 => Some("Language4"),
        0x94 => Some("Language5"),
        0xE0 => Some("LCtrl"),
        0xE1 => Some("LShift"),
        0xE2 => Some("LAlt"),
        0xE3 => Some("LGui"),
        0xE4 => Some("RCtrl"),
        0xE5 => Some("RShift"),
        0xE6 => Some("RAlt"),
        0xE7 => Some("RGui"),
        _ => None,
    }
}

fn key_name(id: u8) -> String {
    hid_key_name(id).map_or_else(|| format!("HID({id})"), |s| s.to_owned())
}

fn mods_to_name(mods: u8) -> String {
    const BITS: [(u8, &str); 4] = [(0x01, "CTL"), (0x02, "SFT"), (0x04, "ALT"), (0x08, "GUI")];
    let right = mods & 0x10 != 0;
    let prefix = if right { "R" } else { "L" };
    let mut parts = Vec::new();
    for (bit, name) in &BITS {
        if mods & bit != 0 {
            parts.push(format!("{prefix}{name}"));
        }
    }
    if parts.is_empty() {
        format!("MOD(0x{mods:02X})")
    } else {
        parts.join("+")
    }
}

/// Converts a [`ResolvedKeycode`] to a tecla `keymap!` macro token string.
///
/// Returns the identifier or macro invocation that `tecla_macros::keymap!`
/// expects — e.g. `"A"`, `"TRNS"`, `"LT!(1, Space)"`, `"MT!(LSHIFT, A)"`.
/// [`Unresolved`](ResolvedKeycode::Unresolved) variants pass through as their
/// raw hex string.
#[must_use]
pub fn keycode_to_tecla(kc: &ResolvedKeycode) -> String {
    match kc {
        ResolvedKeycode::NoOp => "NoOp".to_owned(),
        ResolvedKeycode::Transparent => "TRNS".to_owned(),
        ResolvedKeycode::HidKey(id) => key_name(*id),
        ResolvedKeycode::Consumer(n) => format!("CONSUMER({n})"),
        ResolvedKeycode::Modified { mods, key } => {
            format!("{}({})", mods_to_name(*mods), key_name(*key))
        }
        ResolvedKeycode::Momentary(n) => format!("MO({n})"),
        ResolvedKeycode::Toggle(n) => format!("TG({n})"),
        ResolvedKeycode::ToLayer(n) => format!("TO({n})"),
        ResolvedKeycode::DefaultLayer(n) => format!("DF({n})"),
        ResolvedKeycode::PersistentDefaultLayer(n) => format!("PDF({n})"),
        ResolvedKeycode::TapToggle(n) => format!("TT({n})"),
        ResolvedKeycode::OneShotLayer(n) => format!("OSL({n})"),
        ResolvedKeycode::OneShotMod(m) => format!("OSM({})", mods_to_name(*m)),
        ResolvedKeycode::LayerTap { layer, key } => {
            format!("LT!({layer}, {})", key_name(*key))
        }
        ResolvedKeycode::ModTap { mods, key } => {
            format!("MT!({}, {})", mods_to_name(*mods), key_name(*key))
        }
        ResolvedKeycode::LayerMod { layer, mods } => {
            format!("LM({layer}, {})", mods_to_name(*mods))
        }
        ResolvedKeycode::TapDance(n) => format!("TD({n})"),
        ResolvedKeycode::SwapHands(k) => format!("SH({k})"),
        ResolvedKeycode::Midi(n) => format!("MIDI({n})"),
        ResolvedKeycode::Steno(n) => format!("STENO({n})"),
        ResolvedKeycode::Macro(n) => format!("MACRO({n})"),
        ResolvedKeycode::Unicode(cp) => format!("UC(0x{cp:04X})"),
        ResolvedKeycode::Magic(n) => format!("MAGIC({n})"),
        ResolvedKeycode::Audio(n) => format!("AUDIO({n})"),
        ResolvedKeycode::Joystick(n) => format!("JS({n})"),
        ResolvedKeycode::ProgrammableButton(n) => format!("PB({n})"),
        ResolvedKeycode::Lighting(n) => format!("LIGHT({n})"),
        ResolvedKeycode::Quantum(n) => format!("QK({n})"),
        ResolvedKeycode::Kb(n) => format!("KB({n})"),
        ResolvedKeycode::User(n) => format!("USER({n})"),
        ResolvedKeycode::Unresolved { raw, .. } => raw.clone(),
    }
}

/// Transpiles parsed layers into a complete tecla `keymap!` macro invocation.
///
/// Takes the output of [`parse_keymap_c`](super::parse_keymap_c) (or its
/// variants) and produces a Rust source string ready to paste into a board
/// crate. Each [`ParsedLayer`] becomes a `layer!(Name, ...)` entry inside the
/// `keymap!` block.
///
/// If `cols` is `Some(n)`, keys are column-aligned and wrapped every `n` keys
/// to mirror the physical keyboard rows. Otherwise all keys appear on one line.
///
/// Returns [`KeymapError::NoLayers`] if the slice is empty, or
/// [`KeymapError::InconsistentKeyCount`] if layers differ in key count.
pub fn transpile(layers: &[ParsedLayer], cols: Option<usize>) -> Result<String, KeymapError> {
    if layers.is_empty() {
        return Err(KeymapError::NoLayers);
    }

    let Some(first_layer) = layers.first() else {
        return Err(KeymapError::NoLayers);
    };
    let expected = first_layer.keys.len();
    for layer in layers.get(1..).unwrap_or_default() {
        if layer.keys.len() != expected {
            return Err(KeymapError::InconsistentKeyCount {
                layer: layer.name.clone(),
                expected,
                got: layer.keys.len(),
            });
        }
    }

    let c = cols.unwrap_or(0);

    let col_widths = if c > 0 {
        compute_col_widths(layers, c)
    } else {
        Vec::new()
    };

    let rows = expected.checked_div(c).unwrap_or(0);

    let mut out = String::new();
    if c > 0 {
        writeln!(
            out,
            "// {layer_count} layer(s), {expected} keys ({rows} rows x {c} cols)",
            layer_count = layers.len(),
        )
        .map_err(|e| KeymapError::ParseFailure(e.to_string()))?;
    }
    out.push_str("keymap! {\n");
    for layer in layers {
        // No `#[rustfmt::skip]` here. `keymap!` parses a bare `layer` ident
        // first and rejects any attribute, so emitting one produced output that
        // could not compile — and every golden snapshot recorded it. It bought
        // nothing anyway: transpiled keymaps are `include!`d, and rustfmt does
        // not format files it cannot reach through a module tree.
        out.push_str("    layer!(");
        out.push_str(&layer.name);
        out.push(',');
        if c > 0 {
            #[expect(
                clippy::integer_division_remainder_used,
                reason = "grid layout arithmetic requires remainder"
            )]
            for (i, kc) in layer.keys.iter().enumerate() {
                let col = i % c;
                if col == 0 {
                    out.push_str("\n        ");
                } else {
                    out.push(' ');
                }
                let token = keycode_to_tecla(kc);
                let width = col_widths.get(col).copied().unwrap_or(0);
                if col < c - 1 {
                    write!(out, "{token:width$},")
                        .map_err(|e| KeymapError::ParseFailure(e.to_string()))?;
                } else {
                    write!(out, "{token},")
                        .map_err(|e| KeymapError::ParseFailure(e.to_string()))?;
                }
            }
            out.push('\n');
            out.push_str("    ),\n");
        } else {
            for kc in &layer.keys {
                out.push(' ');
                out.push_str(&keycode_to_tecla(kc));
                out.push(',');
            }
            out.push_str("),\n");
        }
    }
    out.push('}');

    Ok(out)
}

#[expect(
    clippy::integer_division_remainder_used,
    reason = "grid layout arithmetic requires remainder"
)]
fn compute_col_widths(layers: &[ParsedLayer], cols: usize) -> Vec<usize> {
    let mut widths = vec![0_usize; cols];
    for layer in layers {
        for (i, kc) in layer.keys.iter().enumerate() {
            let col = i % cols;
            let len = keycode_to_tecla(kc).len();
            if let Some(w) = widths.get_mut(col) {
                if len > *w {
                    *w = len;
                }
            }
        }
    }
    widths
}

/// Validates that all keycodes in the layers are recognized.
pub fn validate_keycodes(layers: &[ParsedLayer]) -> Result<(), KeymapError> {
    for layer in layers {
        for kc in &layer.keys {
            if let ResolvedKeycode::Unresolved { raw, .. } = kc {
                return Err(KeymapError::UnrecognizedKeycode(raw.clone()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn keycode_to_tecla_basic() {
        assert_eq!(keycode_to_tecla(&ResolvedKeycode::NoOp), "NoOp");
        assert_eq!(keycode_to_tecla(&ResolvedKeycode::Transparent), "TRNS");
        assert_eq!(keycode_to_tecla(&ResolvedKeycode::Momentary(1)), "MO(1)");
        assert_eq!(keycode_to_tecla(&ResolvedKeycode::Toggle(2)), "TG(2)");
    }

    #[test]
    fn keycode_to_tecla_hid_key() {
        assert_eq!(keycode_to_tecla(&ResolvedKeycode::HidKey(4)), "A");
    }

    #[test]
    fn keycode_to_tecla_modified() {
        assert_eq!(
            keycode_to_tecla(&ResolvedKeycode::Modified { mods: 2, key: 4 }),
            "LSFT(A)"
        );
    }

    #[test]
    fn keycode_to_tecla_layer_tap() {
        assert_eq!(
            keycode_to_tecla(&ResolvedKeycode::LayerTap {
                layer: 1,
                key: 0x2C
            }),
            "LT!(1, Space)"
        );
    }

    #[test]
    fn keycode_to_tecla_mod_tap() {
        assert_eq!(
            keycode_to_tecla(&ResolvedKeycode::ModTap { mods: 1, key: 4 }),
            "MT!(LCTL, A)"
        );
    }

    #[test]
    fn keycode_to_tecla_one_shot_mod() {
        assert_eq!(
            keycode_to_tecla(&ResolvedKeycode::OneShotMod(2)),
            "OSM(LSFT)"
        );
    }

    #[test]
    fn keycode_to_tecla_combo_mods() {
        assert_eq!(
            keycode_to_tecla(&ResolvedKeycode::Modified { mods: 0x06, key: 4 }),
            "LSFT+LALT(A)"
        );
    }

    #[test]
    fn keycode_to_tecla_right_mods() {
        assert_eq!(
            keycode_to_tecla(&ResolvedKeycode::Modified { mods: 0x11, key: 4 }),
            "RCTL(A)"
        );
    }

    #[test]
    fn keycode_to_tecla_unknown_hid() {
        assert_eq!(keycode_to_tecla(&ResolvedKeycode::HidKey(0x03)), "HID(3)");
    }

    #[test]
    fn keycode_to_tecla_unresolved() {
        assert_eq!(
            keycode_to_tecla(&ResolvedKeycode::Unresolved {
                raw: "ALIEN".into(),
                reason: "test".into()
            }),
            "ALIEN"
        );
    }

    #[test]
    fn transpile_produces_keymap() {
        let layers = vec![
            ParsedLayer {
                index: 0,
                name: "Base".into(),
                keys: vec![ResolvedKeycode::HidKey(4), ResolvedKeycode::HidKey(5)],
            },
            ParsedLayer {
                index: 1,
                name: "Fn".into(),
                keys: vec![ResolvedKeycode::HidKey(0x3A), ResolvedKeycode::Transparent],
            },
        ];
        let result = transpile(&layers, None).unwrap();
        assert!(result.contains("keymap!"));
        assert!(result.contains("TRNS"));
        assert!(result.contains("A,"));
        assert!(result.contains("F1,"));
    }

    #[test]
    fn transpile_no_layers_error() {
        let err = transpile(&[], None).unwrap_err();
        assert!(matches!(err, KeymapError::NoLayers));
        assert_eq!(err.to_string(), "no layers found in keymap");
    }

    #[test]
    fn transpile_inconsistent_key_count_error() {
        let layers = vec![
            ParsedLayer {
                index: 0,
                name: "A".into(),
                keys: vec![ResolvedKeycode::NoOp],
            },
            ParsedLayer {
                index: 1,
                name: "B".into(),
                keys: vec![ResolvedKeycode::NoOp, ResolvedKeycode::NoOp],
            },
        ];
        let err = transpile(&layers, None).unwrap_err();
        assert!(matches!(err, KeymapError::InconsistentKeyCount { .. }));
        assert!(err.to_string().contains("2 keys, expected 1"));
    }

    #[test]
    fn validate_keycodes_detects_unknown() {
        let layers = vec![ParsedLayer {
            index: 0,
            name: "Base".into(),
            keys: vec![ResolvedKeycode::Unresolved {
                raw: "ALIEN".into(),
                reason: "test".into(),
            }],
        }];
        let err = validate_keycodes(&layers).unwrap_err();
        assert!(matches!(err, KeymapError::UnrecognizedKeycode(_)));
        assert!(err.to_string().contains("ALIEN"));
    }

    #[test]
    fn validate_keycodes_passes_for_known() {
        let layers = vec![ParsedLayer {
            index: 0,
            name: "Base".into(),
            keys: vec![ResolvedKeycode::HidKey(4), ResolvedKeycode::Momentary(1)],
        }];
        validate_keycodes(&layers).unwrap();
    }

    #[test]
    fn mods_to_name_single() {
        assert_eq!(mods_to_name(0x01), "LCTL");
        assert_eq!(mods_to_name(0x02), "LSFT");
        assert_eq!(mods_to_name(0x04), "LALT");
        assert_eq!(mods_to_name(0x08), "LGUI");
        assert_eq!(mods_to_name(0x11), "RCTL");
        assert_eq!(mods_to_name(0x12), "RSFT");
        assert_eq!(mods_to_name(0x14), "RALT");
        assert_eq!(mods_to_name(0x18), "RGUI");
    }

    #[test]
    fn mods_to_name_combo() {
        assert_eq!(mods_to_name(0x03), "LCTL+LSFT");
        assert_eq!(mods_to_name(0x0F), "LCTL+LSFT+LALT+LGUI");
    }
}
