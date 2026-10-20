//! Exhaustive ZMK shield parsing test.
//!
//! Parses every vendored ZMK shield and asserts that every field we claim
//! to extract is actually present. Failures mean the parser has a gap.
#![expect(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::tests_outside_test_module, reason = "integration test")]

use std::path::{Path, PathBuf};
use tecla_config::zmk::dts;
use tecla_config::zmk::shield::{parse_metadata, parse_shield};

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
fn every_shield_produces_a_board_config() {
    let mut failures = Vec::new();
    let shields = all_shield_dirs();
    assert!(
        shields.len() > 50,
        "only {} ZMK shields discovered — run `git submodule update --init`",
        shields.len()
    );
    for (name, path) in shields {
        if let Err(e) = parse_shield(&path) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(
        failures.is_empty(),
        "Failed to parse {} shields:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_shield_has_matrix_dimensions() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for (name, path) in all_shield_dirs() {
        let config = parse_shield(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Shields with no `keys` feature (dongles, adapters) have no matrix.
        if !config.has_feature("keys") {
            continue;
        }
        checked += 1;
        let (rows, cols) = config.matrix_size;
        if rows == 0 || cols == 0 {
            failures.push(format!("{name}: matrix_size is {:?}", config.matrix_size));
        }
    }
    assert!(
        checked > 0,
        "no keyboard shields discovered — is the ZMK submodule checked out?"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} shields missing matrix dimensions:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_shield_has_gpio_pins() {
    let mut failures = Vec::new();
    for (name, path) in all_shield_dirs() {
        let Ok(config) = parse_shield(&path) else {
            failures.push(format!("{name}: parse_shield failed"));
            continue;
        };
        if !config.has_feature("keys") {
            continue;
        }
        let has_matrix_pins = !config.row_pins.is_empty() && !config.col_pins.is_empty();
        let has_direct_pins = config.row_pins.is_empty() && !config.col_pins.is_empty();
        if !has_matrix_pins && !has_direct_pins {
            failures.push(format!(
                "{name}: no pins (rows={}, cols={})",
                config.row_pins.len(),
                config.col_pins.len()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} shields missing GPIO pins:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_shield_with_keymap_file_produces_layers() {
    let mut failures = Vec::new();
    for (name, path) in all_shield_dirs() {
        let Ok(config) = parse_shield(&path) else {
            continue;
        };
        if !config.has_feature("keys") {
            continue;
        }
        let keymap_path = path.join(format!("{name}.keymap"));
        if !keymap_path.exists() {
            failures.push(format!("{name}: no .keymap file"));
            continue;
        }
        let source = std::fs::read_to_string(&keymap_path).unwrap();
        let layers = dts::extract_bindings(&source);
        if layers.is_empty() {
            failures.push(format!("{name}: .keymap exists but parsed 0 layers"));
        }
        for (i, layer) in layers.iter().enumerate() {
            if layer.is_empty() {
                failures.push(format!("{name}: layer {i} has 0 bindings"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} keymap parsing failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_shield_with_yml_parses_metadata() {
    let mut failures = Vec::new();
    for (name, path) in all_shield_dirs() {
        if name.contains("posix")
            || name.contains("tester")
            || name == "clueboard_california"
            || name == "settings_reset"
        {
            continue;
        }
        let yml_path = path.join(format!("{name}.zmk.yml"));
        if !yml_path.exists() {
            failures.push(format!("{name}: no .zmk.yml file"));
            continue;
        }
        let source = std::fs::read_to_string(&yml_path).unwrap();
        match parse_metadata(&source) {
            Ok(meta) => {
                if meta.id.is_none() && meta.name.is_none() {
                    failures.push(format!("{name}: .zmk.yml parsed but has no id or name"));
                }
            }
            Err(e) => {
                failures.push(format!("{name}: .zmk.yml parse error: {e}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} metadata failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_shield_with_encoder_dts_nodes_extracts_them() {
    let mut failures = Vec::new();
    for (name, path) in all_shield_dirs() {
        let dtsi_path = path.join(format!("{name}.dtsi"));
        let overlay_path = path.join(format!("{name}.overlay"));
        let source_path = if dtsi_path.exists() {
            dtsi_path
        } else if overlay_path.exists() {
            overlay_path
        } else {
            continue;
        };
        let source = std::fs::read_to_string(&source_path).unwrap();
        let has_encoder_nodes = source.contains("alps,ec11");
        if !has_encoder_nodes {
            continue;
        }

        let Ok(config) = parse_shield(&path) else {
            failures.push(format!(
                "{name}: has alps,ec11 nodes but parse_shield failed"
            ));
            continue;
        };
        if config.encoders.is_empty() {
            failures.push(format!(
                "{name}: has alps,ec11 nodes in DTS but parse_shield found 0 encoders"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} encoder extraction failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_split_shield_has_siblings_in_metadata() {
    let mut failures = Vec::new();
    for (name, path) in all_shield_dirs() {
        let left_path = path.join(format!("{name}_left.overlay"));
        let right_path = path.join(format!("{name}_right.overlay"));
        let is_split = left_path.exists() && right_path.exists();
        if !is_split {
            continue;
        }

        let yml_path = path.join(format!("{name}.zmk.yml"));
        if !yml_path.exists() {
            failures.push(format!("{name}: split keyboard but no .zmk.yml"));
            continue;
        }
        let yml = std::fs::read_to_string(&yml_path).unwrap();
        let Ok(meta) = parse_metadata(&yml) else {
            failures.push(format!(
                "{name}: split keyboard but .zmk.yml failed to parse"
            ));
            continue;
        };
        if meta.siblings.is_empty() {
            failures.push(format!(
                "{name}: split keyboard but .zmk.yml has no siblings"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} split shield metadata failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn board_config_matrix_size_matches_layout_keys() {
    let mut failures = Vec::new();
    for (name, path) in all_shield_dirs() {
        let Ok(config) = parse_shield(&path) else {
            continue;
        };
        let (rows, cols) = config.matrix_size;
        if rows == 0 || cols == 0 {
            continue;
        }
        for (layout_name, layout) in &config.layouts {
            for (i, key) in layout.keys.iter().enumerate() {
                if key.row >= rows {
                    failures.push(format!(
                        "{name}/{layout_name}: key {i} row {} >= matrix rows {rows}",
                        key.row
                    ));
                }
                if key.col >= cols {
                    failures.push(format!(
                        "{name}/{layout_name}: key {i} col {} >= matrix cols {cols}",
                        key.col
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} layout key out-of-bounds:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
