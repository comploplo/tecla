//! Integration test: parse every vendored QMK keyboard config and keymap.
#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![expect(clippy::unwrap_used, reason = "tests use unwrap for brevity")]
#![expect(
    clippy::print_stderr,
    reason = "test diagnostics use eprintln for visibility"
)]
#![expect(
    clippy::non_ascii_literal,
    reason = "em-dash in test messages is intentional"
)]

use std::path::{Path, PathBuf};
use tecla_config::qmk::{info_json, keymap};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn walk_files(dir: &Path, name: &str, results: &mut Vec<(String, bool)>) {
    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk_files(&path, name, results);
                } else if path.file_name().is_some_and(|n| n == name) {
                    let kb_root = workspace_root().join("keyboards");
                    let rel = path.strip_prefix(&kb_root).unwrap_or(&path);
                    let key = rel.display().to_string();
                    let content =
                        std::fs::read_to_string(&path).expect("failed to read discovered file");
                    let ok = if name == "keymap.c" {
                        keymap::parse_keymap_c(&content).is_ok()
                    } else {
                        info_json::parse(&content).is_ok()
                    };
                    results.push((key, ok));
                }
            }
        }
    }
}

#[test]
fn parse_all_qmk_info_json() {
    let fixtures = workspace_root().join("keyboards");
    assert!(
        fixtures.exists(),
        "keyboards/ directory missing — run from repo root"
    );

    let mut results = Vec::new();
    walk_files(&fixtures, "info.json", &mut results);
    walk_files(&fixtures, "keyboard.json", &mut results);

    let total = results.len();
    let passed = results.iter().filter(|(_, ok)| *ok).count();
    let failed = total - passed;

    eprintln!("info.json + keyboard.json: {passed}/{total} passed, {failed} failed");

    if failed > 0 {
        let failures: Vec<_> = results.iter().filter(|(_, ok)| !*ok).take(20).collect();
        for (path, _) in &failures {
            eprintln!("  FAIL: {path}");
        }
        if failed > 20 {
            eprintln!("  ... and {} more", failed - 20);
        }
    }

    assert_eq!(failed, 0, "info.json: {failed}/{total} failed to parse");
}

#[test]
fn parse_all_qmk_keymap_c() {
    let fixtures = workspace_root().join("keyboards");
    assert!(
        fixtures.exists(),
        "keyboards/ directory missing — run from repo root"
    );

    let mut results = Vec::new();
    walk_files(&fixtures, "keymap.c", &mut results);

    let total = results.len();
    let passed = results.iter().filter(|(_, ok)| *ok).count();
    let failed = total - passed;

    eprintln!("keymap.c: {passed}/{total} passed, {failed} failed");

    if failed > 0 {
        let failures: Vec<_> = results.iter().filter(|(_, ok)| !*ok).take(20).collect();
        for (path, _) in &failures {
            eprintln!("  FAIL: {path}");
        }
        if failed > 20 {
            eprintln!("  ... and {} more", failed - 20);
        }
    }

    assert_eq!(failed, 0, "keymap.c: {failed}/{total} failed to parse");
}
