#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![expect(clippy::unwrap_used, reason = "tests use unwrap for brevity")]
#![expect(clippy::string_slice, reason = "slicing known-ASCII test strings")]
#![allow(missing_docs, reason = "test module")]

use std::path::Path;
use tecla_config::qmk::keymap;
use tecla_config::qmk::keymap::ResolvedKeycode;

fn quantum_defines() -> keymap::QmkDefines {
    let kb = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/quantum");
    if kb.exists() {
        keymap::QmkDefines::from_quantum_dir(&kb)
    } else {
        keymap::QmkDefines::default()
    }
}

const PLANCK_KEYMAP_C: &str = "
#include QMK_KEYBOARD_H

enum layers { _BASE = 0, _FN };

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [_BASE] = LAYOUT_planck_grid(
        KC_TAB,  KC_Q,    KC_W,    KC_E,    KC_R,    KC_T,    KC_Y,    KC_U,    KC_I,    KC_O,    KC_P,    KC_BSPC,
        KC_ESC,  KC_A,    KC_S,    KC_D,    KC_F,    KC_G,    KC_H,    KC_J,    KC_K,    KC_L,    KC_SCLN, KC_ENT,
        KC_LSFT, KC_Z,    KC_X,    KC_C,    KC_V,    KC_B,    KC_N,    KC_M,    KC_COMM, KC_DOT,  KC_SLSH, KC_RSFT,
        KC_LCTL, KC_LGUI, KC_LALT, KC_LSFT, LT(1, KC_SPC), KC_SPC, KC_SPC, KC_RSFT, KC_LEFT, KC_DOWN, KC_UP, KC_RGHT
    ),
    [_FN] = LAYOUT_planck_grid(
        KC_GRV,  KC_1,    KC_2,    KC_3,    KC_4,    KC_5,    KC_6,    KC_7,    KC_8,    KC_9,    KC_0,    KC_DEL,
        KC_TRNS, KC_F1,   KC_F2,   KC_F3,   KC_F4,   KC_F5,   KC_F6,   KC_MINS, KC_EQL,  KC_LBRC, KC_RBRC, KC_BSLS,
        KC_TRNS, KC_F7,   KC_F8,   KC_F9,   KC_F10,  KC_F11,  KC_F12,  KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS,
        KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS, KC_TRNS
    ),
};
";

const VALID_BARE_TOKENS: &[&str] = &[
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "N0",
    "N1",
    "N2",
    "N3",
    "N4",
    "N5",
    "N6",
    "N7",
    "N8",
    "N9",
    "Enter",
    "Escape",
    "Backspace",
    "Tab",
    "Space",
    "Minus",
    "Equal",
    "LeftBrace",
    "RightBrace",
    "Backslash",
    "NonUsHash",
    "Semicolon",
    "Quote",
    "Grave",
    "Comma",
    "Dot",
    "Slash",
    "CapsLock",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "PrintScreen",
    "ScrollLock",
    "Pause",
    "Insert",
    "Home",
    "PageUp",
    "Delete",
    "End",
    "PageDown",
    "Right",
    "Left",
    "Down",
    "Up",
    "NumLock",
    "KpSlash",
    "KpAster",
    "KpMinus",
    "KpPlus",
    "KpEnter",
    "Kp0",
    "Kp1",
    "Kp2",
    "Kp3",
    "Kp4",
    "Kp5",
    "Kp6",
    "Kp7",
    "Kp8",
    "Kp9",
    "KpDot",
    "NonUsBackslash",
    "Application",
    "Power",
    "KpEqual",
    "F13",
    "F14",
    "F15",
    "F16",
    "F17",
    "F18",
    "F19",
    "F20",
    "F21",
    "F22",
    "F23",
    "F24",
    "Execute",
    "Help",
    "Select",
    "Stop",
    "Again",
    "Undo",
    "Cut",
    "Copy",
    "Paste",
    "Find",
    "KpComma",
    "International1",
    "International2",
    "International3",
    "International4",
    "International5",
    "Language1",
    "Language2",
    "Language3",
    "Language4",
    "Language5",
    "LCtrl",
    "LShift",
    "LAlt",
    "LGui",
    "RCtrl",
    "RShift",
    "RAlt",
    "RGui",
    "NoOp",
    "TRNS",
    "None",
];

fn is_valid_tecla_token(token: &str) -> bool {
    if VALID_BARE_TOKENS.contains(&token) {
        return true;
    }

    if token.starts_with("LT!(") && token.ends_with(')') {
        let inner = &token[4..token.len() - 1];
        let (layer, key) = inner.split_once(", ").unwrap_or(("", ""));
        return layer.parse::<u8>().is_ok() && VALID_BARE_TOKENS.contains(&key);
    }

    if token.starts_with("MT!(") && token.ends_with(')') {
        return true;
    }

    for prefix in &["MO(", "TG(", "TO(", "DF(", "TT(", "OSL(", "PDF("] {
        if token.starts_with(prefix) && token.ends_with(')') {
            let inner = &token[prefix.len()..token.len() - 1];
            if inner.parse::<u8>().is_ok() {
                return true;
            }
        }
    }

    if token.starts_with("OSM(") && token.ends_with(')') {
        return true;
    }

    if token.starts_with("LM(") && token.ends_with(')') {
        return true;
    }

    for prefix in &[
        "CONSUMER(",
        "TD(",
        "SH(",
        "MIDI(",
        "STENO(",
        "MACRO(",
        "UC(",
        "MAGIC(",
        "AUDIO(",
        "JS(",
        "PB(",
        "LIGHT(",
        "QK(",
        "KB(",
        "USER(",
        "HID(",
    ] {
        if token.starts_with(prefix) && token.ends_with(')') {
            return true;
        }
    }

    for mod_prefix in &[
        "LCTL(", "LSFT(", "LALT(", "LGUI(", "RCTL(", "RSFT(", "RALT(", "RGUI(",
    ] {
        if token.starts_with(mod_prefix) && token.ends_with(')') {
            return true;
        }
    }

    if token.contains('+') && token.contains('(') {
        return true;
    }

    false
}

