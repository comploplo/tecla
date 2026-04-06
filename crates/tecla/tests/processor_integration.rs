#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unused_result_ok, reason = "test code")]
#![expect(clippy::expect_used, reason = "test code")]
#![expect(clippy::panic, reason = "test assertions")]

use heapless::Vec as HVec;
use tecla::action::{Action, KeyCode, LayerOp, ModifierKey};
use tecla::encoder::encoder_key_pos;
use tecla::event::{EncoderDirection, KeyEvent, KeyPos};
use tecla::keyboard::KeyboardProcessor;
use tecla::keymap::{Layer, LayerStack};
use tecla::report::HidReport;
use tecla::split::SplitConfig;
use tecla::tapping::TapHoldConfig;

const ROWS: usize = 2;
const COLS: usize = 6;
const LAYERS: usize = 2;
const KEYS: usize = 12;

fn pos(row: u8, col: u8) -> KeyPos {
    KeyPos::new(row, col)
}

fn press(pos: KeyPos, ms: u64) -> KeyEvent {
    KeyEvent::press(pos, ms)
}

fn release(pos: KeyPos, ms: u64) -> KeyEvent {
    KeyEvent::release(pos, ms)
}

fn make_two_layer(
    base: &[Action<()>],
    overlay: &[Action<()>],
) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    let mut layers = HVec::<Layer<()>, LAYERS>::new();
    layers.push(Layer::new(base)).ok();
    layers.push(Layer::new(overlay)).ok();
    KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
}

fn collect(
    proc: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>,
    event: KeyEvent,
    now: u64,
) -> std::vec::Vec<HidReport> {
    let mut out = std::vec::Vec::new();
    proc.process_event(event, now, &mut |r| out.push(r));
    out
}

fn first(reports: &[HidReport]) -> &HidReport {
    reports.first().expect("expected at least one report")
}

fn kb_keycodes(r: &HidReport) -> [u8; 6] {
    match r {
        HidReport::Keyboard(kr) => kr.keycodes,
        _ => [0; 6],
    }
}

fn kb_mods(r: &HidReport) -> u8 {
    match r {
        HidReport::Keyboard(kr) => kr.modifiers,
        _ => 0,
    }
}

fn tick_collect(
    proc: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>,
    now: u64,
) -> std::vec::Vec<HidReport> {
    let mut out = std::vec::Vec::new();
    proc.tick(now, &mut |r| out.push(r));
    out
}

// ── Basic key press round-trip ──────────────────────────────────────────────

