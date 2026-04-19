#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]

mod common;
use common::{collect, pos, press, release};

use heapless::Vec as HVec;
use std::vec::Vec;
use tecla::action::{Action, GamepadAction, KeyCode, MidiAction, ModifierKey};
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::devices::steno::{OP_TEXT, OP_UNDO_LAST_TRANSLATION};
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::KeyEvent;
use tecla::pipeline::keymap::{Layer, LayerStack};
use tecla::pipeline::report::HidReport;
use tecla_config::trie::compile_steno_dictionary;

const ROWS: usize = 2;
const COLS: usize = 6;
const LAYERS: usize = 2;
const KEYS: usize = 12;

fn make_keyboard(keys: &[Action<()>; KEYS]) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    let mut layers = HVec::<Layer<()>, LAYERS>::new();
    layers.push(Layer::new(keys)).ok();
    layers.push(Layer::new(keys)).ok();
    KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default()).with_telemetry()
}

fn simple_keymap() -> [Action<()>; KEYS] {
    [
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::B),
        Action::Key(KeyCode::C),
        Action::Key(KeyCode::D),
        Action::Key(KeyCode::E),
        Action::Key(KeyCode::F),
        Action::Key(KeyCode::G),
        Action::Key(KeyCode::H),
        Action::Key(KeyCode::I),
        Action::Key(KeyCode::J),
        Action::Key(KeyCode::K),
        Action::Key(KeyCode::Backspace),
    ]
}

// ── Core telemetry ───────────────────────────────────────────────────────────

#[test]
fn press_count_tracks_all_keys() {
    let mut proc = make_keyboard(&simple_keymap());
    for col in 0..6u8 {
        collect(
            &mut proc,
            press(pos(0, col), u64::from(col) * 100),
            u64::from(col) * 100,
        );
        collect(
            &mut proc,
            release(pos(0, col), u64::from(col) * 100 + 50),
            u64::from(col) * 100 + 50,
        );
    }
    let t = proc.telemetry().unwrap();
    for col in 0..6 {
        assert_eq!(
            t.core.press_counts()[col],
            1,
            "key {col} should have 1 press"
        );
    }
    for col in 6..KEYS {
        assert_eq!(
            t.core.press_counts()[col],
            0,
            "key {col} should have 0 presses"
        );
    }
}

#[test]
fn repeated_key_increments_count() {
    let mut proc = make_keyboard(&simple_keymap());
    for i in 0..10u64 {
        collect(&mut proc, press(pos(0, 0), i * 200), i * 200);
        collect(&mut proc, release(pos(0, 0), i * 200 + 50), i * 200 + 50);
    }
    assert_eq!(proc.telemetry().unwrap().core.press_counts()[0], 10);
}

#[test]
fn hold_time_ema_tracks_duration() {
    let mut proc = make_keyboard(&simple_keymap());
    for i in 0..20u64 {
        collect(&mut proc, press(pos(0, 0), i * 200), i * 200);
        collect(&mut proc, release(pos(0, 0), i * 200 + 80), i * 200 + 80);
    }
    let ema = proc.telemetry().unwrap().core.hold_time_emas()[0];
    assert!(
        ema > 60 && ema < 90,
        "hold time EMA should converge near 80ms, got {ema}"
    );
}

#[test]
fn hold_time_variance_nonzero_with_jitter() {
    let mut proc = make_keyboard(&simple_keymap());
    let hold_times = [50, 100, 50, 100, 50, 100, 50, 100];
    for (i, &hold) in hold_times.iter().enumerate() {
        let t = (i as u64) * 200;
        collect(&mut proc, press(pos(0, 0), t), t);
        collect(&mut proc, release(pos(0, 0), t + hold), t + hold);
    }
    let var = proc.telemetry().unwrap().core.hold_time_vars()[0];
    assert!(
        var > 0,
        "variance should be non-zero with jittery hold times, got {var}"
    );
}