#[test]
fn planck_transpile_every_token_is_valid() {
    let shared = quantum_defines();
    let layers = keymap::parse_keymap_c_with_shared(PLANCK_KEYMAP_C, &shared).unwrap();
    keymap::validate_keycodes(&layers).unwrap();

    for layer in &layers {
        for (i, kc) in layer.keys.iter().enumerate() {
            let token = keymap::keycode_to_tecla(kc);
            assert!(
                is_valid_tecla_token(&token),
                "layer '{}' key {i}: token '{token}' is not a recognized tecla keymap! macro token (from {kc:?})",
                layer.name,
            );
        }
    }
}

#[test]
fn planck_transpile_output_has_no_unresolved_keycodes() {
    let shared = quantum_defines();
    let layers = keymap::parse_keymap_c_with_shared(PLANCK_KEYMAP_C, &shared).unwrap();
    for layer in &layers {
        for (i, kc) in layer.keys.iter().enumerate() {
            assert!(
                !matches!(kc, ResolvedKeycode::Unresolved { .. }),
                "layer '{}' key {i}: unresolved keycode {kc:?}",
                layer.name,
            );
        }
    }
}

#[test]
fn transpiled_output_contains_nothing_the_keymap_macro_cannot_parse() {
    // `keymap!` parses a bare `layer` ident and rejects anything else, so an
    // attribute in the output is a hard compile error in the generated file.
    // The transpiler emitted `#[rustfmt::skip]` before every layer, which made
    // every transpiled keymap non-compiling — and the golden snapshots recorded
    // it as expected output, so nothing failed.
    let shared = quantum_defines();
    let layers = keymap::parse_keymap_c_with_shared(PLANCK_KEYMAP_C, &shared).unwrap();
    let output = keymap::transpile(&layers, Some(12)).unwrap();

    assert!(
        !output.contains('#'),
        "transpiled keymap contains an attribute or comment `keymap!` cannot parse:\n{output}"
    );
    for line in output.lines().map(str::trim) {
        assert!(
            !line.starts_with("#["),
            "attribute in transpiled output: {line}"
        );
    }
}

#[test]
fn every_resolved_keycode_variant_produces_valid_token() {
    let variants: Vec<ResolvedKeycode> = vec![
        ResolvedKeycode::NoOp,
        ResolvedKeycode::Transparent,
        ResolvedKeycode::HidKey(0x04),
        ResolvedKeycode::HidKey(0x2C),
        ResolvedKeycode::Consumer(0x00E9),
        ResolvedKeycode::Modified {
            mods: 0x02,
            key: 0x04,
        },
        ResolvedKeycode::Modified {
            mods: 0x11,
            key: 0x04,
        },
        ResolvedKeycode::Modified {
            mods: 0x03,
            key: 0x04,
        },
        ResolvedKeycode::Momentary(1),
        ResolvedKeycode::Toggle(2),
        ResolvedKeycode::ToLayer(0),
        ResolvedKeycode::DefaultLayer(1),
        ResolvedKeycode::PersistentDefaultLayer(0),
        ResolvedKeycode::TapToggle(1),
        ResolvedKeycode::OneShotLayer(2),
        ResolvedKeycode::OneShotMod(0x02),
        ResolvedKeycode::LayerTap {
            layer: 1,
            key: 0x2C,
        },
        ResolvedKeycode::ModTap {
            mods: 0x01,
            key: 0x04,
        },
        ResolvedKeycode::LayerMod {
            layer: 1,
            mods: 0x02,
        },
        ResolvedKeycode::TapDance(0),
        ResolvedKeycode::SwapHands(0),
        ResolvedKeycode::Midi(0),
        ResolvedKeycode::Steno(0),
        ResolvedKeycode::Macro(0),
        ResolvedKeycode::Unicode(0x8000),
        ResolvedKeycode::Magic(0),
        ResolvedKeycode::Audio(0),
        ResolvedKeycode::Joystick(0),
        ResolvedKeycode::ProgrammableButton(0),
        ResolvedKeycode::Lighting(0),
        ResolvedKeycode::Quantum(0),
        ResolvedKeycode::Kb(0),
        ResolvedKeycode::User(0),
    ];

    for kc in &variants {
        let token = keymap::keycode_to_tecla(kc);
        assert!(
            is_valid_tecla_token(&token),
            "ResolvedKeycode::{kc:?} produced invalid token: '{token}'"
        );
    }
}