#[test]
fn basic_key_press_produces_correct_hid_report() {
    let mut proc = make_two_layer(
        &[
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[Action::Transparent; 12],
    );
    let reports = collect(&mut proc, press(pos(0, 0), 0), 0);
    assert_eq!(reports.len(), 1);
    assert!(kb_keycodes(first(&reports)).contains(&KeyCode::A.usage_id()));
    assert_eq!(kb_mods(first(&reports)), 0);

    let reports = collect(&mut proc, release(pos(0, 0), 10), 10);
    assert_eq!(reports.len(), 1);
    assert!(!kb_keycodes(first(&reports)).contains(&KeyCode::A.usage_id()));
}

// ── Modifier combo ──────────────────────────────────────────────────────────

#[test]
fn modifier_plus_key_combo() {
    let mut proc = make_two_layer(
        &[
            Action::Modifier(ModifierKey::LCTRL),
            Action::Key(KeyCode::C),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[Action::Transparent; 12],
    );
    collect(&mut proc, press(pos(0, 0), 0), 0);
    let reports = collect(&mut proc, press(pos(0, 1), 5), 5);
    assert_eq!(
        kb_mods(first(&reports)) & ModifierKey::LCTRL.bits(),
        ModifierKey::LCTRL.bits()
    );
    assert!(kb_keycodes(first(&reports)).contains(&KeyCode::C.usage_id()));
}

// ── Layer tap: tap path ─────────────────────────────────────────────────────

#[test]
fn layer_tap_quick_tap_emits_keycode() {
    let mut proc = make_two_layer(
        &[
            Action::LayerTap {
                layer: 1,
                tap: KeyCode::Space,
            },
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Key(KeyCode::N1),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    );
    collect(&mut proc, press(pos(0, 0), 0), 0);
    let reports = collect(&mut proc, release(pos(0, 0), 100), 100);
    let any_space = reports
        .iter()
        .any(|r| kb_keycodes(r).contains(&KeyCode::Space.usage_id()));
    assert!(any_space, "tap should emit Space keycode");
}

// ── Layer tap: hold path ────────────────────────────────────────────────────

#[test]
fn layer_tap_hold_activates_layer_then_deactivates() {
    let mut proc = make_two_layer(
        &[
            Action::LayerTap {
                layer: 1,
                tap: KeyCode::Space,
            },
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[
            Action::Transparent,
            Action::Key(KeyCode::N1),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
    );
    collect(&mut proc, press(pos(0, 0), 0), 0);
    tick_collect(&mut proc, 250);
    let reports = collect(&mut proc, press(pos(0, 1), 260), 260);
    assert!(
        kb_keycodes(first(&reports)).contains(&KeyCode::N1.usage_id()),
        "hold should activate layer 1"
    );

    collect(&mut proc, release(pos(0, 1), 270), 270);
    collect(&mut proc, release(pos(0, 0), 280), 280);
    let reports = collect(&mut proc, press(pos(0, 1), 300), 300);
    assert!(
        kb_keycodes(first(&reports)).contains(&KeyCode::A.usage_id()),
        "after release, layer 1 should be off"
    );
}

// ── Mod tap ─────────────────────────────────────────────────────────────────

#[test]
fn mod_tap_hold_activates_modifier() {
    let mut proc = make_two_layer(
        &[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::Escape,
            },
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[Action::Transparent; 12],
    );
    collect(&mut proc, press(pos(0, 0), 0), 0);
    let reports = tick_collect(&mut proc, 250);
    assert_eq!(
        kb_mods(first(&reports)) & ModifierKey::LSHIFT.bits(),
        ModifierKey::LSHIFT.bits()
    );

    let reports = collect(&mut proc, release(pos(0, 0), 260), 260);
    let cleared = reports.iter().any(|r| kb_mods(r) == 0);
    assert!(cleared, "releasing mod-tap hold should clear modifier");
}

// ── One-shot mod ────────────────────────────────────────────────────────────

#[test]
fn oneshot_mod_applies_once_then_clears() {
    let mut proc = make_two_layer(
        &[
            Action::OneShotMod(ModifierKey::LSHIFT),
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[Action::Transparent; 12],
    );
    collect(&mut proc, press(pos(0, 0), 0), 0);
    let reports = collect(&mut proc, press(pos(0, 1), 10), 10);
    assert_ne!(
        kb_mods(first(&reports)) & ModifierKey::LSHIFT.bits(),
        0,
        "OSM should apply to first key"
    );

    collect(&mut proc, release(pos(0, 1), 20), 20);
    let reports = collect(&mut proc, press(pos(0, 2), 30), 30);
    assert_eq!(
        kb_mods(first(&reports)) & ModifierKey::LSHIFT.bits(),
        0,
        "OSM should be cleared after first key"
    );
}

// ── Caps word ───────────────────────────────────────────────────────────────

#[test]
fn caps_word_shifts_letters_not_digits_deactivates_on_space() {
    let mut proc = make_two_layer(
        &[
            Action::CapsWord,
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::N1),
            Action::Key(KeyCode::Space),
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[Action::Transparent; 12],
    );
    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, release(pos(0, 0), 5), 5);

    let reports = collect(&mut proc, press(pos(0, 1), 10), 10);
    assert_ne!(
        kb_mods(first(&reports)) & 0x02,
        0,
        "letter should be shifted"
    );
    collect(&mut proc, release(pos(0, 1), 15), 15);

    let reports = collect(&mut proc, press(pos(0, 2), 20), 20);
    assert_eq!(
        kb_mods(first(&reports)) & 0x02,
        0,
        "digit should not be shifted"
    );
    collect(&mut proc, release(pos(0, 2), 25), 25);

    collect(&mut proc, press(pos(0, 3), 30), 30);
    collect(&mut proc, release(pos(0, 3), 35), 35);

    let reports = collect(&mut proc, press(pos(0, 4), 40), 40);
    assert_eq!(
        kb_mods(first(&reports)) & 0x02,
        0,
        "after space, caps word should be off"
    );
}

// ── Repeat key ──────────────────────────────────────────────────────────────

#[test]
fn repeat_key_replays_last_keycode() {
    let mut proc = make_two_layer(
        &[
            Action::Key(KeyCode::A),
            Action::RepeatKey,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[Action::Transparent; 12],
    );
    collect(&mut proc, press(pos(0, 0), 0), 0);
    collect(&mut proc, release(pos(0, 0), 10), 10);
    let reports = collect(&mut proc, press(pos(0, 1), 20), 20);
    assert!(kb_keycodes(first(&reports)).contains(&KeyCode::A.usage_id()));

    let reports = collect(&mut proc, release(pos(0, 1), 30), 30);
    assert!(
        !kb_keycodes(first(&reports)).contains(&KeyCode::A.usage_id()),
        "release of repeat should release key"
    );
}

// ── Split keyboard: col_offset remapping ────────────────────────────────────

#[test]
fn split_secondary_col_offset_maps_to_correct_key() {
    let cfg = SplitConfig::secondary(3);

    let mut proc = make_two_layer(
        &[
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::E),
            Action::Key(KeyCode::F),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[Action::Transparent; 12],
    );

    let remote_pos = KeyPos::new(0, 0);
    let remapped = KeyPos::new(remote_pos.row, remote_pos.col + cfg.col_offset);

    let reports = collect(&mut proc, press(remapped, 0), 0);
    assert!(
        kb_keycodes(first(&reports)).contains(&KeyCode::D.usage_id()),
        "col 0 on secondary with offset 3 should map to key at col 3 (KeyCode::D)"
    );
}

#[test]
fn split_secondary_multiple_keys_remap_correctly() {
    let cfg = SplitConfig::secondary(3);
    let mut proc = make_two_layer(
        &[
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::E),
            Action::Key(KeyCode::F),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ],
        &[Action::Transparent; 12],
    );

    for local_col in 0u8..3 {
        let remapped = KeyPos::new(0, local_col + cfg.col_offset);
        let reports = collect(
            &mut proc,
            press(remapped, u64::from(local_col) * 10),
            u64::from(local_col) * 10,
        );
        let expected = match local_col {
            0 => KeyCode::D,
            1 => KeyCode::E,
            2 => KeyCode::F,
            _ => panic!("unexpected local_col value"),
        };
        assert!(
            kb_keycodes(first(&reports)).contains(&expected.usage_id()),
            "secondary col {local_col} should map to {expected:?}"
        );
    }
}

// ── Encoder through processor ───────────────────────────────────────────────

const ENC_ROWS: usize = 3;
const ENC_COLS: usize = 4;
const ENC_LAYERS: usize = 2;
const ENC_KEYS: usize = 14; // 3*4 matrix + 2 encoder virtual keys
#[expect(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "ENC_ROWS is 3, fits in u8"
)]
const ENC_ROWS_U8: u8 = ENC_ROWS as u8;

#[expect(
    clippy::indexing_slicing,
    reason = "test setup with known-valid encoder positions"
)]
fn make_encoder_processor() -> KeyboardProcessor<ENC_ROWS, ENC_COLS, ENC_LAYERS, ENC_KEYS> {
    let mut base_actions = [Action::NoOp; ENC_KEYS];
    base_actions[0] = Action::Layer(LayerOp::Momentary(1));
    let cw_pos = encoder_key_pos(0, EncoderDirection::Clockwise, ENC_ROWS_U8);
    let counter_cw = encoder_key_pos(0, EncoderDirection::CounterClockwise, ENC_ROWS_U8);
    base_actions[cw_pos.flat(ENC_COLS)] = Action::Key(KeyCode::Right);
    base_actions[counter_cw.flat(ENC_COLS)] = Action::Key(KeyCode::Left);

    let mut overlay_actions = [Action::Transparent; ENC_KEYS];
    overlay_actions[cw_pos.flat(ENC_COLS)] = Action::Key(KeyCode::PageDown);
    overlay_actions[counter_cw.flat(ENC_COLS)] = Action::Key(KeyCode::PageUp);

    let mut layers = HVec::<Layer<()>, ENC_LAYERS>::new();
    layers.push(Layer::new(&base_actions)).ok();
    layers.push(Layer::new(&overlay_actions)).ok();
    KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
}

fn enc_collect(
    proc: &mut KeyboardProcessor<ENC_ROWS, ENC_COLS, ENC_LAYERS, ENC_KEYS>,
    event: KeyEvent,
    now: u64,
) -> std::vec::Vec<HidReport> {
    let mut out = std::vec::Vec::new();
    proc.process_event(event, now, &mut |r| out.push(r));
    out
}

#[test]
fn encoder_cw_on_base_layer_emits_right() {
    let mut proc = make_encoder_processor();
    let cw_pos = encoder_key_pos(0, EncoderDirection::Clockwise, ENC_ROWS_U8);
    let reports = enc_collect(&mut proc, press(cw_pos, 0), 0);
    assert!(kb_keycodes(first(&reports)).contains(&KeyCode::Right.usage_id()));
}

#[test]
fn encoder_ccw_on_base_layer_emits_left() {
    let mut proc = make_encoder_processor();
    let counter_cw = encoder_key_pos(0, EncoderDirection::CounterClockwise, ENC_ROWS_U8);
    let reports = enc_collect(&mut proc, press(counter_cw, 0), 0);
    assert!(kb_keycodes(first(&reports)).contains(&KeyCode::Left.usage_id()));
}

#[test]
fn encoder_resolves_through_active_layer() {
    let mut proc = make_encoder_processor();
    enc_collect(&mut proc, press(pos(0, 0), 0), 0);

    let cw_pos = encoder_key_pos(0, EncoderDirection::Clockwise, ENC_ROWS_U8);
    let reports = enc_collect(&mut proc, press(cw_pos, 10), 10);
    assert!(
        kb_keycodes(first(&reports)).contains(&KeyCode::PageDown.usage_id()),
        "encoder CW with layer 1 active should emit PageDown"
    );
}

#[test]
#[expect(
    clippy::indexing_slicing,
    reason = "test setup with known-valid encoder positions"
)]
fn encoder_falls_back_to_base_when_overlay_transparent() {
    let mut base_actions = [Action::NoOp; ENC_KEYS];
    let cw_pos = encoder_key_pos(0, EncoderDirection::Clockwise, ENC_ROWS_U8);
    base_actions[cw_pos.flat(ENC_COLS)] = Action::Key(KeyCode::Up);
    base_actions[0] = Action::Layer(LayerOp::Momentary(1));

    let overlay_actions = [Action::Transparent; ENC_KEYS];

    let mut layers = HVec::<Layer<()>, ENC_LAYERS>::new();
    layers.push(Layer::new(&base_actions)).ok();
    layers.push(Layer::new(&overlay_actions)).ok();
    let mut proc: KeyboardProcessor<ENC_ROWS, ENC_COLS, ENC_LAYERS, ENC_KEYS> =
        KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default());

    enc_collect(&mut proc, press(pos(0, 0), 0), 0);
    let reports = enc_collect(&mut proc, press(cw_pos, 10), 10);
    assert!(
        kb_keycodes(first(&reports)).contains(&KeyCode::Up.usage_id()),
        "transparent overlay should fall through to base layer for encoder"
    );
}

