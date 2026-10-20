//! End-to-end integration test: parse a real QMK keyboard config, build a
//! KeyboardProcessor from the parsed keycodes, press every key, and verify
//! HID reports are correct.
//!
//! This tests the full pipeline: QMK parse → numeric resolution → classify →
//! Action conversion → KeyboardProcessor → HidReport.

#![allow(clippy::tests_outside_test_module, reason = "integration test")]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::expect_used, reason = "test code")]
#![expect(clippy::panic, reason = "test assertions")]
#![expect(clippy::unused_result_ok, reason = "test code")]

use std::path::Path;

use heapless::Vec as HVec;
use tecla::action::{Action, KeyCode, LayerOp, ModifierKey};
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::keymap::{Layer, LayerStack};
use tecla::pipeline::report::HidReport;
use tecla_config::qmk::keymap::{parse_keymap_c_with_shared, QmkDefines, ResolvedKeycode};

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn load_shared_defines() -> QmkDefines {
    let quantum = workspace_root().join("keyboards/quantum");
    assert!(
        quantum.exists(),
        "QMK quantum directory not found at {quantum:?} — run `git submodule update --init`"
    );
    tecla_config::qmk::keymap::QmkDefines::from_quantum_dir(&quantum)
}

fn qmk_mods_to_modifier(mods: u8) -> Option<ModifierKey> {
    let right = mods & 0x10 != 0;
    let base = mods & 0x0F;
    match (base, right) {
        (0x01, false) => Some(ModifierKey::LCTRL),
        (0x01, true) => Some(ModifierKey::RCTRL),
        (0x02, false) => Some(ModifierKey::LSHIFT),
        (0x02, true) => Some(ModifierKey::RSHIFT),
        (0x04, false) => Some(ModifierKey::LALT),
        (0x04, true) => Some(ModifierKey::RALT),
        (0x08, false) => Some(ModifierKey::LGUI),
        (0x08, true) => Some(ModifierKey::RGUI),
        _ => None,
    }
}

fn resolved_to_action(kc: &ResolvedKeycode) -> Action<()> {
    match kc {
        ResolvedKeycode::NoOp => Action::NoOp,
        ResolvedKeycode::Transparent => Action::Transparent,
        ResolvedKeycode::HidKey(id) => {
            if (0xE0..=0xE7).contains(id) {
                let mk = match id {
                    0xE0 => ModifierKey::LCTRL,
                    0xE1 => ModifierKey::LSHIFT,
                    0xE2 => ModifierKey::LALT,
                    0xE3 => ModifierKey::LGUI,
                    0xE4 => ModifierKey::RCTRL,
                    0xE5 => ModifierKey::RSHIFT,
                    0xE6 => ModifierKey::RALT,
                    0xE7 => ModifierKey::RGUI,
                    _ => panic!("unexpected modifier HID ID"),
                };
                Action::Modifier(mk)
            } else if let Some(kc) = KeyCode::from_usage_id(*id) {
                Action::Key(kc)
            } else {
                Action::NoOp
            }
        }
        ResolvedKeycode::Modified { mods, key } => {
            if let (Some(m), Some(k)) = (qmk_mods_to_modifier(*mods), KeyCode::from_usage_id(*key))
            {
                Action::ModifiedKey {
                    modifier: m,
                    key: k,
                }
            } else {
                Action::NoOp
            }
        }
        ResolvedKeycode::Momentary(n) => Action::Layer(LayerOp::Momentary(*n)),
        ResolvedKeycode::Toggle(n) => Action::Layer(LayerOp::Toggle(*n)),
        ResolvedKeycode::ToLayer(n) => Action::Layer(LayerOp::On(*n)),
        ResolvedKeycode::DefaultLayer(n) => Action::Layer(LayerOp::SetDefault(*n)),
        ResolvedKeycode::TapToggle(n) => Action::TapToggle(*n),
        ResolvedKeycode::OneShotLayer(n) => Action::OneShotLayer(*n),
        ResolvedKeycode::OneShotMod(m) => {
            qmk_mods_to_modifier(*m).map_or(Action::NoOp, Action::OneShotMod)
        }
        ResolvedKeycode::LayerTap { layer, key } => {
            if let Some(k) = KeyCode::from_usage_id(*key) {
                Action::LayerTap {
                    layer: *layer,
                    tap: k,
                }
            } else {
                Action::Layer(LayerOp::Momentary(*layer))
            }
        }
        ResolvedKeycode::ModTap { mods, key } => {
            if let (Some(m), Some(k)) = (qmk_mods_to_modifier(*mods), KeyCode::from_usage_id(*key))
            {
                Action::ModTap { hold: m, tap: k }
            } else {
                Action::NoOp
            }
        }
        ResolvedKeycode::LayerMod { layer, mods } => {
            if let Some(m) = qmk_mods_to_modifier(*mods) {
                Action::LayerMod {
                    layer: *layer,
                    modifier: m,
                }
            } else {
                Action::Layer(LayerOp::Momentary(*layer))
            }
        }
        _ => Action::NoOp,
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::integer_division_remainder_used,
    reason = "keyboard dimensions fit in u8; integer division is the correct operation"
)]
fn pos_for_index(index: usize, cols: usize) -> KeyPos {
    KeyPos::new((index / cols) as u8, (index % cols) as u8)
}

