//! End-to-end test: parse a real Planck QMK config and transpile to tecla keymap.
#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![expect(clippy::unwrap_used, reason = "tests use unwrap for brevity")]
#![expect(
    clippy::indexing_slicing,
    reason = "tests index into known-length slices"
)]
#![expect(clippy::panic, reason = "tests use panic for assertion failures")]

use std::path::Path;
use tecla_config::info_json;
use tecla_config::keymap;

fn quantum_defines() -> keymap::QmkDefines {
    let kb = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/quantum");
    if kb.exists() {
        keymap::QmkDefines::from_quantum_dir(&kb)
    } else {
        keymap::QmkDefines::default()
    }
}

const PLANCK_INFO_JSON: &str = r#"{
    "keyboard_name": "planck/rev6",
    "manufacturer": "OLKB",
    "usb": { "vid": "0x03A8", "pid": "0xA4F9" },
    "matrix_pins": {
        "rows": ["B0", "B1", "B2", "B3"],
        "cols": ["A0", "A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "A11"]
    },
    "matrix_size": { "rows": 4, "cols": 12 },
    "layouts": {
        "LAYOUT_planck_grid": {
            "layout": [
                {"matrix": [0, 0], "x": 0, "y": 0},
                {"matrix": [0, 1], "x": 1, "y": 0},
                {"matrix": [0, 2], "x": 2, "y": 0},
                {"matrix": [0, 3], "x": 3, "y": 0},
                {"matrix": [0, 4], "x": 4, "y": 0},
                {"matrix": [0, 5], "x": 5, "y": 0},
                {"matrix": [0, 6], "x": 6, "y": 0},
                {"matrix": [0, 7], "x": 7, "y": 0},
                {"matrix": [0, 8], "x": 8, "y": 0},
                {"matrix": [0, 9], "x": 9, "y": 0},
                {"matrix": [0, 10], "x": 10, "y": 0},
                {"matrix": [0, 11], "x": 11, "y": 0},
                {"matrix": [1, 0], "x": 0, "y": 1},
                {"matrix": [1, 1], "x": 1, "y": 1},
                {"matrix": [1, 2], "x": 2, "y": 1},
                {"matrix": [1, 3], "x": 3, "y": 1},
                {"matrix": [1, 4], "x": 4, "y": 1},
                {"matrix": [1, 5], "x": 5, "y": 1},
                {"matrix": [1, 6], "x": 6, "y": 1},
                {"matrix": [1, 7], "x": 7, "y": 1},
                {"matrix": [1, 8], "x": 8, "y": 1},
                {"matrix": [1, 9], "x": 9, "y": 1},
                {"matrix": [1, 10], "x": 10, "y": 1},
                {"matrix": [1, 11], "x": 11, "y": 1},
                {"matrix": [2, 0], "x": 0, "y": 2},
                {"matrix": [2, 1], "x": 1, "y": 2},
                {"matrix": [2, 2], "x": 2, "y": 2},
                {"matrix": [2, 3], "x": 3, "y": 2},
                {"matrix": [2, 4], "x": 4, "y": 2},
                {"matrix": [2, 5], "x": 5, "y": 2},
                {"matrix": [2, 6], "x": 6, "y": 2},
                {"matrix": [2, 7], "x": 7, "y": 2},
                {"matrix": [2, 8], "x": 8, "y": 2},
                {"matrix": [2, 9], "x": 9, "y": 2},
                {"matrix": [2, 10], "x": 10, "y": 2},
                {"matrix": [2, 11], "x": 11, "y": 2},
                {"matrix": [3, 0], "x": 0, "y": 3},
                {"matrix": [3, 1], "x": 1, "y": 3},
                {"matrix": [3, 2], "x": 2, "y": 3},
                {"matrix": [3, 3], "x": 3, "y": 3},
                {"matrix": [3, 4], "x": 4, "y": 3},
                {"matrix": [3, 5], "x": 5, "y": 3},
                {"matrix": [3, 6], "x": 6, "y": 3},
                {"matrix": [3, 7], "x": 7, "y": 3},
                {"matrix": [3, 8], "x": 8, "y": 3},
                {"matrix": [3, 9], "x": 9, "y": 3},
                {"matrix": [3, 10], "x": 10, "y": 3},
                {"matrix": [3, 11], "x": 11, "y": 3}
            ]
        }
    }
}"#;

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

#[test]
fn end_to_end_planck_config_to_tecla_keymap() {
    let config = info_json::parse(PLANCK_INFO_JSON).unwrap();

    assert_eq!(config.keyboard_name.as_deref(), Some("planck/rev6"));
    assert_eq!(config.manufacturer.as_deref(), Some("OLKB"));
    assert_eq!(config.usb.vid, 0x03A8);
    assert_eq!(config.usb.pid, 0xA4F9);
    assert_eq!(config.matrix_size.rows, 4);
    assert_eq!(config.matrix_size.cols, 12);
    assert_eq!(config.matrix_pins.rows.len(), 4);
    assert_eq!(config.matrix_pins.cols.len(), 12);
    assert!(config
        .layouts
        .get("LAYOUT_planck_grid")
        .is_some_and(|l| l.layout.len() == 48));

    let shared = quantum_defines();
    let layers = keymap::parse_keymap_c_with_shared(PLANCK_KEYMAP_C, &shared).unwrap();

    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].keys.len(), 48, "base layer key count");
    assert_eq!(layers[1].keys.len(), 48, "fn layer key count");

    let output = keymap::transpile(&layers, Some(12)).unwrap();

    assert!(output.contains("keymap!"));
    assert!(output.contains("LT!(1, Space)"));
    assert!(output.contains("Tab"));
    assert!(output.contains("Backspace"));
    assert_eq!(output.matches("layer!(").count(), 2);
}

#[test]
fn end_to_end_parse_keymap_c_full_source() {
    let shared = quantum_defines();
    let layers = keymap::parse_keymap_c_with_shared(PLANCK_KEYMAP_C, &shared).unwrap();
    assert_eq!(layers.len(), 2);
    let [base, fun] = layers.as_slice() else {
        panic!("expected 2 layers")
    };
    assert_eq!(base.name, "_BASE");
    assert_eq!(fun.name, "_FN");
    assert_eq!(base.keys.len(), 48);
    assert_eq!(fun.keys.len(), 48);

    keymap::validate_keycodes(&layers).unwrap();

    let output = keymap::transpile(&layers, Some(12)).unwrap();
    assert!(output.contains("LT!(1, Space)"));
    assert!(output.contains("Grave"));
    assert!(output.contains("Delete"));
    assert_eq!(output.matches("layer!(").count(), 2);
}
