#![expect(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::unused_result_ok, reason = "test code")]

use super::RolloverMode;
use super::*;
use crate::action::{Action, KeyCode, KeyOverride, LayerOp, ModifierKey, MouseAction, TriLayerDef};
use crate::devices::keyboard::tapping::{TapHoldConfig, TapHoldMode};
use crate::devices::steno::{CaseMode, OP_RETRO_CASE, OP_SPACE, OP_STITCH, OP_TEXT};
use crate::pipeline::event::{KeyEvent, KeyPos};
use crate::pipeline::keymap::{Layer, LayerStack};
use crate::pipeline::report::{ConsumerReport, HidReport, MouseReport};
use heapless::{Deque, Vec};

const ROWS: usize = 2;
const COLS: usize = 3;
const LAYERS: usize = 2;
const KEYS: usize = 6;

fn pos(row: u8, col: u8) -> KeyPos {
    KeyPos::new(row, col)
}

fn press(pos: KeyPos, ms: u64) -> KeyEvent {
    KeyEvent::press(pos, ms)
}

fn release(pos: KeyPos, ms: u64) -> KeyEvent {
    KeyEvent::release(pos, ms)
}

fn make_processor(base: &[Action]) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    let mut layers: heapless::Vec<Layer<()>, LAYERS> = heapless::Vec::new();
    layers.push(Layer::new(base)).ok();
    let stack: LayerStack<LAYERS, KEYS> = LayerStack::new(layers);
    KeyboardProcessor::new(stack, TapHoldConfig::default())
}

fn make_processor_two_layers(
    base: &[Action],
    overlay: &[Action],
) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    let mut layers: heapless::Vec<Layer<()>, LAYERS> = heapless::Vec::new();
    layers.push(Layer::new(base)).ok();
    layers.push(Layer::new(overlay)).ok();
    let stack: LayerStack<LAYERS, KEYS> = LayerStack::new(layers);
    KeyboardProcessor::new(stack, TapHoldConfig::default())
}

fn collect_reports(f: impl FnOnce(&mut dyn FnMut(HidReport))) -> Vec<HidReport, 16> {
    let mut reports: Vec<HidReport, 16> = Vec::new();
    f(&mut |r| {
        reports.push(r).ok();
    });
    reports
}

fn keyboard_keycodes(report: HidReport) -> [u8; 6] {
    match report {
        HidReport::Keyboard(r) => r.keycodes,
        HidReport::Consumer(_)
        | HidReport::NkroKeyboard(_)
        | HidReport::Mouse(_)
        | HidReport::Midi(_)
        | HidReport::Gamepad(_) => [0; 6],
    }
}

fn keyboard_mods(report: HidReport) -> u8 {
    match report {
        HidReport::Keyboard(r) => r.modifiers,
        HidReport::Consumer(_)
        | HidReport::NkroKeyboard(_)
        | HidReport::Mouse(_)
        | HidReport::Midi(_)
        | HidReport::Gamepad(_) => 0,
    }
}

fn text_bytes(text: &str) -> Vec<u8, 32> {
    let mut bytes = Vec::new();
    bytes.push(OP_TEXT).ok();
    push_compact_len(&mut bytes, text.len());
    bytes.extend_from_slice(text.as_bytes()).ok();
    bytes
}

fn stitch_bytes(text: &str) -> Vec<u8, 32> {
    let mut bytes = Vec::new();
    bytes.push(OP_STITCH).ok();
    push_compact_len(&mut bytes, text.len());
    bytes.extend_from_slice(text.as_bytes()).ok();
    bytes
}

fn push_compact_len(bytes: &mut Vec<u8, 32>, len: usize) {
    if len < 0xFF {
        bytes.push(len as u8).ok();
    } else {
        bytes.push(0xFF).ok();
        bytes.push((len & 0xff) as u8).ok();
        bytes.push(((len >> 8) & 0xff) as u8).ok();
    }
}

// ── Regular key press / release ───────────────────────────────────────────

#[test]
fn press_key_emits_report_with_keycode() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(reports.len(), 1);
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

#[test]
fn release_key_emits_report_without_keycode() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert_eq!(reports.len(), 1);
    assert!(!keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

#[test]
fn retro_case_rewrites_previous_word() {
    let mut proc = make_processor(&[]);
    let mut reports: Vec<HidReport, 64> = Vec::new();

    let word = text_bytes("word");
    let retro = [OP_RETRO_CASE, CaseMode::CapFirstWord as u8];

    proc.playback_steno_translation(word.as_slice(), &mut |r| {
        reports.push(r).ok();
    });
    let first_pass_len = reports.len();

    proc.playback_steno_translation(&retro, &mut |r| {
        reports.push(r).ok();
    });

    let retro_reports = &reports.as_slice()[first_pass_len..];
    let backspaces = retro_reports
        .iter()
        .filter(|report| keyboard_keycodes(**report).contains(&KeyCode::Backspace.usage_id()))
        .count();
    assert_eq!(backspaces, 4);

    assert!(retro_reports.iter().any(|report| {
        keyboard_keycodes(*report).contains(&KeyCode::W.usage_id())
            && keyboard_mods(*report) & ModifierKey::LSHIFT.bits() != 0
    }));
}

#[test]
fn stitch_preserves_spacing_state_for_following_text() {
    let mut proc = make_processor(&[]);
    let mut reports: Vec<HidReport, 64> = Vec::new();

    let mut translation: Vec<u8, 32> = Vec::new();
    translation.push(OP_SPACE).ok();
    translation
        .extend_from_slice(stitch_bytes("ing").as_slice())
        .ok();
    translation
        .extend_from_slice(text_bytes("fox").as_slice())
        .ok();

    proc.playback_steno_translation(translation.as_slice(), &mut |r| {
        reports.push(r).ok();
    });

    let mut seen: Vec<u8, 16> = Vec::new();
    for report in &reports {
        if let HidReport::Keyboard(kr) = report {
            if let Some(kc) = kr.keycodes.iter().copied().find(|kc| *kc != 0) {
                let _ = seen.push(kc);
            }
        }
    }

    assert_eq!(
        seen.as_slice(),
        &[
            KeyCode::I.usage_id(),
            KeyCode::N.usage_id(),
            KeyCode::G.usage_id(),
            KeyCode::Space.usage_id(),
            KeyCode::F.usage_id(),
            KeyCode::O.usage_id(),
            KeyCode::X.usage_id()
        ]
    );
}

#[test]
fn multiple_keys_pressed_simultaneously_appear_in_report() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::B),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 1), 1, emit);
    });
    assert_eq!(reports.len(), 1);
    let kcs = keyboard_keycodes(*reports.first().unwrap());
    assert!(kcs.contains(&KeyCode::A.usage_id()));
    assert!(kcs.contains(&KeyCode::B.usage_id()));
}