// ── Tap-hold LT! tap and hold (mirrors HIL matrix test firmware) ───────────

const PLANCK_ROWS: usize = 4;
const PLANCK_COLS: usize = 12;
const PLANCK_LAYERS: usize = 2;
const PLANCK_KEYS: usize = 48;

#[expect(clippy::too_many_lines, reason = "exhaustive keymap definition")]
fn planck_processor() -> KeyboardProcessor<PLANCK_ROWS, PLANCK_COLS, PLANCK_LAYERS, PLANCK_KEYS> {
    use tecla::keymap::Layer as KLayer;
    let k = |kc| Action::<()>::Key(kc);
    let m = |mk| Action::<()>::Modifier(mk);
    let tr = Action::<()>::Transparent;
    let base: [Action<()>; 48] = [
        k(KeyCode::Tab),
        k(KeyCode::Q),
        k(KeyCode::W),
        k(KeyCode::E),
        k(KeyCode::R),
        k(KeyCode::T),
        k(KeyCode::Y),
        k(KeyCode::U),
        k(KeyCode::I),
        k(KeyCode::O),
        k(KeyCode::P),
        k(KeyCode::Backspace),
        k(KeyCode::Escape),
        k(KeyCode::A),
        k(KeyCode::S),
        k(KeyCode::D),
        k(KeyCode::F),
        k(KeyCode::G),
        k(KeyCode::H),
        k(KeyCode::J),
        k(KeyCode::K),
        k(KeyCode::L),
        k(KeyCode::Semicolon),
        k(KeyCode::Enter),
        m(ModifierKey::LSHIFT),
        k(KeyCode::Z),
        k(KeyCode::X),
        k(KeyCode::C),
        k(KeyCode::V),
        k(KeyCode::B),
        k(KeyCode::N),
        k(KeyCode::M),
        k(KeyCode::Comma),
        k(KeyCode::Dot),
        k(KeyCode::Slash),
        m(ModifierKey::RSHIFT),
        m(ModifierKey::LCTRL),
        m(ModifierKey::LGUI),
        m(ModifierKey::LALT),
        m(ModifierKey::LSHIFT),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space,
        },
        k(KeyCode::Space),
        k(KeyCode::Space),
        m(ModifierKey::RSHIFT),
        k(KeyCode::Left),
        k(KeyCode::Down),
        k(KeyCode::Up),
        k(KeyCode::Right),
    ];
    let fn_layer: [Action<()>; 48] = [
        k(KeyCode::Grave),
        k(KeyCode::N1),
        k(KeyCode::N2),
        k(KeyCode::N3),
        k(KeyCode::N4),
        k(KeyCode::N5),
        k(KeyCode::N6),
        k(KeyCode::N7),
        k(KeyCode::N8),
        k(KeyCode::N9),
        k(KeyCode::N0),
        k(KeyCode::Delete),
        tr,
        k(KeyCode::F1),
        k(KeyCode::F2),
        k(KeyCode::F3),
        k(KeyCode::F4),
        k(KeyCode::F5),
        k(KeyCode::F6),
        k(KeyCode::Minus),
        k(KeyCode::Equal),
        k(KeyCode::LeftBrace),
        k(KeyCode::RightBrace),
        k(KeyCode::Backslash),
        tr,
        k(KeyCode::F7),
        k(KeyCode::F8),
        k(KeyCode::F9),
        k(KeyCode::F10),
        k(KeyCode::F11),
        k(KeyCode::F12),
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
        tr,
    ];

    let mut layers = HVec::<KLayer<()>, PLANCK_LAYERS>::new();
    layers.push(KLayer::new(&base)).ok();
    layers.push(KLayer::new(&fn_layer)).ok();
    KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
}