#[test]
#[expect(
    clippy::integer_division_remainder_used,
    reason = "computing rows from key count"
)]
fn planck_qmk_to_processor_round_trip() {
    const MAX_LAYERS: usize = 16;
    const MAX_KEYS: usize = 48;
    const ROWS: usize = 4;
    const COLS: usize = 12;

    let root = workspace_root();
    let keymap_path = root.join("keyboards/keyboards/planck/keymaps/default/keymap.c");
    assert!(
        keymap_path.exists(),
        "planck keymap not found at {keymap_path:?} — run `git submodule update --init`"
    );

    let source = std::fs::read_to_string(&keymap_path).unwrap();
    let shared = load_shared_defines();
    let layers = parse_keymap_c_with_shared(&source, &shared).unwrap();

    assert!(!layers.is_empty(), "no layers parsed");
    let keys_per_layer = layers.first().expect("no layers parsed").keys.len();
    assert!(keys_per_layer > 0, "empty layer");

    let cols: usize = 12;
    let rows = keys_per_layer / cols;
    assert_eq!(
        rows * cols,
        keys_per_layer,
        "key count not divisible by 12 columns"
    );

    let mut action_layers: std::vec::Vec<std::vec::Vec<Action<()>>> = std::vec::Vec::new();
    for layer in &layers {
        let actions: std::vec::Vec<Action<()>> =
            layer.keys.iter().map(resolved_to_action).collect();
        action_layers.push(actions);
    }

    let mut hlayers = HVec::<Layer<()>, MAX_LAYERS>::new();
    for al in &action_layers {
        let mut hkeys = HVec::<Action<()>, MAX_KEYS>::new();
        for a in al {
            hkeys.push(*a).ok();
        }
        hlayers.push(Layer::new(&hkeys)).ok();
    }
    let stack = LayerStack::new(hlayers);
    let mut proc =
        KeyboardProcessor::<ROWS, COLS, MAX_LAYERS, MAX_KEYS>::new(stack, TapHoldConfig::default());

    let mut press_results: std::vec::Vec<(KeyPos, Action<()>, std::vec::Vec<HidReport>)> =
        std::vec::Vec::new();
    let mut time = 100u64;

    let base_layer = layers.first().expect("no layers parsed");
    for (i, kc) in base_layer.keys.iter().enumerate() {
        let pos = pos_for_index(i, cols);
        let action = resolved_to_action(kc);

        if matches!(action, Action::NoOp | Action::Transparent) {
            continue;
        }
        if action.needs_tap_hold() {
            continue;
        }

        let mut reports = std::vec::Vec::new();
        proc.process_event(KeyEvent::press(pos, time), time, &mut |r| reports.push(r));
        time += 10;
        let mut release_reports = std::vec::Vec::new();
        proc.process_event(KeyEvent::release(pos, time), time, &mut |r| {
            release_reports.push(r);
        });
        time += 10;

        if !reports.is_empty() {
            press_results.push((pos, action, reports));
        }
    }

    let keys_with_output = press_results.len();
    assert!(
        keys_with_output > 20,
        "expected at least 20 keys to produce HID reports on the Planck base layer, got {keys_with_output}"
    );

    // The round trip only means something if the emitted HID matches the
    // keycode the QMK source actually named — a constant-returning keymap
    // satisfies "non-empty report" just as well.
    for (pos, action, reports) in &press_results {
        let expected = match action {
            Action::Key(kc) => Some(*kc),
            _ => None,
        };
        for report in reports {
            match report {
                HidReport::Keyboard(kr) => {
                    if let Some(kc) = expected {
                        assert!(
                            kr.keycodes.contains(&kc.usage_id()),
                            "key at ({},{}) resolved to {kc:?} (usage {}), but the report carried {:?}",
                            pos.row,
                            pos.col,
                            kc.usage_id(),
                            kr.keycodes,
                        );
                    } else {
                        let has_keycode = kr.keycodes.iter().any(|&k| k != 0);
                        let has_modifier = kr.modifiers != 0;
                        assert!(
                            has_keycode || has_modifier,
                            "key at ({},{}) produced an empty keyboard report",
                            pos.row,
                            pos.col,
                        );
                    }
                }
                HidReport::Consumer(_) | HidReport::NkroKeyboard(_) | HidReport::Mouse(_) => {}
                _ => {}
            }
        }
    }

    #[expect(clippy::print_stderr, reason = "test diagnostic output")]
    {
        eprintln!(
            "planck round-trip: {} layers, {} keys/layer, {} base-layer keys produced HID reports",
            layers.len(),
            keys_per_layer,
            keys_with_output,
        );
    }
}

#[test]
fn resolved_to_action_covers_all_basic_keys() {
    for id in 0x04..=0x63u8 {
        let resolved = ResolvedKeycode::HidKey(id);
        let action = resolved_to_action(&resolved);
        assert!(
            matches!(action, Action::Key(_)),
            "HID key 0x{id:02X} should map to Action::Key, got {action:?}"
        );
    }
}

#[test]
fn resolved_to_action_modifiers() {
    for id in 0xE0..=0xE7u8 {
        let resolved = ResolvedKeycode::HidKey(id);
        let action = resolved_to_action(&resolved);
        assert!(
            matches!(action, Action::Modifier(_)),
            "HID modifier 0x{id:02X} should map to Action::Modifier, got {action:?}"
        );
    }
}

#[test]
fn resolved_to_action_layer_ops() {
    assert!(matches!(
        resolved_to_action(&ResolvedKeycode::Momentary(1)),
        Action::Layer(LayerOp::Momentary(1))
    ));
    assert!(matches!(
        resolved_to_action(&ResolvedKeycode::Toggle(2)),
        Action::Layer(LayerOp::Toggle(2))
    ));
    assert!(matches!(
        resolved_to_action(&ResolvedKeycode::LayerTap {
            layer: 1,
            key: 0x2C
        }),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space
        }
    ));
}