#[test]
fn noop_key_emits_no_report() {
    let mut proc = make_processor(&[Action::NoOp; 6]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn duplicate_key_press_does_not_duplicate_keycode_in_report() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 1), 1, emit);
    });
    let kcs = keyboard_keycodes(*reports.first().unwrap());
    let count = kcs.iter().filter(|&&k| k == KeyCode::A.usage_id()).count();
    assert_eq!(count, 1);
}

// ── Modifier keys ─────────────────────────────────────────────────────────

#[test]
fn modifier_press_sets_modifier_byte() {
    let mut proc = make_processor(&[
        Action::Modifier(ModifierKey::LSHIFT),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(reports.len(), 1);
    assert_eq!(
        keyboard_mods(*reports.first().unwrap()),
        ModifierKey::LSHIFT.bits()
    );
}

#[test]
fn modifier_release_clears_modifier_byte() {
    let mut proc = make_processor(&[
        Action::Modifier(ModifierKey::LSHIFT),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert_eq!(keyboard_mods(*reports.first().unwrap()), 0);
}

// ── Consumer keys ─────────────────────────────────────────────────────────

#[test]
fn consumer_press_emits_consumer_report() {
    use crate::action::ConsumerCode;
    let mut proc = make_processor(&[
        Action::Consumer(ConsumerCode::VolumeUp),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(reports.len(), 1);
    assert!(matches!(
        *reports.first().unwrap(),
        HidReport::Consumer(ConsumerReport { keycode: Some(_) })
    ));
}

#[test]
fn consumer_release_emits_empty_consumer_report() {
    use crate::action::ConsumerCode;
    let mut proc = make_processor(&[
        Action::Consumer(ConsumerCode::VolumeUp),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert!(matches!(
        *reports.first().unwrap(),
        HidReport::Consumer(ConsumerReport { keycode: None })
    ));
}

// ── Layer operations ──────────────────────────────────────────────────────

#[test]
fn momentary_layer_key_activates_overlay() {
    let base = [
        Action::Layer(LayerOp::Momentary(1)),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ];
    let overlay = [
        Action::Transparent,
        Action::Key(KeyCode::B),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ];
    let mut proc = make_processor_two_layers(&base, &overlay);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 1), 1, emit);
    });
    assert_eq!(reports.len(), 1);
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
}

#[test]
fn momentary_layer_key_release_deactivates_overlay() {
    let base = [
        Action::Layer(LayerOp::Momentary(1)),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ];
    let overlay = [
        Action::Transparent,
        Action::Key(KeyCode::B),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ];
    let mut proc = make_processor_two_layers(&base, &overlay);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 20), 20, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

// ── Tap-hold: tap resolution ──────────────────────────────────────────────

#[test]
fn tap_hold_tap_emits_press_and_release() {
    let mut proc = make_processor(&[
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 100), 100, emit);
    });
    assert!(reports.len() >= 2);
    let has_press = reports
        .iter()
        .any(|r| keyboard_keycodes(*r).contains(&KeyCode::A.usage_id()));
    let has_release = reports
        .iter()
        .any(|r| !keyboard_keycodes(*r).contains(&KeyCode::A.usage_id()));
    assert!(has_press);
    assert!(has_release);
}

#[test]
fn tap_hold_tap_does_not_activate_modifier() {
    let mut proc = make_processor(&[
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 100), 100, emit);
    });
    assert!(reports.iter().all(|r| keyboard_mods(*r) == 0));
}

// ── Tap-hold: hold resolution via tick ────────────────────────────────────

#[test]
fn tap_hold_hold_via_tick_activates_modifier() {
    let mut proc = make_processor(&[
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.tick(200, emit);
    });
    assert_eq!(reports.len(), 1);
    assert_eq!(
        keyboard_mods(*reports.first().unwrap()),
        ModifierKey::LSHIFT.bits()
    );
}

#[test]
fn tap_hold_hold_release_clears_modifier() {
    let mut proc = make_processor(&[
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.tick(200, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 210), 210, emit);
    });
    assert!(reports.iter().any(|r| keyboard_mods(*r) == 0));
}

#[test]
fn tap_hold_layer_hold_activates_layer() {
    let base = [
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space,
        },
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ];
    let overlay = [
        Action::Transparent,
        Action::Key(KeyCode::B),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ];
    let mut proc = make_processor_two_layers(&base, &overlay);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.tick(200, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 210), 210, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
}

#[test]
fn tap_hold_layer_hold_release_deactivates_layer() {
    let base = [
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space,
        },
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ];
    let overlay = [
        Action::Transparent,
        Action::Key(KeyCode::B),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ];
    let mut proc = make_processor_two_layers(&base, &overlay);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.tick(200, &mut |_| {});
    proc.process_event(release(pos(0, 0), 210), 210, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 220), 220, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

// ── Tap-hold: HoldOnOtherKeyPress ─────────────────────────────────────────

#[test]
fn hokp_hold_resolves_on_other_key_press() {
    let cfg = TapHoldConfig::new(200).with_mode(TapHoldMode::HoldOnOtherKeyPress);
    let mut layers: heapless::Vec<Layer<()>, LAYERS> = heapless::Vec::new();
    layers
        .push(Layer::new(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]))
        .ok();
    let stack: LayerStack<LAYERS, KEYS> = LayerStack::new(layers);
    let mut proc: KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> = KeyboardProcessor::new(stack, cfg);

    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 50), 50, emit);
    });
    assert!(reports
        .iter()
        .any(|r| keyboard_mods(*r) == ModifierKey::LSHIFT.bits()));
}

