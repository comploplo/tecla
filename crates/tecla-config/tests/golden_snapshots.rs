//! Golden snapshot tests — parse real QMK keymap.c files and snapshot the tecla output.
#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![expect(clippy::unwrap_used, reason = "tests use unwrap for brevity")]

use std::path::Path;
use tecla_config::keymap;

fn quantum_defines() -> keymap::QmkDefines {
    let kb = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/quantum");
    if kb.exists() {
        keymap::QmkDefines::from_quantum_dir(&kb)
    } else {
        keymap::QmkDefines::default()
    }
}

#[test]
fn golden_planck_4x12() {
    let input = include_str!("goldens/planck_4x12.c");
    let shared = quantum_defines();
    let layers = keymap::parse_keymap_c_with_shared(input, &shared).unwrap();
    keymap::validate_keycodes(&layers).unwrap();
    let output = keymap::transpile(&layers, Some(12)).unwrap();
    insta::assert_snapshot!("planck_4x12", output);
}

#[test]
fn golden_corne_3x6_3() {
    let input = include_str!("goldens/corne_3x6_3.c");
    let shared = quantum_defines();
    let layers = keymap::parse_keymap_c_with_shared(input, &shared).unwrap();
    keymap::validate_keycodes(&layers).unwrap();
    let output = keymap::transpile(&layers, Some(12)).unwrap();
    insta::assert_snapshot!("corne_3x6_3", output);
}