fn planck_collect(
    proc: &mut KeyboardProcessor<PLANCK_ROWS, PLANCK_COLS, PLANCK_LAYERS, PLANCK_KEYS>,
    event: KeyEvent,
    now: u64,
) -> std::vec::Vec<HidReport> {
    let mut out = std::vec::Vec::new();
    proc.process_event(event, now, &mut |r| out.push(r));
    out
}

fn planck_tick(
    proc: &mut KeyboardProcessor<PLANCK_ROWS, PLANCK_COLS, PLANCK_LAYERS, PLANCK_KEYS>,
    now: u64,
) -> std::vec::Vec<HidReport> {
    let mut out = std::vec::Vec::new();
    proc.tick(now, &mut |r| out.push(r));
    out
}

#[test]
fn tap_hold_lt_tap_and_hold() {
    let mut proc = planck_processor();
    let tap_hold_pos = KeyPos::new(3, 4);
    let fn_key_pos = KeyPos::new(0, 1);
    let mut time = 1100u64;

    // ── TAP: press LT!(Fn, SPC), release quickly, then tick past tapping_term ──
    let mut tap_result: u8 = 0;
    let reports = planck_collect(&mut proc, press(tap_hold_pos, time), time);
    for r in &reports {
        let kc = kb_keycodes(r);
        if kc.first().copied().unwrap_or(0) != 0 {
            tap_result = kc.first().copied().unwrap_or(0);
        }
    }
    time += 10;

    let reports = planck_collect(&mut proc, release(tap_hold_pos, time), time);
    for r in &reports {
        let kc = kb_keycodes(r);
        if kc.first().copied().unwrap_or(0) != 0 {
            tap_result = kc.first().copied().unwrap_or(0);
        }
    }
    time += 250;

    let reports = planck_tick(&mut proc, time);
    for r in &reports {
        let kc = kb_keycodes(r);
        if kc.first().copied().unwrap_or(0) != 0 {
            tap_result = kc.first().copied().unwrap_or(0);
        }
    }

    assert_eq!(
        tap_result,
        KeyCode::Space.usage_id(),
        "TAP: LT!(Fn, SPC) quick tap should produce Space (0x2C), got 0x{tap_result:02X}"
    );

    time += 50;

    // ── HOLD: press LT!(Fn, SPC), wait past tapping_term, press (0,1) ──
    let mut hold_result: u8 = 0;

    let reports = planck_collect(&mut proc, press(tap_hold_pos, time), time);
    for r in &reports {
        let kc = kb_keycodes(r);
        if kc.first().copied().unwrap_or(0) != 0 {
            hold_result = kc.first().copied().unwrap_or(0);
        }
    }
    time += 250;

    let reports = planck_tick(&mut proc, time);
    for r in &reports {
        let kc = kb_keycodes(r);
        if kc.first().copied().unwrap_or(0) != 0 {
            hold_result = kc.first().copied().unwrap_or(0);
        }
    }
    time += 10;

    let reports = planck_collect(&mut proc, press(fn_key_pos, time), time);
    for r in &reports {
        let kc = kb_keycodes(r);
        if kc.first().copied().unwrap_or(0) != 0 {
            hold_result = kc.first().copied().unwrap_or(0);
        }
    }

    assert_eq!(
        hold_result,
        KeyCode::N1.usage_id(),
        "HOLD: pressing (0,1) with Fn layer active should produce N1 (0x1E), got 0x{hold_result:02X}"
    );
}