#[test]
fn wpm_increases_with_typing_speed() {
    let mut proc = make_keyboard(&simple_keymap());
    for i in 0..30u64 {
        let col = (i % 6) as u8;
        collect(&mut proc, press(pos(0, col), i * 50), i * 50);
        collect(&mut proc, release(pos(0, col), i * 50 + 20), i * 50 + 20);
    }
    let wpm = proc.telemetry().unwrap().core.wpm();
    assert!(wpm > 100, "fast typing should produce high WPM, got {wpm}");
}

#[test]
fn wpm_lower_with_slow_typing() {
    let mut proc = make_keyboard(&simple_keymap());
    for i in 0..20u64 {
        let col = (i % 6) as u8;
        collect(&mut proc, press(pos(0, col), i * 500), i * 500);
        collect(&mut proc, release(pos(0, col), i * 500 + 50), i * 500 + 50);
    }
    let wpm = proc.telemetry().unwrap().core.wpm();
    assert!(wpm < 50, "slow typing should produce low WPM, got {wpm}");
}

#[test]
fn backspace_records_error_on_previous_key() {
    let mut proc = make_keyboard(&simple_keymap());

    collect(&mut proc, press(pos(0, 2), 0), 0);
    collect(&mut proc, release(pos(0, 2), 50), 50);

    collect(&mut proc, press(pos(1, 5), 100), 100);
    collect(&mut proc, release(pos(1, 5), 150), 150);

    let t = proc.telemetry().unwrap();
    assert_eq!(t.core.error_counts()[2], 1, "key C should have 1 error");
    assert_eq!(
        t.core.error_counts()[11],
        0,
        "backspace itself should have 0 errors"
    );
}

#[test]
fn multiple_errors_on_different_keys() {
    let mut proc = make_keyboard(&simple_keymap());

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, release(pos(0, 0), 50), 50);
    collect(&mut proc, press(pos(1, 5), 100), 100);
    collect(&mut proc, release(pos(1, 5), 150), 150);

    collect(&mut proc, press(pos(0, 1), 200), 200);
    collect(&mut proc, release(pos(0, 1), 250), 250);
    collect(&mut proc, press(pos(1, 5), 300), 300);
    collect(&mut proc, release(pos(1, 5), 350), 350);

    assert_eq!(
        proc.telemetry().unwrap().core.error_counts()[0],
        1,
        "key A error"
    );
    assert_eq!(
        proc.telemetry().unwrap().core.error_counts()[1],
        1,
        "key B error"
    );
}

#[test]
fn bigram_tracking_records_key_pairs() {
    let mut proc = make_keyboard(&simple_keymap());

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, press(pos(0, 1), 80), 80);
    collect(&mut proc, release(pos(0, 0), 100), 100);
    collect(&mut proc, release(pos(0, 1), 160), 160);

    let bigrams = proc.telemetry().unwrap().core.bigrams();
    assert!(!bigrams.is_empty(), "should have at least one bigram");
    assert_eq!(bigrams[0].from, 0);
    assert_eq!(bigrams[0].to, 1);
    assert!(bigrams[0].pp_ema > 0);
}

#[test]
fn snapshot_reflects_session_state() {
    let mut proc = make_keyboard(&simple_keymap());

    for i in 0..15u64 {
        let col = (i % 6) as u8;
        collect(&mut proc, press(pos(0, col), i * 100), i * 100);
        collect(&mut proc, release(pos(0, col), i * 100 + 50), i * 100 + 50);
    }

    collect(&mut proc, press(pos(1, 5), 1600), 1600);
    collect(&mut proc, release(pos(1, 5), 1650), 1650);

    let snap = proc.telemetry().unwrap().core.snapshot();
    assert_eq!(snap.total_presses, 16);
    assert!(snap.wpm > 0);
    assert!(snap.avg_hold_ms > 0);
    assert!(snap.error_rate > 0);
}

#[test]
fn delta_presses_reset_independently() {
    let mut proc = make_keyboard(&simple_keymap());

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, release(pos(0, 0), 50), 50);
    assert_eq!(proc.telemetry().unwrap().core.snapshot().delta_presses, 1);

    // Simulate snapshot send
    if let Some(t) = proc.telemetry_mut() {
        t.core.reset_deltas();
    }

    assert_eq!(proc.telemetry().unwrap().core.snapshot().delta_presses, 0);
    assert_eq!(proc.telemetry().unwrap().core.snapshot().total_presses, 1);
}