// ── next_deadline_ms ──────────────────────────────────────────────────────

#[test]
fn next_deadline_ms_is_none_with_no_tap_hold() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert!(proc.next_deadline_ms().is_none());
}

#[test]
fn next_deadline_ms_is_some_while_tap_hold_pending() {
    let mut proc = make_processor(&[
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    assert!(proc.next_deadline_ms().is_some());
}

#[test]
fn next_deadline_ms_is_none_after_tap_hold_resolves() {
    let mut proc = make_processor(&[
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.tick(200, &mut |_| {});
    assert!(proc.next_deadline_ms().is_none());
}

// ── Oneshot modifier ──────────────────────────────────────────────────────

#[test]
fn oneshot_mod_applies_to_next_key() {
    let mut proc = make_processor(&[
        Action::OneShotMod(ModifierKey::LSHIFT),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 10), 10, emit);
    });
    assert!(reports
        .iter()
        .any(|r| keyboard_mods(*r) & ModifierKey::LSHIFT.bits() != 0));
}

#[test]
fn nkro_mode_emits_nkro_report() {
    let layout = [Action::Key(KeyCode::A), Action::NoOp];
    let mut layers = heapless::Vec::<Layer<()>, 1>::new();
    layers.push(Layer::new(&layout)).ok();
    let mut proc: KeyboardProcessor<1, 2, 1, 2> =
        KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
            .with_rollover_mode(RolloverMode::Nkro);

    let pos = KeyPos::new(0, 0);
    let mut reports: heapless::Vec<HidReport, 4> = heapless::Vec::new();
    proc.process_event(KeyEvent::press(pos, 0), 0, &mut |r| {
        reports.push(r).ok();
    });

    assert!(matches!(reports.first(), Some(HidReport::NkroKeyboard(_))));
}

#[test]
fn oneshot_mod_is_cleared_after_next_key() {
    let mut proc = make_processor(&[
        Action::OneShotMod(ModifierKey::LSHIFT),
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::B),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(press(pos(0, 1), 10), 10, &mut |_| {});
    proc.process_event(release(pos(0, 1), 20), 20, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 30), 30, emit);
    });
    assert!(reports
        .iter()
        .all(|r| keyboard_mods(*r) & ModifierKey::LSHIFT.bits() == 0));
}

// ── OneShotLayer press and release ────────────────────────────────────────

#[test]
fn oneshot_layer_press_activates_layer() {
    let mut proc = make_processor_two_layers(
        &[
            Action::OneShotLayer(1),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    );
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 1), 1, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
}

#[test]
fn oneshot_layer_release_deactivates_layer() {
    let mut proc = make_processor_two_layers(
        &[
            Action::OneShotLayer(1),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    );
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 5), 5, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 10), 10, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

// ── process_one: tap-hold active, event is Pending (line 177) ────────────

#[test]
fn second_key_during_tap_hold_is_buffered() {
    let mut proc = make_processor(&[
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
        Action::Key(KeyCode::B),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let mut reports = heapless::Vec::<_, 16>::new();
    proc.process_event(press(pos(0, 1), 50), 50, &mut |r| {
        reports.push(r).ok();
    });
    assert!(reports.is_empty());
}

// ── tick() with no active tap-hold ────────────────────────────────────────

#[test]
fn tick_with_no_tap_hold_does_nothing() {
    let mut proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    let mut reports = heapless::Vec::<_, 16>::new();
    proc.tick(100, &mut |r| {
        reports.push(r).ok();
    });
    assert!(reports.is_empty());
}

// ── tick() pending early-return ───────────────────────────────────────────

#[test]
fn tick_before_deadline_returns_without_emitting() {
    let mut proc = make_processor(&[
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.tick(50, emit);
    });
    assert!(reports.is_empty());
}

// ── keyboard_keycodes / keyboard_mods Consumer and NkroKeyboard arms ────────

#[test]
fn keyboard_keycodes_returns_zeros_for_consumer_report() {
    let report = HidReport::Consumer(ConsumerReport { keycode: None });
    assert_eq!(keyboard_keycodes(report), [0; 6]);
}

#[test]
fn keyboard_mods_returns_zero_for_consumer_report() {
    let report = HidReport::Consumer(ConsumerReport { keycode: None });
    assert_eq!(keyboard_mods(report), 0);
}

#[test]
fn keyboard_keycodes_returns_zeros_for_nkro_report() {
    use crate::pipeline::report::NkroKeyboardReport;
    let report = HidReport::NkroKeyboard(NkroKeyboardReport::default());
    assert_eq!(keyboard_keycodes(report), [0; 6]);
}

#[test]
fn keyboard_mods_returns_zero_for_nkro_report() {
    use crate::pipeline::report::NkroKeyboardReport;
    let report = HidReport::NkroKeyboard(NkroKeyboardReport::default());
    assert_eq!(keyboard_mods(report), 0);
}

// ── Caps Word ────────────────────────────────────────────────────────────

#[test]
fn caps_word_shifts_alpha_keys() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 20), 20, emit);
    });
    assert_eq!(reports.len(), 1);
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    assert_ne!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

#[test]
fn caps_word_does_not_shift_digits() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::N1),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 20), 20, emit);
    });
    assert_eq!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

#[test]
fn caps_word_deactivates_on_space() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::Space),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 1), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    assert_eq!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

#[test]
fn caps_word_continues_on_backspace() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::Backspace),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 1), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    assert_ne!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

#[test]
fn caps_word_shift_does_not_persist_after_release() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 1), 30), 30, emit);
    });
    assert_eq!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

// ── Layer Lock ───────────────────────────────────────────────────────────

#[test]
fn layer_lock_activates_and_persists() {
    let mut proc = make_processor_two_layers(
        &[
            Action::LayerLock(1),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    );
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 20), 20, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
}

