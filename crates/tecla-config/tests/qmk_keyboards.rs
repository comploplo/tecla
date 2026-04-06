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
#![expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "usize-to-f64 for percentage display in test output"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tecla_config::{info_json, keymap};

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
                    let content = std::fs::read_to_string(&path).unwrap_or_default();
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

#[test]
fn transpile_coverage() {
    let kb = workspace_root().join("keyboards");
    assert!(kb.exists(), "keyboards/ directory missing");

    let mut keymap_paths = Vec::new();
    walk_keymap_files(&kb, &mut keymap_paths);

    let shared = keymap::QmkDefines::from_quantum_dir(&kb.join("quantum"));

    let mut total_keys: usize = 0;
    let mut mapped_keys: usize = 0;
    let mut unmapped_freq: BTreeMap<String, usize> = BTreeMap::new();
    let mut passthrough_freq: BTreeMap<String, usize> = BTreeMap::new();
    let mut passthrough_total: usize = 0;

    for path in &keymap_paths {
        let path_str = path.display().to_string();
        // wilba_tech keyboards use proprietary LED effect keycodes (EF_*, BR_*, H1_*, etc.)
        // that are defined in custom per-keyboard headers outside the QMK standard
        if path_str.contains("wilba_tech") {
            continue;
        }
        let Ok(layers) = keymap::parse_keymap_c_with_includes(path, &shared) else {
            continue;
        };
        for layer in &layers {
            for kc in &layer.keys {
                total_keys += 1;
                let _tecla = keymap::keycode_to_tecla(kc);
                if let keymap::ResolvedKeycode::Unresolved { ref raw, .. } = kc {
                    *unmapped_freq.entry(raw.clone()).or_default() += 1;
                    continue;
                }
                mapped_keys += 1;
                if is_passthrough(kc) {
                    passthrough_total += 1;
                    *passthrough_freq.entry(format!("{kc:?}")).or_default() += 1;
                }
            }
        }
    }

    let unmapped = total_keys - mapped_keys;
    let pct = (mapped_keys as f64) / (total_keys.max(1) as f64) * 100.0;
    eprintln!("\ntranspile coverage: {mapped_keys}/{total_keys} mapped ({pct:.1}%)");
    eprintln!(
        "{unmapped} unmapped keycode instances, {} distinct",
        unmapped_freq.len()
    );

    let mut unmapped_by_freq: Vec<_> = unmapped_freq.into_iter().collect();
    unmapped_by_freq.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("\nTop 60 unresolved:");
    for (kc, count) in unmapped_by_freq.iter().take(60) {
        eprintln!("  {count:>5}x  {kc}");
    }
    if unmapped_by_freq.len() > 60 {
        eprintln!("  ... and {} more", unmapped_by_freq.len() - 60);
    }

    assert!(
        pct > 50.0,
        "transpile coverage {pct:.1}% below 50% threshold"
    );

    let mut by_freq: Vec<_> = passthrough_freq.into_iter().collect();
    by_freq.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!(
        "\n{passthrough_total} passthrough instances, {} distinct",
        by_freq.len()
    );
    eprintln!("\nTop 60:");
    for (kc, count) in by_freq.iter().take(60) {
        eprintln!("  {count:>5}x  {kc}");
    }
    if by_freq.len() > 60 {
        eprintln!("  ... and {} more", by_freq.len() - 60);
    }
}

fn walk_keymap_files(dir: &Path, results: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "quantum") {
                    continue;
                }
                walk_keymap_files(&path, results);
            } else if path.file_name().is_some_and(|n| n == "keymap.c") {
                results.push(path);
            }
        }
    }
}

fn is_passthrough(kc: &keymap::ResolvedKeycode) -> bool {
    matches!(
        kc,
        keymap::ResolvedKeycode::User(_) | keymap::ResolvedKeycode::Kb(_)
    )
}