// ── Steno telemetry ──────────────────────────────────────────────────────────

const STENO_ROWS: usize = 1;
const STENO_COLS: usize = 3;
const STENO_LAYERS: usize = 1;
const STENO_KEYS: usize = 3;

fn make_steno_processor(
    dict: &'static [u8],
) -> KeyboardProcessor<STENO_ROWS, STENO_COLS, STENO_LAYERS, STENO_KEYS> {
    let mut layers = HVec::<Layer<()>, STENO_LAYERS>::new();
    layers
        .push(Layer::new(&[
            Action::Steno(0),
            Action::Steno(1),
            Action::Steno(2),
        ]))
        .ok();
    KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
        .with_steno(tecla::devices::steno::StenoEngine::new(dict))
        .with_telemetry()
}

fn steno_collect(
    proc: &mut KeyboardProcessor<STENO_ROWS, STENO_COLS, STENO_LAYERS, STENO_KEYS>,
    event: KeyEvent,
    now: u64,
) -> Vec<HidReport> {
    let mut out = Vec::new();
    proc.process_event(event, now, &mut |r| out.push(r));
    out
}

fn compact_text_payload(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(OP_TEXT);
    if text.len() < 0xFF {
        bytes.push(text.len() as u8);
    } else {
        bytes.push(0xFF);
        bytes.push((text.len() & 0xff) as u8);
        bytes.push(((text.len() >> 8) & 0xff) as u8);
    }
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

fn leak_dict(entries: Vec<(Vec<u64>, Vec<u8>)>) -> &'static [u8] {
    Box::leak(compile_steno_dictionary(&entries).into_boxed_slice())
}

#[test]
fn steno_telemetry_records_dictionary_hit() {
    let dict = leak_dict(vec![(vec![1u64], compact_text_payload("the"))]);
    let mut proc = make_steno_processor(dict);

    steno_collect(&mut proc, press(pos(0, 0), 0), 0);
    steno_collect(&mut proc, release(pos(0, 0), 50), 50);

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert_eq!(snap.hit_rate, 255, "single hit should be 100%");
    assert_eq!(snap.delta_strokes, 1);
}

#[test]
fn steno_telemetry_records_dictionary_miss() {
    let dict = leak_dict(vec![(vec![1u64], compact_text_payload("the"))]);
    let mut proc = make_steno_processor(dict);

    steno_collect(&mut proc, press(pos(0, 1), 0), 0);
    steno_collect(&mut proc, release(pos(0, 1), 50), 50);

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert_eq!(snap.hit_rate, 0, "unmatched stroke should be 0% hit rate");
}

#[test]
fn steno_telemetry_mixed_hit_miss() {
    let dict = leak_dict(vec![(vec![1u64], compact_text_payload("the"))]);
    let mut proc = make_steno_processor(dict);

    steno_collect(&mut proc, press(pos(0, 0), 0), 0);
    steno_collect(&mut proc, release(pos(0, 0), 50), 50);

    steno_collect(&mut proc, press(pos(0, 1), 100), 100);
    steno_collect(&mut proc, release(pos(0, 1), 150), 150);

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert!(
        snap.hit_rate > 100 && snap.hit_rate < 200,
        "50% hit rate, got {}",
        snap.hit_rate
    );
}

#[test]
fn steno_telemetry_records_undo() {
    let undo_payload = vec![OP_UNDO_LAST_TRANSLATION];
    let dict = leak_dict(vec![
        (vec![1u64], compact_text_payload("the")),
        (vec![1u64 << 1], undo_payload),
    ]);
    let mut proc = make_steno_processor(dict);

    steno_collect(&mut proc, press(pos(0, 0), 0), 0);
    steno_collect(&mut proc, release(pos(0, 0), 50), 50);

    steno_collect(&mut proc, press(pos(0, 1), 100), 100);
    steno_collect(&mut proc, release(pos(0, 1), 150), 150);

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert_eq!(snap.undos, 1);
}