#[test]
fn layer_lock_second_press_unlocks() {
    let mut proc = make_processor_two_layers(
        &[
            Action::LayerLock(1),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::LayerLock(1),
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    );
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 0), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 0), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 40), 40, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

// ── Repeat Key ───────────────────────────────────────────────────────────

#[test]
fn repeat_key_emits_last_keycode() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::RepeatKey,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 20), 20, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

#[test]
fn repeat_key_noop_when_nothing_pressed() {
    let mut proc = make_processor(&[
        Action::RepeatKey,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn repeat_key_does_not_update_history() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::RepeatKey,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 1), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 40), 40, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

#[test]
fn repeat_key_release_emits_release() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::RepeatKey,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 1), 30), 30, emit);
    });
    assert!(!keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

// ── Tri-layer ────────────────────────────────────────────────────────────

fn make_processor_three_layers(
    base: &[Action],
    mid: &[Action],
    top: &[Action],
) -> KeyboardProcessor<ROWS, COLS, 3, KEYS> {
    let mut layers: heapless::Vec<Layer<()>, 3> = heapless::Vec::new();
    layers.push(Layer::new(base)).ok();
    layers.push(Layer::new(mid)).ok();
    layers.push(Layer::new(top)).ok();
    KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
}

#[test]
fn tri_layer_activates_when_both_held() {
    static TRI: &[TriLayerDef] = &[TriLayerDef {
        layer1: 1,
        layer2: 2,
        target: 0,
    }];
    let mut proc = make_processor_three_layers(
        &[
            Action::Layer(LayerOp::Momentary(1)),
            Action::Layer(LayerOp::Momentary(2)),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Transparent,
            Action::Key(KeyCode::C),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    )
    .with_tri_layers(TRI);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(press(pos(0, 1), 10), 10, &mut |_| {});
    assert!(proc.layer_stack.state() & 1 != 0);
}

#[test]
fn tri_layer_deactivates_when_one_released() {
    static TRI: &[TriLayerDef] = &[TriLayerDef {
        layer1: 1,
        layer2: 2,
        target: 0,
    }];
    let mut proc = make_processor_three_layers(
        &[
            Action::Layer(LayerOp::Momentary(1)),
            Action::Layer(LayerOp::Momentary(2)),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Transparent,
            Action::Key(KeyCode::C),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    )
    .with_tri_layers(TRI);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(press(pos(0, 1), 10), 10, &mut |_| {});
    proc.process_event(release(pos(0, 0), 20), 20, &mut |_| {});
    assert_eq!(proc.layer_stack.state() & 1, 0);
}

#[test]
fn tri_layer_no_defs_is_noop() {
    let mut proc = make_processor(&[
        Action::Layer(LayerOp::Momentary(1)),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
}

// ── Key Overrides ────────────────────────────────────────────────────────

#[test]
fn key_override_replaces_action_when_mods_match() {
    static OVERRIDES: &[KeyOverride] = &[KeyOverride {
        trigger: KeyCode::Backspace,
        required_mods: ModifierKey::LSHIFT,
        suppress_mods: ModifierKey::NONE,
        replacement: Action::Key(KeyCode::Delete),
    }];
    let mut proc = make_processor(&[
        Action::Modifier(ModifierKey::LSHIFT),
        Action::Key(KeyCode::Backspace),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ])
    .with_overrides(OVERRIDES);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 10), 10, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::Delete.usage_id()));
}

#[test]
fn key_override_does_not_trigger_without_mods() {
    static OVERRIDES: &[KeyOverride] = &[KeyOverride {
        trigger: KeyCode::Backspace,
        required_mods: ModifierKey::LSHIFT,
        suppress_mods: ModifierKey::NONE,
        replacement: Action::Key(KeyCode::Delete),
    }];
    let mut proc = make_processor(&[
        Action::Key(KeyCode::Backspace),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ])
    .with_overrides(OVERRIDES);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::Backspace.usage_id()));
}

#[test]
fn empty_overrides_has_no_effect() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

// ── Dynamic Macros ───────────────────────────────────────────────────────

#[test]
fn macro_record_and_play() {
    let mut proc = make_processor(&[
        Action::MacroRecord(0),
        Action::Key(KeyCode::A),
        Action::MacroStop,
        Action::MacroPlay(0),
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 5), 5, &mut |_| {});
    proc.process_event(press(pos(0, 1), 10), 10, &mut |_| {});
    proc.process_event(release(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(press(pos(0, 2), 30), 30, &mut |_| {});
    proc.process_event(release(pos(0, 2), 35), 35, &mut |_| {});

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(1, 0), 40), 40, emit);
    });
    assert!(reports
        .iter()
        .any(|r| keyboard_keycodes(*r).contains(&KeyCode::A.usage_id())));
}

