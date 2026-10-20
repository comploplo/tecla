//! `KeyboardProcessor::process_event` latency benchmarks.
//!
//! Measures per-event cost across realistic workloads. Use as a relative
//! regression check — x86 microseconds translate to ARM ratios within ±2×.
//!
//! Workloads:
//!
//! - **plain key press/release** — baseline; no recognizers active.
//! - **tap-hold burst** — exercises the disambiguation buffer.
//! - **modifier roll** — tracks modifier state mutations.
//! - **rollover burst** — 32 keys pressed back-to-back, 6KRO truncation path.

#![allow(missing_docs, reason = "criterion bench harness")]

use criterion::{criterion_group, criterion_main, Criterion};
use heapless::Vec as HVec;
use std::hint::black_box;
use tecla::action::{Action, KeyCode, ModifierKey};
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::keymap::{Layer, LayerStack};
use tecla::pipeline::report::HidReport;

const ROWS: usize = 4;
const COLS: usize = 8;
const LAYERS: usize = 2;
const KEYS: usize = ROWS * COLS;

fn build_processor() -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    let mut base = [Action::<()>::NoOp; KEYS];
    // First 26 keys are A..Z.
    let alphabet = [
        KeyCode::A,
        KeyCode::B,
        KeyCode::C,
        KeyCode::D,
        KeyCode::E,
        KeyCode::F,
        KeyCode::G,
        KeyCode::H,
        KeyCode::I,
        KeyCode::J,
        KeyCode::K,
        KeyCode::L,
        KeyCode::M,
        KeyCode::N,
        KeyCode::O,
        KeyCode::P,
        KeyCode::Q,
        KeyCode::R,
        KeyCode::S,
        KeyCode::T,
        KeyCode::U,
        KeyCode::V,
        KeyCode::W,
        KeyCode::X,
        KeyCode::Y,
        KeyCode::Z,
    ];
    for (i, kc) in alphabet.iter().enumerate() {
        base[i] = Action::Key(*kc);
    }
    base[26] = Action::Modifier(ModifierKey::LSHIFT);
    base[27] = Action::Modifier(ModifierKey::LCTRL);
    base[28] = Action::LayerTap {
        layer: 1,
        tap: KeyCode::Space,
    };
    base[29] = Action::Key(KeyCode::Enter);

    let overlay = [Action::<()>::NoOp; KEYS];

    let mut layers = HVec::<Layer<()>, LAYERS>::new();
    layers.push(Layer::new(&base)).ok();
    layers.push(Layer::new(&overlay)).ok();
    KeyboardProcessor::new(
        LayerStack::new(layers),
        TapHoldConfig {
            tapping_term_ms: 200,
            ..TapHoldConfig::default()
        },
    )
}

fn run_events(proc: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>, events: &[KeyEvent]) {
    let mut sink = |_: HidReport| {};
    for ev in events {
        proc.process_event(*ev, ev.timestamp_ms, &mut sink);
    }
}

fn bench_plain_alpha(c: &mut Criterion) {
    // Type "abcdefgh" — 8 press/release pairs.
    let mut events = Vec::new();
    let mut t = 0u64;
    for col in 0..8u8 {
        let pos = KeyPos::new(0, col);
        events.push(KeyEvent::press(pos, t));
        t += 5;
        events.push(KeyEvent::release(pos, t));
        t += 5;
    }
    c.bench_function("process_event/plain_alpha_8_chars", |b| {
        b.iter(|| {
            let mut proc = build_processor();
            run_events(&mut proc, black_box(&events));
        });
    });
}

fn bench_modifier_roll(c: &mut Criterion) {
    // Hold shift, press 5 keys, release shift.
    let mods = KeyPos::new(3, 2); // base[26] = LSFT
    let mut events = vec![KeyEvent::press(mods, 0)];
    let mut t = 1u64;
    for col in 0..5u8 {
        let pos = KeyPos::new(0, col);
        events.push(KeyEvent::press(pos, t));
        t += 5;
        events.push(KeyEvent::release(pos, t));
        t += 5;
    }
    events.push(KeyEvent::release(mods, t));
    c.bench_function("process_event/modifier_roll", |b| {
        b.iter(|| {
            let mut proc = build_processor();
            run_events(&mut proc, black_box(&events));
        });
    });
}

fn bench_tap_hold_settle(c: &mut Criterion) {
    let lt = KeyPos::new(3, 4); // base[28] = LayerTap { layer: 1, tap: Space }
                                // Press LT, then tick past tapping_term_ms (200) so it resolves as tap.
    let events = vec![KeyEvent::press(lt, 0), KeyEvent::release(lt, 50)];
    c.bench_function("process_event/tap_hold_tap_path", |b| {
        b.iter(|| {
            let mut proc = build_processor();
            run_events(&mut proc, black_box(&events));
        });
    });

    // Press LT, hold past tapping_term, press another key — resolves as hold.
    let other = KeyPos::new(0, 0);
    let events_hold = vec![
        KeyEvent::press(lt, 0),
        KeyEvent::press(other, 250),
        KeyEvent::release(other, 260),
        KeyEvent::release(lt, 270),
    ];
    c.bench_function("process_event/tap_hold_hold_path", |b| {
        b.iter(|| {
            let mut proc = build_processor();
            run_events(&mut proc, black_box(&events_hold));
        });
    });
}

fn bench_rollover_burst(c: &mut Criterion) {
    // 16 keys pressed in rapid succession then released — exceeds 6KRO,
    // exercises the truncation path.
    let mut events = Vec::new();
    let mut t = 0u64;
    for col in 0..8u8 {
        events.push(KeyEvent::press(KeyPos::new(0, col), t));
        t += 1;
    }
    for col in 0..8u8 {
        events.push(KeyEvent::press(KeyPos::new(1, col), t));
        t += 1;
    }
    for col in 0..8u8 {
        events.push(KeyEvent::release(KeyPos::new(0, col), t));
        t += 1;
    }
    for col in 0..8u8 {
        events.push(KeyEvent::release(KeyPos::new(1, col), t));
        t += 1;
    }
    c.bench_function("process_event/rollover_16_keys", |b| {
        b.iter(|| {
            let mut proc = build_processor();
            run_events(&mut proc, black_box(&events));
        });
    });
}

criterion_group!(
    benches,
    bench_plain_alpha,
    bench_modifier_roll,
    bench_tap_hold_settle,
    bench_rollover_burst
);
criterion_main!(benches);