// ── MIDI telemetry ───────────────────────────────────────────────────────────

fn make_midi_keyboard() -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    let keys: [Action<()>; KEYS] = [
        Action::Midi(MidiAction::NoteOn {
            channel: 0,
            note: 60,
            velocity: 80,
        }),
        Action::Midi(MidiAction::NoteOn {
            channel: 0,
            note: 62,
            velocity: 100,
        }),
        Action::Midi(MidiAction::NoteOn {
            channel: 0,
            note: 64,
            velocity: 120,
        }),
        Action::Midi(MidiAction::Cc {
            channel: 0,
            controller: 1,
            value: 64,
        }),
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::B),
        Action::Key(KeyCode::C),
        Action::Key(KeyCode::D),
        Action::Key(KeyCode::E),
        Action::Key(KeyCode::F),
        Action::Key(KeyCode::G),
        Action::Key(KeyCode::H),
    ];
    make_keyboard(&keys)
}

#[test]
fn midi_telemetry_tracks_velocity_range() {
    let mut proc = make_midi_keyboard();

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, press(pos(0, 1), 10), 10);
    collect(&mut proc, press(pos(0, 2), 20), 20);

    let snap = proc.telemetry().unwrap().midi.snapshot();
    assert_eq!(snap.min_velocity, 80);
    assert_eq!(snap.max_velocity, 120);
    assert_eq!(snap.avg_velocity, 100);
}

#[test]
fn midi_telemetry_tracks_polyphony() {
    let mut proc = make_midi_keyboard();

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, press(pos(0, 1), 10), 10);
    collect(&mut proc, press(pos(0, 2), 20), 20);

    assert_eq!(proc.telemetry().unwrap().midi.snapshot().max_polyphony, 3);

    collect(&mut proc, release(pos(0, 0), 30), 30);
    collect(&mut proc, release(pos(0, 1), 40), 40);

    assert_eq!(
        proc.telemetry().unwrap().midi.snapshot().max_polyphony,
        3,
        "peak polyphony should persist"
    );
}

#[test]
fn midi_telemetry_tracks_cc() {
    let mut proc = make_midi_keyboard();

    collect(&mut proc, press(pos(0, 3), 0), 0);
    collect(&mut proc, release(pos(0, 3), 50), 50);

    assert_eq!(proc.telemetry().unwrap().midi.snapshot().cc_count, 1);
}

// ── Gamepad telemetry ────────────────────────────────────────────────────────

fn make_gamepad_keyboard() -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    let keys: [Action<()>; KEYS] = [
        Action::Gamepad(GamepadAction::ButtonPress(0)),
        Action::Gamepad(GamepadAction::ButtonPress(1)),
        Action::Gamepad(GamepadAction::ButtonPress(2)),
        Action::Gamepad(GamepadAction::AxisSet {
            axis: 0,
            value: 100,
        }),
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::B),
        Action::Key(KeyCode::C),
        Action::Key(KeyCode::D),
        Action::Key(KeyCode::E),
        Action::Key(KeyCode::F),
        Action::Key(KeyCode::G),
        Action::Key(KeyCode::H),
    ];
    make_keyboard(&keys)
}

#[test]
fn gamepad_telemetry_tracks_buttons() {
    let mut proc = make_gamepad_keyboard();

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, press(pos(0, 1), 10), 10);

    let snap = proc.telemetry().unwrap().gamepad.snapshot();
    assert_eq!(snap.max_simultaneous, 2);
    assert_eq!(snap.total_presses, 2);
}

#[test]
fn gamepad_telemetry_tracks_axis_range() {
    let mut proc = make_gamepad_keyboard();

    collect(&mut proc, press(pos(0, 3), 0), 0);

    let snap = proc.telemetry().unwrap().gamepad.snapshot();
    assert_eq!(snap.axis_max[0], 100);
}

