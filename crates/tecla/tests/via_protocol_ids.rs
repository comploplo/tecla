//! Pins tecla's Via command IDs to QMK's `quantum/via.h`.
//!
//! These numbers are a wire protocol owned by Via's host application, not by
//! this project. Comparing with the upstream header independently of the
//! implementation prevents a copied constant table from validating itself.
//!
//! QMK is vendored at `.vendor/qmk_firmware`, so the authoritative header is
//! right here: parse it and compare, rather than trusting a copied table.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Parses `enum <name> { ident = 0xNN, ... }` out of a C header.
fn parse_enum(source: &str, enum_name: &str) -> HashMap<String, u8> {
    let start = source
        .find(&format!("enum {enum_name}"))
        .unwrap_or_else(|| panic!("`enum {enum_name}` not found in via.h"));
    let body = &source[start..];
    let end = body.find('}').expect("unterminated enum");
    let mut out = HashMap::new();
    for line in body[..end].lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_end_matches(',').trim();
        let Some(hex) = value.strip_prefix("0x") else {
            continue;
        };
        if let Ok(v) = u8::from_str_radix(hex, 16) {
            out.insert(name.trim().to_string(), v);
        }
    }
    assert!(!out.is_empty(), "parsed no variants from enum {enum_name}");
    out
}

fn via_header() -> String {
    let path = workspace_root().join(".vendor/qmk_firmware/quantum/via.h");
    assert!(
        path.exists(),
        "{path:?} missing — run `git submodule update --init`"
    );
    std::fs::read_to_string(path).expect("read via.h")
}

#[test]
fn command_ids_match_the_vendored_qmk_header() {
    let ids = parse_enum(&via_header(), "via_command_id");

    // (QMK name, the value tecla uses). Kept as literals on purpose: this test
    // is worthless if it imports the same constants the implementation uses.
    let expected = [
        ("id_get_protocol_version", 0x01u8),
        ("id_get_keyboard_value", 0x02),
        ("id_dynamic_keymap_get_keycode", 0x04),
        ("id_dynamic_keymap_set_keycode", 0x05),
        ("id_dynamic_keymap_reset", 0x06),
        ("id_dynamic_keymap_get_layer_count", 0x11),
        ("id_unhandled", 0xFF),
    ];

    for (qmk_name, tecla_value) in expected {
        let actual = ids
            .get(qmk_name)
            .unwrap_or_else(|| panic!("{qmk_name} not in via.h"));
        assert_eq!(
            *actual, tecla_value,
            "{qmk_name} is 0x{actual:02X} in QMK but tecla uses 0x{tecla_value:02X}"
        );
    }
}

#[test]
fn keyboard_value_sub_ids_match_the_vendored_qmk_header() {
    let ids = parse_enum(&via_header(), "via_keyboard_value_id");
    for (qmk_name, tecla_value) in [
        ("id_uptime", 0x01u8),
        ("id_layout_options", 0x02),
        ("id_firmware_version", 0x04),
    ] {
        let actual = ids
            .get(qmk_name)
            .unwrap_or_else(|| panic!("{qmk_name} not in via.h"));
        assert_eq!(
            *actual, tecla_value,
            "{qmk_name} is 0x{actual:02X} in QMK but tecla uses 0x{tecla_value:02X}"
        );
    }
}

#[test]
fn teclas_private_eeprom_commands_do_not_collide_with_via() {
    // 0x23/0x24 (EEPROM) and 0x25/0x26 (telemetry) are tecla extensions. If Via
    // ever assigns them, the two meanings would silently conflict on the wire.
    let ids = parse_enum(&via_header(), "via_command_id");
    for tecla_private in [0x23u8, 0x24, 0x25, 0x26] {
        assert!(
            !ids.values().any(|&v| v == tecla_private),
            "0x{tecla_private:02X} is now assigned by Via; pick another id for the tecla-private commands"
        );
    }
}
