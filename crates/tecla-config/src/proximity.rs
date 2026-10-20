//! Key proximity table generation from physical keyboard layouts.
//!
//! [`generate_proximity_table`] computes a 26x26 Euclidean distance matrix
//! for letter keys (A-Z) based on their physical `(x, y)` coordinates in a
//! [`crate::board::Layout`]. The resulting table can be fed to the
//! autocorrect engine so that nearby-key typos score lower cost than
//! distant-key typos.
//!
//! [`keymap_from_qmk_names`] is a helper that converts a flat slice of QMK
//! key name strings (e.g. from a parsed `keymap.c`) into the
//! `(row, col, usage_id)` triples that [`generate_proximity_table`] expects.

use crate::board::Layout;

const HID_A: u8 = 0x04;
const LETTER_COUNT: usize = 26;

/// Builds a 26x26 proximity cost matrix from a physical keyboard layout.
///
/// Each entry `[i][j]` is the normalized Euclidean distance (0-255) between
/// letter keys `i` and `j` where `i = usage_id - 0x04`. Keys not present
/// in the keymap get a maximum cost of 255.
#[must_use]
#[expect(
    clippy::float_arithmetic,
    reason = "euclidean distance requires floating-point math"
)]
#[expect(
    clippy::as_conversions,
    reason = "f64-to-u8 conversion is guarded by round + clamp"
)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "value is clamped to 0..=255 before cast"
)]
#[expect(clippy::cast_sign_loss, reason = "value is non-negative after clamp")]
#[expect(
    clippy::indexing_slicing,
    reason = "indices are bounded by LETTER_COUNT constant"
)]
pub fn generate_proximity_table(
    layout: &Layout,
    keymap_layer0: &[(usize, usize, u8)],
) -> [[u8; LETTER_COUNT]; LETTER_COUNT] {
    let mut positions: [Option<(f64, f64)>; LETTER_COUNT] = [None; LETTER_COUNT];

    for key in &layout.keys {
        let row = key.row;
        let col = key.col;
        for &(kr, kc, usage_id) in keymap_layer0 {
            if kr == row && kc == col && is_letter(usage_id) {
                let idx = usize::from(usage_id - HID_A);
                positions[idx] = Some((key.x, key.y));
            }
        }
    }

    let mut max_dist: f64 = 0.0;
    let mut raw = [[0.0f64; LETTER_COUNT]; LETTER_COUNT];

    for (i, pos_i) in positions.iter().enumerate() {
        for (j, pos_j) in positions.iter().enumerate().skip(i + 1) {
            if let (Some((x1, y1)), Some((x2, y2))) = (*pos_i, *pos_j) {
                let d = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
                raw[i][j] = d;
                raw[j][i] = d;
                if d > max_dist {
                    max_dist = d;
                }
            }
        }
    }

    let mut table = [[255u8; LETTER_COUNT]; LETTER_COUNT];
    for (i, row) in table.iter_mut().enumerate() {
        row[i] = 0;
    }

    if max_dist > 0.0 {
        for (i, row) in raw.iter().enumerate() {
            for (j, &distance) in row.iter().enumerate().skip(i + 1) {
                if positions[i].is_some() && positions[j].is_some() {
                    let normalized = (distance / max_dist * 255.0).round().clamp(0.0, 255.0) as u8;
                    table[i][j] = normalized;
                    table[j][i] = normalized;
                }
            }
        }
    }

    table
}

#[expect(clippy::cast_possible_truncation, reason = "26 fits in u8")]
#[expect(clippy::as_conversions, reason = "const context requires as cast")]
const LETTER_COUNT_U8: u8 = LETTER_COUNT as u8;

const fn is_letter(usage_id: u8) -> bool {
    usage_id >= HID_A && usage_id < HID_A + LETTER_COUNT_U8
}

fn qmk_name_to_usage_id(name: &str) -> u8 {
    match name.trim() {
        "KC_A" => 0x04,
        "KC_B" => 0x05,
        "KC_C" => 0x06,
        "KC_D" => 0x07,
        "KC_E" => 0x08,
        "KC_F" => 0x09,
        "KC_G" => 0x0A,
        "KC_H" => 0x0B,
        "KC_I" => 0x0C,
        "KC_J" => 0x0D,
        "KC_K" => 0x0E,
        "KC_L" => 0x0F,
        "KC_M" => 0x10,
        "KC_N" => 0x11,
        "KC_O" => 0x12,
        "KC_P" => 0x13,
        "KC_Q" => 0x14,
        "KC_R" => 0x15,
        "KC_S" => 0x16,
        "KC_T" => 0x17,
        "KC_U" => 0x18,
        "KC_V" => 0x19,
        "KC_W" => 0x1A,
        "KC_X" => 0x1B,
        "KC_Y" => 0x1C,
        "KC_Z" => 0x1D,
        _ => 0x00,
    }
}