#[test]
fn gamepad_button_release_tracked() {
    let mut proc = make_gamepad_keyboard();

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, press(pos(0, 1), 10), 10);
    collect(&mut proc, release(pos(0, 0), 20), 20);

    let snap = proc.telemetry().unwrap().gamepad.snapshot();
    assert_eq!(snap.max_simultaneous, 2, "peak should be preserved");
}

// ── Accessibility telemetry ──────────────────────────────────────────────────

fn make_accessible_keyboard(
    config: tecla::accessibility::AccessibilityConfig,
) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    make_keyboard(&simple_keymap()).with_accessibility(config)
}

#[test]
fn accessibility_telemetry_bounce_rejection() {
    let config = tecla::accessibility::AccessibilityConfig::default().with_bounce_keys(50);
    let mut proc = make_accessible_keyboard(config).with_telemetry();

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, release(pos(0, 0), 10), 10);
    collect(&mut proc, press(pos(0, 0), 20), 20);

    let snap = proc.telemetry().unwrap().accessibility.snapshot();
    assert!(
        snap.bounce_reject_rate > 0,
        "should have recorded a bounce rejection"
    );
}

#[test]
fn accessibility_telemetry_slow_key_acceptance() {
    let config = tecla::accessibility::AccessibilityConfig::default().with_slow_keys(100);
    let mut proc = make_accessible_keyboard(config).with_telemetry();

    collect(&mut proc, press(pos(0, 0), 0), 0);

    let mut reports = Vec::new();
    proc.tick(100, &mut |r| reports.push(r));

    let snap = proc.telemetry().unwrap().accessibility.snapshot();
    assert!(
        snap.slow_reject_rate < 255,
        "accepted slow key should reduce rejection rate"
    );
}

#[test]
fn accessibility_telemetry_sticky_key_usage() {
    let config = tecla::accessibility::AccessibilityConfig::default().with_sticky_keys();
    let mut keys = simple_keymap();
    keys[0] = Action::Modifier(ModifierKey::LSHIFT);
    let mut layers = HVec::<Layer<()>, LAYERS>::new();
    layers.push(Layer::new(&keys)).ok();
    layers.push(Layer::new(&keys)).ok();
    let mut proc = KeyboardProcessor::<ROWS, COLS, LAYERS, KEYS>::new(
        LayerStack::new(layers),
        TapHoldConfig::default(),
    )
    .with_accessibility(config)
    .with_telemetry();

    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, release(pos(0, 0), 50), 50);

    assert_eq!(
        proc.telemetry()
            .unwrap()
            .accessibility
            .snapshot()
            .sticky_uses,
        1
    );
}

// ── Cross-modality: telemetry + normal operation ─────────────────────────────

#[test]
fn telemetry_does_not_alter_hid_output() {
    let keymap = simple_keymap();
    let mut with_telemetry = make_keyboard(&keymap);

    let mut layers = HVec::<Layer<()>, LAYERS>::new();
    layers.push(Layer::new(&keymap)).ok();
    layers.push(Layer::new(&keymap)).ok();
    let mut without_telemetry = KeyboardProcessor::<ROWS, COLS, LAYERS, KEYS>::new(
        LayerStack::new(layers),
        TapHoldConfig::default(),
    );

    let events = [
        press(pos(0, 0), 0),
        press(pos(0, 1), 10),
        release(pos(0, 0), 50),
        release(pos(0, 1), 60),
    ];

    for event in &events {
        let r1 = collect(&mut with_telemetry, *event, event.timestamp_ms);
        let r2 = collect(&mut without_telemetry, *event, event.timestamp_ms);
        assert_eq!(r1.len(), r2.len(), "report count should match");
        for (a, b) in r1.iter().zip(r2.iter()) {
            assert_eq!(
                a, b,
                "reports should be identical with and without telemetry"
            );
        }
    }
}