#[test]
fn macro_play_empty_slot_is_noop() {
    let mut proc = make_processor(&[
        Action::MacroPlay(0),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(reports.is_empty());
}

// ── Send String ──────────────────────────────────────────────────────────

#[test]
fn send_string_emits_keycodes() {
    static SEQ: &[KeyCode] = &[KeyCode::H, KeyCode::I];
    let mut proc = make_processor(&[
        Action::SendString(SEQ),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(reports.len() >= 2);
    assert!(reports
        .iter()
        .any(|r| keyboard_keycodes(*r).contains(&KeyCode::H.usage_id())));
}

#[test]
fn send_string_empty_is_noop() {
    static SEQ: &[KeyCode] = &[];
    let mut proc = make_processor(&[
        Action::SendString(SEQ),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(reports.is_empty());
}

// ── Mouse Keys ───────────────────────────────────────────────────────────

#[test]
fn mouse_click_emits_mouse_report() {
    let mut proc = make_processor(&[
        Action::Mouse(MouseAction::Click(1)),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(reports.len(), 1);
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Mouse(MouseReport {
            buttons: 1,
            dx: 0,
            dy: 0,
            scroll: 0
        })
    );
}

#[test]
fn mouse_click_release_clears_button() {
    let mut proc = make_processor(&[
        Action::Mouse(MouseAction::Click(1)),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Mouse(MouseReport {
            buttons: 0,
            dx: 0,
            dy: 0,
            scroll: 0
        })
    );
}

#[test]
fn mouse_move_emits_delta() {
    let mut proc = make_processor(&[
        Action::Mouse(MouseAction::Move { dx: 10, dy: -5 }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Mouse(MouseReport {
            buttons: 0,
            dx: 10,
            dy: -5,
            scroll: 0
        })
    );
}

#[test]
fn mouse_scroll_emits_scroll() {
    let mut proc = make_processor(&[
        Action::Mouse(MouseAction::Scroll(3)),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Mouse(MouseReport {
            buttons: 0,
            dx: 0,
            dy: 0,
            scroll: 3
        })
    );
}

// ── Bootmagic ────────────────────────────────────────────────────────────

#[test]
fn bootmagic_sets_flag() {
    let mut proc = make_processor(&[
        Action::Bootmagic,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    assert!(!proc.bootmagic_triggered());
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    assert!(proc.bootmagic_triggered());
}

#[test]
fn ble_advertise_sets_pending_command() {
    let mut proc = make_processor(&[
        Action::BleAdvertise,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    assert!(proc.pending_ble_command().is_none());
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    assert_eq!(
        proc.pending_ble_command(),
        Some(crate::transport::ble::BleCommand::Advertise)
    );
    assert!(proc.pending_ble_command().is_none());
}

#[test]
fn ble_disconnect_sets_pending_command() {
    let mut proc = make_processor(&[
        Action::BleDisconnect,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    assert_eq!(
        proc.pending_ble_command(),
        Some(crate::transport::ble::BleCommand::Disconnect)
    );
}

#[test]
fn ble_host_switch_sets_pending_command() {
    let mut proc = make_processor(&[
        Action::BleHostSwitch(2),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    assert_eq!(
        proc.pending_ble_command(),
        Some(crate::transport::ble::BleCommand::SwitchHost(2))
    );
}

// ── State accessors & restore ────────────────────────────────────────────

#[test]
fn default_state_returns_layer_default() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert_eq!(proc.default_state(), 1);
}

#[test]
fn locked_layers_initially_zero() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert_eq!(proc.locked_layers(), 0);
}

#[test]
fn rollover_mode_returns_current() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert_eq!(proc.rollover_mode(), RolloverMode::Kro6);
}

#[test]
fn macro_slot_returns_none_for_invalid() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert!(proc.macro_slot(99).is_none());
}

#[test]
fn macro_slot_returns_some_for_valid() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert!(proc.macro_slot(0).is_some());
}

#[test]
fn restore_state_applies_rollover_mode() {
    let mut proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    proc.restore_state(
        1,
        0,
        RolloverMode::Nkro,
        [Deque::new(), Deque::new(), Deque::new(), Deque::new()],
    );
    assert_eq!(proc.rollover_mode(), RolloverMode::Nkro);
}

#[test]
fn restore_state_applies_locked_layers() {
    let mut proc =
        make_processor_two_layers(&[Action::Key(KeyCode::A); 6], &[Action::Key(KeyCode::B); 6]);
    proc.restore_state(
        1,
        0b10,
        RolloverMode::Kro6,
        [Deque::new(), Deque::new(), Deque::new(), Deque::new()],
    );
    assert_eq!(proc.locked_layers(), 0b10);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
}

#[test]
fn restore_state_applies_non_zero_default_layer() {
    let mut proc =
        make_processor_two_layers(&[Action::Key(KeyCode::A); 6], &[Action::Key(KeyCode::B); 6]);
    proc.restore_state(
        0b10,
        0,
        RolloverMode::Kro6,
        [Deque::new(), Deque::new(), Deque::new(), Deque::new()],
    );
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
}

#[test]
fn restore_state_applies_macros() {
    let mut proc = make_processor(&[
        Action::MacroPlay(0),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let mut slot: Deque<KeyCode, 64> = Deque::new();
    slot.push_back(KeyCode::Z).ok();
    proc.restore_state(
        1,
        0,
        RolloverMode::Kro6,
        [slot, Deque::new(), Deque::new(), Deque::new()],
    );
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(reports
        .iter()
        .any(|r| keyboard_keycodes(*r).contains(&KeyCode::Z.usage_id())));
}

// ── Edge cases: Caps Word ────────────────────────────────────────────────

#[test]
fn caps_word_deactivates_on_enter() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::Enter),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 1), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    assert_eq!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

#[test]
fn caps_word_continues_on_digits() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::N5),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 1), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    assert_ne!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

#[test]
fn caps_word_continues_on_minus() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::Minus),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 1), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    assert_ne!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

#[test]
fn caps_word_continues_on_delete() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::Delete),
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 1), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    assert_ne!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

// ── Edge cases: Dynamic Macros ───────────────────────────────────────────

#[test]
fn macro_stop_without_record_is_harmless() {
    let mut proc = make_processor(&[
        Action::MacroStop,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
}

#[test]
fn macro_record_invalid_slot_is_harmless() {
    let mut proc = make_processor(&[
        Action::MacroRecord(99),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
}

#[test]
fn macro_play_invalid_slot_is_harmless() {
    let mut proc = make_processor(&[
        Action::MacroPlay(99),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn macro_records_multiple_keys_and_replays_in_order() {
    let mut proc = make_processor(&[
        Action::MacroRecord(0),
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::B),
        Action::MacroStop,
        Action::MacroPlay(0),
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 5), 5, &mut |_| {});
    proc.process_event(press(pos(0, 1), 10), 10, &mut |_| {});
    proc.process_event(release(pos(0, 1), 15), 15, &mut |_| {});
    proc.process_event(press(pos(0, 2), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 2), 25), 25, &mut |_| {});
    proc.process_event(press(pos(1, 0), 30), 30, &mut |_| {});
    proc.process_event(release(pos(1, 0), 35), 35, &mut |_| {});

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(1, 1), 40), 40, emit);
    });
    let has_a = reports
        .iter()
        .any(|r| keyboard_keycodes(*r).contains(&KeyCode::A.usage_id()));
    let has_b = reports
        .iter()
        .any(|r| keyboard_keycodes(*r).contains(&KeyCode::B.usage_id()));
    assert!(has_a);
    assert!(has_b);
}

// ── Edge cases: Mouse keys ───────────────────────────────────────────────

#[test]
fn mouse_two_buttons_held_simultaneously() {
    let mut proc = make_processor(&[
        Action::Mouse(MouseAction::Click(1)),
        Action::Mouse(MouseAction::Click(2)),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 10), 10, emit);
    });
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Mouse(MouseReport {
            buttons: 0b11,
            dx: 0,
            dy: 0,
            scroll: 0
        })
    );
}

#[test]
fn mouse_release_one_button_keeps_other() {
    let mut proc = make_processor(&[
        Action::Mouse(MouseAction::Click(1)),
        Action::Mouse(MouseAction::Click(2)),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(press(pos(0, 1), 10), 10, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 20), 20, emit);
    });
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Mouse(MouseReport {
            buttons: 0b10,
            dx: 0,
            dy: 0,
            scroll: 0
        })
    );
}

#[test]
fn mouse_move_release_emits_nothing() {
    let mut proc = make_processor(&[
        Action::Mouse(MouseAction::Move { dx: 5, dy: 5 }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn mouse_scroll_release_emits_nothing() {
    let mut proc = make_processor(&[
        Action::Mouse(MouseAction::Scroll(-1)),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert!(reports.is_empty());
}

// ── Edge cases: Key Overrides ────────────────────────────────────────────

#[test]
fn key_override_with_no_overrides_configured() {
    let mut proc = make_processor(&[
        Action::Key(KeyCode::A),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ])
    .with_overrides(&[]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

// ── Edge cases: Send String ──────────────────────────────────────────────

#[test]
fn send_string_release_is_noop() {
    static SEQ: &[KeyCode] = &[KeyCode::A];
    let mut proc = make_processor(&[
        Action::SendString(SEQ),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert!(reports.is_empty());
}

// ── Integration: full pipeline ───────────────────────────────────────────

#[test]
fn integration_layer_lock_plus_key_override() {
    static OVERRIDES: &[KeyOverride] = &[KeyOverride {
        trigger: KeyCode::B,
        required_mods: ModifierKey::LSHIFT,
        suppress_mods: ModifierKey::NONE,
        replacement: Action::Key(KeyCode::N1),
    }];
    let mut proc = make_processor_two_layers(
        &[
            Action::LayerLock(1),
            Action::Modifier(ModifierKey::LSHIFT),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    )
    .with_overrides(OVERRIDES);

    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});

    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 30), 30, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::N1.usage_id()));
}

#[test]
fn integration_caps_word_with_repeat_key() {
    let mut proc = make_processor(&[
        Action::CapsWord,
        Action::Key(KeyCode::A),
        Action::RepeatKey,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
    proc.process_event(press(pos(0, 1), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 1), 30), 30, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    assert_ne!(keyboard_mods(*reports.first().unwrap()) & 0x02, 0);
}

#[test]
fn integration_tri_layer_with_momentary_holds() {
    static TRI: &[TriLayerDef] = &[TriLayerDef {
        layer1: 1,
        layer2: 2,
        target: 0,
    }];
    let mut proc = make_processor_three_layers(
        &[
            Action::Layer(LayerOp::Momentary(1)),
            Action::Layer(LayerOp::Momentary(2)),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Transparent,
            Action::Key(KeyCode::C),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    )
    .with_tri_layers(TRI);

    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 10), 10, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));

    proc.process_event(release(pos(0, 2), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 0), 30), 30, &mut |_| {});

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
}

#[test]
fn integration_macro_record_play_then_caps_word() {
    let mut proc = make_processor(&[
        Action::MacroRecord(0),
        Action::Key(KeyCode::H),
        Action::MacroStop,
        Action::MacroPlay(0),
        Action::CapsWord,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    proc.process_event(release(pos(0, 0), 5), 5, &mut |_| {});
    proc.process_event(press(pos(0, 1), 10), 10, &mut |_| {});
    proc.process_event(release(pos(0, 1), 15), 15, &mut |_| {});
    proc.process_event(press(pos(0, 2), 20), 20, &mut |_| {});
    proc.process_event(release(pos(0, 2), 25), 25, &mut |_| {});

    proc.process_event(press(pos(1, 1), 30), 30, &mut |_| {});
    proc.process_event(release(pos(1, 1), 35), 35, &mut |_| {});

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(1, 0), 40), 40, emit);
    });
    let has_h = reports
        .iter()
        .any(|r| keyboard_keycodes(*r).contains(&KeyCode::H.usage_id()));
    assert!(has_h);
}

// ── MIDI ─────────────────────────────────────────────────────────────────

#[test]
fn midi_note_on_emits_midi_report() {
    use crate::action::MidiAction;
    use crate::pipeline::report::MidiEvent;
    let mut proc = make_processor(&[
        Action::Midi(MidiAction::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(reports.len(), 1);
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Midi(MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100
        }),
    );
}

#[test]
fn midi_note_on_release_emits_note_off() {
    use crate::action::MidiAction;
    use crate::pipeline::report::MidiEvent;
    let mut proc = make_processor(&[
        Action::Midi(MidiAction::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert_eq!(reports.len(), 1);
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Midi(MidiEvent::NoteOff {
            channel: 0,
            note: 60
        }),
    );
}

#[test]
fn midi_cc_emits_midi_report() {
    use crate::action::MidiAction;
    use crate::pipeline::report::MidiEvent;
    let mut proc = make_processor(&[
        Action::Midi(MidiAction::Cc {
            channel: 1,
            controller: 7,
            value: 64,
        }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Midi(MidiEvent::Cc {
            channel: 1,
            controller: 7,
            value: 64
        }),
    );
}

#[test]
fn midi_cc_release_emits_nothing() {
    use crate::action::MidiAction;
    let mut proc = make_processor(&[
        Action::Midi(MidiAction::Cc {
            channel: 1,
            controller: 7,
            value: 64,
        }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn midi_note_off_action_emits_note_off() {
    use crate::action::MidiAction;
    use crate::pipeline::report::MidiEvent;
    let mut proc = make_processor(&[
        Action::Midi(MidiAction::NoteOff {
            channel: 2,
            note: 48,
        }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Midi(MidiEvent::NoteOff {
            channel: 2,
            note: 48
        }),
    );
}

// ── Gamepad ──────────────────────────────────────────────────────────────

#[test]
fn gamepad_button_press_emits_report() {
    use crate::action::GamepadAction;
    use crate::pipeline::report::GamepadReport;
    let mut proc = make_processor(&[
        Action::Gamepad(GamepadAction::ButtonPress(0)),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(reports.len(), 1);
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Gamepad(GamepadReport {
            buttons: 1,
            axes: [0; 6]
        }),
    );
}

#[test]
fn gamepad_button_release_clears_bit() {
    use crate::action::GamepadAction;
    use crate::pipeline::report::GamepadReport;
    let mut proc = make_processor(&[
        Action::Gamepad(GamepadAction::ButtonPress(0)),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert_eq!(reports.len(), 1);
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Gamepad(GamepadReport {
            buttons: 0,
            axes: [0; 6]
        }),
    );
}

#[test]
fn gamepad_axis_set_emits_report() {
    use crate::action::GamepadAction;
    use crate::pipeline::report::GamepadReport;
    let mut proc = make_processor(&[
        Action::Gamepad(GamepadAction::AxisSet {
            axis: 0,
            value: 127,
        }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    let expected_axes = [127, 0, 0, 0, 0, 0];
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Gamepad(GamepadReport {
            buttons: 0,
            axes: expected_axes
        }),
    );
}

#[test]
fn gamepad_axis_release_does_not_emit() {
    use crate::action::GamepadAction;
    let mut proc = make_processor(&[
        Action::Gamepad(GamepadAction::AxisSet {
            axis: 0,
            value: 127,
        }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn gamepad_axis_out_of_range_ignored() {
    use crate::action::GamepadAction;
    use crate::pipeline::report::GamepadReport;
    let mut proc = make_processor(&[
        Action::Gamepad(GamepadAction::AxisSet {
            axis: 10,
            value: 127,
        }),
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
        Action::NoOp,
    ]);
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(
        *reports.first().unwrap(),
        HidReport::Gamepad(GamepadReport {
            buttons: 0,
            axes: [0; 6]
        }),
    );
}

#[test]
fn keyboard_keycodes_returns_zeros_for_midi_report() {
    use crate::pipeline::report::MidiEvent;
    let report = HidReport::Midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 127,
    });
    assert_eq!(keyboard_keycodes(report), [0; 6]);
}

#[test]
fn keyboard_mods_returns_zero_for_midi_report() {
    use crate::pipeline::report::MidiEvent;
    let report = HidReport::Midi(MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 127,
    });
    assert_eq!(keyboard_mods(report), 0);
}

#[test]
fn keyboard_keycodes_returns_zeros_for_gamepad_report() {
    use crate::pipeline::report::GamepadReport;
    let report = HidReport::Gamepad(GamepadReport::default());
    assert_eq!(keyboard_keycodes(report), [0; 6]);
}

#[test]
fn keyboard_mods_returns_zero_for_gamepad_report() {
    use crate::pipeline::report::GamepadReport;
    let report = HidReport::Gamepad(GamepadReport::default());
    assert_eq!(keyboard_mods(report), 0);
}

#[test]
fn with_autocorrect_attaches_engine() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert!(proc.autocorrect.is_none());
    let proc = proc.with_autocorrect(crate::autocorrect::AutocorrectEngine::new(&[]));
    assert!(proc.autocorrect.is_some());
}

#[test]
fn with_autocomplete_attaches_engine() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert!(proc.autocomplete.is_none());
    let proc = proc.with_autocomplete(crate::autocomplete::AutocompleteEngine::new(&[]));
    assert!(proc.autocomplete.is_some());
}

#[test]
fn set_action_and_get_action_roundtrip() {
    let mut proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert_eq!(proc.get_action(0, 0), Some(Action::Key(KeyCode::A)));
    assert!(proc.set_action(0, 0, Action::Key(KeyCode::Z)));
    assert_eq!(proc.get_action(0, 0), Some(Action::Key(KeyCode::Z)));
}

#[test]
fn layer_mod_activates_layer_and_modifier_on_press() {
    let mut proc = make_processor_two_layers(
        &[
            Action::LayerMod {
                layer: 1,
                modifier: ModifierKey::LSHIFT,
            },
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    );
    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(reports.len(), 1);
    assert_eq!(
        keyboard_mods(*reports.first().unwrap()),
        ModifierKey::LSHIFT.bits()
    );
    assert!(proc.layer_stack.state() & (1 << 1) != 0);

    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });
    assert_eq!(keyboard_mods(*reports.first().unwrap()), 0);
    assert_eq!(proc.layer_stack.state() & (1 << 1), 0);
}

// ── Accessibility ────────────────────────────────────────────────────

fn make_accessible_processor(
    base: &[Action],
    config: crate::accessibility::AccessibilityConfig,
) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    make_processor(base).with_accessibility(config)
}

#[test]
fn bounce_keys_rejects_rapid_repress() {
    let config = crate::accessibility::AccessibilityConfig::default().with_bounce_keys(50);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert_eq!(reports.len(), 1);

    collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 20), 20, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn bounce_keys_accepts_press_after_window() {
    let config = crate::accessibility::AccessibilityConfig::default().with_bounce_keys(50);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 70), 70, emit);
    });
    assert_eq!(reports.len(), 1);
}

#[test]
fn bounce_keys_does_not_affect_different_key() {
    let config = crate::accessibility::AccessibilityConfig::default().with_bounce_keys(50);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 20), 20, emit);
    });
    assert_eq!(reports.len(), 1);
}

#[test]
fn slow_keys_defers_press() {
    let config = crate::accessibility::AccessibilityConfig::default().with_slow_keys(200);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn slow_keys_fires_after_threshold() {
    let config = crate::accessibility::AccessibilityConfig::default().with_slow_keys(200);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });

    let reports = collect_reports(|emit| {
        proc.tick(200, emit);
    });
    assert_eq!(reports.len(), 1);
}

#[test]
fn slow_keys_cancelled_by_early_release() {
    let config = crate::accessibility::AccessibilityConfig::default().with_slow_keys(200);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });

    collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 100), 100, emit);
    });

    let reports = collect_reports(|emit| {
        proc.tick(200, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn slow_keys_deadline_reported() {
    let config = crate::accessibility::AccessibilityConfig::default().with_slow_keys(200);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    assert_eq!(proc.next_deadline_ms(), None);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 50), 50, emit);
    });

    assert_eq!(proc.next_deadline_ms(), Some(250));
}

#[test]
fn slow_keys_multiple_keys_fire_independently() {
    let config = crate::accessibility::AccessibilityConfig::default().with_slow_keys(100);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 50), 50, emit);
    });

    let reports = collect_reports(|emit| {
        proc.tick(100, emit);
    });
    assert_eq!(reports.len(), 1);

    let reports = collect_reports(|emit| {
        proc.tick(150, emit);
    });
    assert_eq!(reports.len(), 1);
}

