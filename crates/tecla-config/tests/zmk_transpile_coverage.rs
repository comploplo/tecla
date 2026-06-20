//! ZMK transpiler coverage gap test.
//!
//! Attempts to transpile every binding in every vendored ZMK shield keymap.
//! Collects all failures and produces an insta snapshot of unsupported
//! behaviors and keycodes, sorted by frequency. As we add support for more
//! behaviors, this snapshot shrinks toward zero.
#![expect(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::tests_outside_test_module, reason = "integration test")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tecla_config::zmk::dts;
use tecla_config::zmk::keymap::transpiler::binding_to_tecla;

fn shields_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/zmk_shields");
    assert!(
        dir.exists(),
        "ZMK shields not found at {dir:?} — run `git submodule update --init`"
    );
    dir
}

fn all_shield_dirs() -> Vec<(String, PathBuf)> {
    let dir = shields_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

#[test]
fn zmk_transpile_coverage_gap() {
    let mut unknown_behaviors: BTreeMap<String, usize> = BTreeMap::new();
    let mut unknown_keys: BTreeMap<String, usize> = BTreeMap::new();
    let mut unknown_mods: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_bindings = 0usize;
    let mut successful_bindings = 0usize;
    let mut shields_with_keymaps = 0usize;

    for (name, path) in all_shield_dirs() {
        if name.starts_with("tester_") {
            continue;
        }
        let keymap_path = path.join(format!("{name}.keymap"));
        if !keymap_path.exists() {
            continue;
        }
        let source = std::fs::read_to_string(&keymap_path).unwrap();
        let layers = dts::extract_bindings(&source);
        if layers.is_empty() {
            continue;
        }

        shields_with_keymaps += 1;

        for layer in &layers {
            for binding in layer {
                total_bindings += 1;
                match binding_to_tecla(binding) {
                    Ok(_) => successful_bindings += 1,
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.starts_with("unknown behavior:") {
                            *unknown_behaviors
                                .entry(binding.behavior.clone())
                                .or_default() += 1;
                        } else if msg.starts_with("unknown key:") {
                            let key = binding.args.first().cloned().unwrap_or_default();
                            *unknown_keys.entry(key).or_default() += 1;
                        } else if msg.starts_with("unknown modifier:") {
                            let modifier = binding.args.first().cloned().unwrap_or_default();
                            *unknown_mods.entry(modifier).or_default() += 1;
                        }
                    }
                }
            }
        }
    }

    let mut report = String::new();
    report.push_str(&format!(
        "shields with keymaps: {shields_with_keymaps}\n\
         total bindings: {total_bindings}\n\
         successful: {successful_bindings}\n\
         failed: {}\n\
         coverage: {:.1}%\n",
        total_bindings - successful_bindings,
        if total_bindings > 0 {
            successful_bindings as f64 / total_bindings as f64 * 100.0
        } else {
            0.0
        },
    ));

    if !unknown_behaviors.is_empty() {
        report.push_str("\nunknown behaviors:\n");
        let mut sorted: Vec<_> = unknown_behaviors.into_iter().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.1));
        for (behavior, count) in &sorted {
            report.push_str(&format!("  &{behavior}: {count}\n"));
        }
    }

    if !unknown_keys.is_empty() {
        report.push_str("\nunknown keys:\n");
        let mut sorted: Vec<_> = unknown_keys.into_iter().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.1));
        for (key, count) in &sorted {
            report.push_str(&format!("  {key}: {count}\n"));
        }
    }

    if !unknown_mods.is_empty() {
        report.push_str("\nunknown modifiers:\n");
        let mut sorted: Vec<_> = unknown_mods.into_iter().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.1));
        for (modifier, count) in &sorted {
            report.push_str(&format!("  {modifier}: {count}\n"));
        }
    }

    insta::assert_snapshot!(report);
}