#[test]
fn telemetry_disabled_collects_nothing() {
    let mut proc = make_keyboard(&simple_keymap());

    if let Some(t) = proc.telemetry_mut() {
        t.core.set_enabled(false);
    }

    for i in 0..10u64 {
        collect(&mut proc, press(pos(0, 0), i * 100), i * 100);
        collect(&mut proc, release(pos(0, 0), i * 100 + 50), i * 100 + 50);
    }

    assert_eq!(proc.telemetry().unwrap().core.press_counts()[0], 0);
}

#[test]
fn telemetry_survives_full_session() {
    let mut proc = make_keyboard(&simple_keymap());

    for i in 0..500u64 {
        let col = (i % 6) as u8;
        let row = ((i / 6) % 2) as u8;
        collect(&mut proc, press(pos(row, col), i * 50), i * 50);
        collect(&mut proc, release(pos(row, col), i * 50 + 25), i * 50 + 25);
    }

    let t = proc.telemetry().unwrap();
    let total: u32 = t.core.press_counts().iter().sum();
    assert_eq!(total, 500);

    let snap = t.core.snapshot();
    assert_eq!(snap.total_presses, 500);
    assert!(snap.wpm > 0);
    assert!(snap.avg_hold_ms > 0);
    assert!(snap.avg_pp_ms > 0);

    for &ema in t.core.hold_time_emas() {
        if ema > 0 {
            assert!(
                ema > 15 && ema < 35,
                "hold EMAs should converge near 25ms, got {ema}"
            );
        }
    }
}

// ── Lapwing-specific steno tests ─────────────────────────────────────────────

const LAPWING_ROWS: usize = 1;
const LAPWING_COLS: usize = 26;
const LAPWING_LAYERS: usize = 1;
const LAPWING_KEYS: usize = 26;

fn make_lapwing_processor(
    dict: &'static [u8],
) -> KeyboardProcessor<LAPWING_ROWS, LAPWING_COLS, LAPWING_LAYERS, LAPWING_KEYS> {
    let mut actions = [Action::NoOp; LAPWING_KEYS];
    for i in 0..26u8 {
        actions[usize::from(i)] = Action::Steno(i);
    }
    let mut layers = HVec::<Layer<()>, LAPWING_LAYERS>::new();
    layers.push(Layer::new(&actions)).ok();
    KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
        .with_steno(tecla::devices::steno::StenoEngine::new(dict))
        .with_telemetry()
}

fn lapwing_collect(
    proc: &mut KeyboardProcessor<LAPWING_ROWS, LAPWING_COLS, LAPWING_LAYERS, LAPWING_KEYS>,
    event: KeyEvent,
    now: u64,
) -> Vec<HidReport> {
    let mut out = Vec::new();
    proc.process_event(event, now, &mut |r| out.push(r));
    out
}

fn lapwing_chord(
    proc: &mut KeyboardProcessor<LAPWING_ROWS, LAPWING_COLS, LAPWING_LAYERS, LAPWING_KEYS>,
    keys: &[u8],
    start_ms: u64,
) -> Vec<HidReport> {
    let mut reports = Vec::new();
    for (i, &key) in keys.iter().enumerate() {
        reports.extend(lapwing_collect(
            proc,
            press(pos(0, key), start_ms + i as u64),
            start_ms + i as u64,
        ));
    }
    for (i, &key) in keys.iter().enumerate() {
        reports.extend(lapwing_collect(
            proc,
            release(pos(0, key), start_ms + 50 + i as u64),
            start_ms + 50 + i as u64,
        ));
    }
    reports
}

fn compile_lapwing() -> &'static [u8] {
    use std::path::Path;
    let dict_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.vendor/plover-lapwing-aio/plover_lapwing/dictionaries");
    let build = tecla_config::steno_dictionary::compile_lapwing_dictionary_dir(&dict_dir)
        .expect("failed to compile Lapwing dictionary");
    assert!(
        build.stats.entries_kept > 100_000,
        "expected >100K entries, got {}",
        build.stats.entries_kept
    );
    Box::leak(build.bytes.into_boxed_slice())
}