#[test]
fn sticky_keys_modifier_is_oneshot() {
    let config = crate::accessibility::AccessibilityConfig::default().with_sticky_keys();
    let mut proc = make_accessible_processor(
        &[
            Action::Modifier(ModifierKey::LSHIFT),
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::E),
        ],
        config,
    );

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 20), 20, emit);
    });
    let mods = keyboard_mods(*reports.first().unwrap());
    assert_ne!(mods & 0x02, 0);

    collect_reports(|emit| {
        proc.process_event(release(pos(0, 1), 30), 30, emit);
    });

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 2), 40), 40, emit);
    });
    let mods = keyboard_mods(*reports.first().unwrap());
    assert_eq!(mods & 0x02, 0);
}

#[test]
fn bounce_and_slow_keys_combined() {
    let config = crate::accessibility::AccessibilityConfig::default()
        .with_bounce_keys(50)
        .with_slow_keys(100);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    let reports = collect_reports(|emit| {
        proc.tick(100, emit);
    });
    assert_eq!(reports.len(), 1);

    collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 110), 110, emit);
    });

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 120), 120, emit);
    });
    assert!(reports.is_empty());

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 200), 200, emit);
    });
    let reports2 = collect_reports(|emit| {
        proc.tick(300, emit);
    });
    assert!(reports.is_empty());
    assert_eq!(reports2.len(), 1);
}