/// Converts a flat slice of QMK key names into `(row, col, usage_id)` triples.
///
/// Only letter keys (A-Z) are included. `cols` is the number of columns per row.
#[must_use]
#[expect(
    clippy::integer_division_remainder_used,
    reason = "grid layout arithmetic requires division"
)]
pub fn keymap_from_qmk_names(names: &[&str], cols: usize) -> Vec<(usize, usize, u8)> {
    let mut result = Vec::new();
    for (idx, name) in names.iter().enumerate() {
        let usage_id = qmk_name_to_usage_id(name);
        if is_letter(usage_id) {
            let row = idx / cols;
            let col = idx % cols;
            result.push((row, col, usage_id));
        }
    }
    result
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::as_conversions, reason = "test code")]
#[expect(clippy::panic, reason = "test code")]
mod tests {
    use super::*;
    use crate::board::{Layout, LayoutKey};

    #[test]
    fn simple_2x3_layout() {
        let layout = Layout {
            keys: vec![
                LayoutKey {
                    row: 0,
                    col: 0,
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                },
                LayoutKey {
                    row: 0,
                    col: 1,
                    x: 1.0,
                    y: 0.0,
                    w: 1.0,
                },
                LayoutKey {
                    row: 0,
                    col: 2,
                    x: 2.0,
                    y: 0.0,
                    w: 1.0,
                },
                LayoutKey {
                    row: 1,
                    col: 0,
                    x: 0.0,
                    y: 1.0,
                    w: 1.0,
                },
                LayoutKey {
                    row: 1,
                    col: 1,
                    x: 1.0,
                    y: 1.0,
                    w: 1.0,
                },
                LayoutKey {
                    row: 1,
                    col: 2,
                    x: 2.0,
                    y: 1.0,
                    w: 1.0,
                },
            ],
        };

        let keymap = vec![
            (0, 0, 0x04),
            (0, 1, 0x05),
            (0, 2, 0x06),
            (1, 0, 0x07),
            (1, 1, 0x08),
            (1, 2, 0x09),
        ];

        let table = generate_proximity_table(&layout, &keymap);

        assert_eq!(table[0][0], 0);
        assert_eq!(table[0][1], table[1][0]);

        let ab = table[0][1];
        let ac = table[0][2];
        assert!(ab < ac, "adjacent keys should have lower cost");

        let af = table[0][5];
        assert_eq!(af, 255);
    }

    #[test]
    fn missing_letters_get_max_cost() {
        let layout = Layout {
            keys: vec![
                LayoutKey {
                    row: 0,
                    col: 0,
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                },
                LayoutKey {
                    row: 0,
                    col: 1,
                    x: 1.0,
                    y: 0.0,
                    w: 1.0,
                },
            ],
        };

        let keymap = vec![(0, 0, 0x04), (0, 1, 0x05)];

        let table = generate_proximity_table(&layout, &keymap);

        assert_eq!(table[0][1], 255);
        assert_eq!(table[0][2], 255);
        assert_eq!(table[2][3], 255);
    }

    #[test]
    #[expect(
        clippy::integer_division_remainder_used,
        reason = "grid arithmetic in test"
    )]
    fn planck_proximity_snapshot() {
        use std::fmt::Write as _;

        let config = crate::qmk::info_json::parse(PLANCK_INFO_JSON).unwrap();
        let board = config.to_board_config();
        let Some(layout) = board.layouts.get("LAYOUT_planck_grid") else {
            panic!("missing layout")
        };

        let names: Vec<&str> = PLANCK_KEYMAP_BASE.split(',').collect();
        let mut keymap = Vec::new();
        for (idx, name) in names.iter().enumerate() {
            let uid = qmk_name_to_usage_id(name);
            if is_letter(uid) {
                let row = idx / 12;
                let col = idx % 12;
                keymap.push((row, col, uid));
            }
        }

        let table = generate_proximity_table(layout, &keymap);

        let mut display = String::new();
        write!(display, "     ").unwrap();
        for i in 0..LETTER_COUNT {
            write!(
                display,
                "{:>4}",
                (b'a' + u8::try_from(i).unwrap_or(0)) as char
            )
            .unwrap();
        }
        display.push('\n');
        for (i, row) in table.iter().enumerate() {
            write!(
                display,
                "  {} ",
                (b'a' + u8::try_from(i).unwrap_or(0)) as char
            )
            .unwrap();
            for cell in row.iter() {
                write!(display, "{:>4}", cell).unwrap();
            }
            display.push('\n');
        }

        insta::assert_snapshot!("planck_proximity", display);
    }

    const PLANCK_INFO_JSON: &str = r#"{
        "keyboard_name": "planck/rev6",
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

    const PLANCK_KEYMAP_BASE: &str =
        "KC_TAB, KC_Q, KC_W, KC_E, KC_R, KC_T, KC_Y, KC_U, KC_I, KC_O, KC_P, KC_BSPC, \
         KC_ESC, KC_A, KC_S, KC_D, KC_F, KC_G, KC_H, KC_J, KC_K, KC_L, KC_SCLN, KC_ENT, \
         KC_LSFT, KC_Z, KC_X, KC_C, KC_V, KC_B, KC_N, KC_M, KC_COMM, KC_DOT, KC_SLSH, KC_RSFT, \
         KC_LCTL, KC_LGUI, KC_LALT, KC_LSFT, KC_SPC, KC_SPC, KC_SPC, KC_RSFT, KC_LEFT, KC_DOWN, KC_UP, KC_RGHT";
}