#[test]
fn lapwing_the_resolves() {
    let dict = compile_lapwing();
    let mut proc = make_lapwing_processor(dict);

    let reports = lapwing_chord(&mut proc, &[1], 0);

    assert!(
        !reports.is_empty(),
        "S should produce output (maps to 'the' or similar)"
    );
    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert!(
        snap.hit_rate > 0 || snap.delta_strokes > 0,
        "should have recorded a stroke"
    );
}

#[test]
fn lapwing_hit_rate_tracks_over_multiple_chords() {
    let dict = compile_lapwing();
    let mut proc = make_lapwing_processor(dict);

    lapwing_chord(&mut proc, &[1], 0);
    lapwing_chord(&mut proc, &[1, 2], 100);
    lapwing_chord(&mut proc, &[1, 2, 3], 200);

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert!(
        snap.delta_strokes >= 3,
        "should have at least 3 strokes, got {}",
        snap.delta_strokes
    );
}

#[test]
fn lapwing_nonsense_chord_records_miss() {
    let dict = compile_lapwing();
    let mut proc = make_lapwing_processor(dict);

    lapwing_chord(
        &mut proc,
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13],
        0,
    );

    let t = proc.telemetry().unwrap();
    let snap = t.steno.snapshot();
    assert_eq!(
        snap.hit_rate, 0,
        "all-keys-pressed nonsense chord should miss"
    );
}

#[test]
fn lapwing_spm_nonzero_with_rapid_chords() {
    let dict = compile_lapwing();
    let mut proc = make_lapwing_processor(dict);

    for i in 0..10u64 {
        lapwing_chord(&mut proc, &[1], i * 200);
    }

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert!(
        snap.spm > 0,
        "rapid chording should produce non-zero SPM, got {}",
        snap.spm
    );
}

#[test]
fn lapwing_dictionary_size_under_2mb() {
    let dict = compile_lapwing();
    assert!(
        dict.len() < 2_000_000,
        "compiled Lapwing dictionary should be under 2MB, got {} bytes",
        dict.len()
    );
}

// ── Multi-stroke steno telemetry ─────────────────────────────────────────────

#[test]
fn steno_multi_stroke_tracking() {
    let dict = leak_dict(vec![(vec![1u64, 2u64], compact_text_payload("hello"))]);
    let mut proc = make_steno_processor(dict);

    steno_collect(&mut proc, press(pos(0, 0), 0), 0);
    steno_collect(&mut proc, release(pos(0, 0), 50), 50);

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert_eq!(
        snap.completion_rate, 0,
        "first stroke is pending, no completion yet"
    );

    steno_collect(&mut proc, press(pos(0, 1), 100), 100);
    steno_collect(&mut proc, release(pos(0, 1), 150), 150);

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert!(
        snap.completion_rate > 0,
        "two-stroke sequence should complete"
    );
    assert!(snap.hit_rate > 0, "completed sequence should be a hit");
}

#[test]
fn steno_multi_stroke_abandoned() {
    let dict = leak_dict(vec![(vec![1u64, 1u64], compact_text_payload("hello"))]);
    let mut proc = make_steno_processor(dict);

    steno_collect(&mut proc, press(pos(0, 0), 0), 0);
    steno_collect(&mut proc, release(pos(0, 0), 50), 50);

    steno_collect(&mut proc, press(pos(0, 1), 100), 100);
    steno_collect(&mut proc, release(pos(0, 1), 150), 150);

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert_eq!(
        snap.completion_rate, 0,
        "wrong second stroke should not complete"
    );
}

#[test]
fn steno_spm_with_timed_strokes() {
    let dict = leak_dict(vec![
        (vec![1u64], compact_text_payload("a")),
        (vec![1u64 << 1], compact_text_payload("b")),
    ]);
    let mut proc = make_steno_processor(dict);

    for i in 0..10u64 {
        let key = (i % 2) as u8;
        steno_collect(&mut proc, press(pos(0, key), i * 500), i * 500);
        steno_collect(&mut proc, release(pos(0, key), i * 500 + 50), i * 500 + 50);
    }

    let snap = proc.telemetry().unwrap().steno.snapshot();
    assert!(snap.spm > 0, "should have non-zero SPM, got {}", snap.spm);
    assert!(
        snap.spm < 200,
        "SPM should be reasonable at 2 strokes/sec = 120 SPM, got {}",
        snap.spm
    );
}