#[test]
fn bounce_keys_toggle_action() {
    let mut proc = make_accessible_processor(
        &[
            Action::BounceKeysToggle,
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::E),
        ],
        crate::accessibility::AccessibilityConfig::default(),
    );

    assert!(proc
        .accessibility
        .as_ref()
        .unwrap()
        .config
        .bounce_keys_ms
        .is_none());

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });

    assert_eq!(
        proc.accessibility.as_ref().unwrap().config.bounce_keys_ms,
        Some(100)
    );

    collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
        proc.process_event(press(pos(0, 0), 200), 200, emit);
    });

    assert!(proc
        .accessibility
        .as_ref()
        .unwrap()
        .config
        .bounce_keys_ms
        .is_none());
}

#[test]
fn slow_keys_toggle_action() {
    let mut proc = make_accessible_processor(
        &[
            Action::SlowKeysToggle,
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::E),
        ],
        crate::accessibility::AccessibilityConfig::default(),
    );

    assert!(proc
        .accessibility
        .as_ref()
        .unwrap()
        .config
        .slow_keys_ms
        .is_none());

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });

    assert_eq!(
        proc.accessibility.as_ref().unwrap().config.slow_keys_ms,
        Some(300)
    );
}

#[test]
fn sticky_keys_toggle_action() {
    let mut proc = make_accessible_processor(
        &[
            Action::StickyKeysToggle,
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::E),
        ],
        crate::accessibility::AccessibilityConfig::default(),
    );

    assert!(!proc.accessibility.as_ref().unwrap().config.sticky_keys);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });

    assert!(proc.accessibility.as_ref().unwrap().config.sticky_keys);
}