// ── Accessibility fatigue tracking ───────────────────────────────────────────

#[test]
fn fatigue_trend_detected_via_telemetry() {
    let mut proc = make_keyboard(&simple_keymap());

    for i in 0..4u64 {
        collect(&mut proc, press(pos(0, 0), i * 200), i * 200);
        collect(&mut proc, release(pos(0, 0), i * 200 + 50), i * 200 + 50);
    }

    for i in 4..8u64 {
        collect(&mut proc, press(pos(0, 0), i * 200), i * 200);
        collect(&mut proc, release(pos(0, 0), i * 200 + 100), i * 200 + 100);
    }

    let trend = proc
        .telemetry()
        .unwrap()
        .accessibility
        .snapshot()
        .hold_time_trend;
    assert!(
        trend > 0,
        "hold times increasing should show positive fatigue trend, got {trend}"
    );
}

// ── Pointing telemetry via processor (key-triggered mouse actions) ────────────

#[test]
fn mouse_move_records_pointing_telemetry() {
    use tecla::action::MouseAction;
    let mut keys = simple_keymap();
    keys[0] = Action::Mouse(MouseAction::Move { dx: 10, dy: -5 });
    let mut proc = make_keyboard(&keys);
    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, release(pos(0, 0), 50), 50);

    let snap = proc.telemetry().unwrap().pointing.snapshot();
    assert!(snap.avg_velocity > 0);
}

#[test]
fn mouse_click_records_pointing_telemetry() {
    use tecla::action::MouseAction;
    let mut keys = simple_keymap();
    keys[0] = Action::Mouse(MouseAction::Click(1));
    let mut proc = make_keyboard(&keys);
    collect(&mut proc, press(pos(0, 0), 0), 0);

    let snap = proc.telemetry().unwrap().pointing.snapshot();
    assert_eq!(snap.total_clicks, 1);
    assert_eq!(snap.delta_clicks, 1);
}

#[test]
fn mouse_scroll_records_pointing_telemetry() {
    use tecla::action::MouseAction;
    let mut keys = simple_keymap();
    keys[0] = Action::Mouse(MouseAction::Scroll(3));
    let mut proc = make_keyboard(&keys);
    collect(&mut proc, press(pos(0, 0), 0), 0);

    let snap = proc.telemetry().unwrap().pointing.snapshot();
    assert_eq!(snap.scroll_count, 1);
}

// ── Telemetry transport round-trip ───────────────────────────────────────────

#[test]
fn collect_frame_captures_all_modalities() {
    use tecla::telemetry::transport::collect_frame;

    let mut proc = make_keyboard(&simple_keymap());
    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, release(pos(0, 0), 50), 50);

    let frame = collect_frame(proc.telemetry_mut().unwrap());
    assert!(frame.core.unwrap().total_presses > 0);

    let frame2 = collect_frame(proc.telemetry_mut().unwrap());
    assert_eq!(frame2.core.unwrap().delta_presses, 0);
}

#[test]
fn telemetry_frame_serialize_deserialize() {
    use tecla::telemetry::transport::{collect_frame, TelemetryFrame};

    let mut proc = make_keyboard(&simple_keymap());
    for i in 0..5u64 {
        collect(
            &mut proc,
            press(pos(0, i as u8 % COLS as u8), i * 100),
            i * 100,
        );
        collect(
            &mut proc,
            release(pos(0, i as u8 % COLS as u8), i * 100 + 50),
            i * 100 + 50,
        );
    }

    let frame = collect_frame(proc.telemetry_mut().unwrap());
    let mut buf = HVec::<u8, 128>::new();
    frame.serialize(&mut buf);
    let decoded = TelemetryFrame::deserialize(&buf).unwrap();
    assert_eq!(
        decoded.core.unwrap().total_presses,
        frame.core.unwrap().total_presses
    );
}