#[test]
fn with_accessibility_attaches_state() {
    let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
    assert!(proc.accessibility.is_none());
    let proc = proc.with_accessibility(crate::accessibility::AccessibilityConfig::default());
    assert!(proc.accessibility.is_some());
}

#[test]
fn bounce_keys_exact_boundary_accepted() {
    let config = crate::accessibility::AccessibilityConfig::default().with_bounce_keys(50);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 10), 10, emit);
    });

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 60), 60, emit);
    });
    assert_eq!(reports.len(), 1);
}

#[test]
fn slow_keys_not_fired_at_threshold_minus_one() {
    let config = crate::accessibility::AccessibilityConfig::default().with_slow_keys(200);
    let mut proc = make_accessible_processor(&[Action::Key(KeyCode::A); 6], config);

    collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });

    let reports = collect_reports(|emit| {
        proc.tick(199, emit);
    });
    assert!(reports.is_empty());
}

#[test]
fn sticky_keys_disabled_modifier_held_normally() {
    let config = crate::accessibility::AccessibilityConfig::default();
    let mut proc = make_accessible_processor(
        &[
            Action::Modifier(ModifierKey::LSHIFT),
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::E),
        ],
        config,
    );

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 0), 0), 0, emit);
    });
    let mods = keyboard_mods(*reports.first().unwrap());
    assert_ne!(mods & 0x02, 0);

    let reports = collect_reports(|emit| {
        proc.process_event(press(pos(0, 1), 10), 10, emit);
    });
    let mods = keyboard_mods(*reports.first().unwrap());
    assert_ne!(mods & 0x02, 0);

    let reports = collect_reports(|emit| {
        proc.process_event(release(pos(0, 0), 20), 20, emit);
    });
    let mods = keyboard_mods(*reports.first().unwrap());
    assert_eq!(mods & 0x02, 0);
}
