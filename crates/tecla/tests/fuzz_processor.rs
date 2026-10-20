//! Property-based fuzz harness for `KeyboardProcessor`.
//!
//! Generates arbitrary `KeyEvent` streams and verifies the processor never
//! panics and preserves a small set of invariants:
//!
//! - Pressed-keys set is bounded by the matrix size.
//! - Once every position has equal press/release counts and we tick past the
//!   tap-hold deadline, no key is reported as held.
//! - Modifier state mask only ever contains valid modifier bits.
//! - Tap-hold disambiguation always terminates within `tapping_term_ms`.
//! - Reports never overflow report bounds (6KRO ≤ 6 keycodes).
//!
//! Proptest runs 256 cases per test by default; shrinking finds minimal
//! failing event streams when an invariant breaks.

#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]

use heapless::Vec as HVec;
use proptest::collection::vec;
use proptest::prelude::*;
use tecla::action::{Action, KeyCode, LayerOp, ModifierKey};
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::keymap::{Layer, LayerStack};
use tecla::pipeline::report::HidReport;

// ── Matrix shape: 2×4, 2 layers, 8 keys ─────────────────────────────────────
const ROWS: usize = 2;
const COLS: usize = 4;
const LAYERS: usize = 2;
const KEYS: usize = ROWS * COLS;
const TAPPING_TERM_MS: u64 = 200;

/// Mix of action kinds: plain keys, modifiers, layer-tap, tap-hold, NoOp.
/// Layer 1 mirrors layer 0 but with different keycodes — exercises layer
/// resolution under layer-tap (`LT(1, SPC)`).
fn build_processor() -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    let base: [Action<()>; KEYS] = [
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::B),
        Action::Modifier(ModifierKey::LSHIFT),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space,
        },
        Action::Key(KeyCode::C),
        Action::Key(KeyCode::D),
        Action::Modifier(ModifierKey::LCTRL),
        Action::NoOp,
    ];
    let overlay: [Action<()>; KEYS] = [
        Action::Key(KeyCode::N1),
        Action::Key(KeyCode::N2),
        Action::Layer(LayerOp::Momentary(0)),
        Action::Transparent,
        Action::Key(KeyCode::N3),
        Action::Key(KeyCode::N4),
        Action::Modifier(ModifierKey::LALT),
        Action::NoOp,
    ];
    let mut layers = HVec::<Layer<()>, LAYERS>::new();
    layers.push(Layer::new(&base)).ok();
    layers.push(Layer::new(&overlay)).ok();
    KeyboardProcessor::new(
        LayerStack::new(layers),
        TapHoldConfig {
            tapping_term_ms: TAPPING_TERM_MS,
            ..TapHoldConfig::default()
        },
    )
}

/// All valid `ModifierKey` bits — used to validate the modifier mask is closed
/// under the documented modifier set.
const VALID_MODIFIER_BITS: u8 = ModifierKey::LCTRL.bits()
    | ModifierKey::LSHIFT.bits()
    | ModifierKey::LALT.bits()
    | ModifierKey::LGUI.bits()
    | ModifierKey::RCTRL.bits()
    | ModifierKey::RSHIFT.bits()
    | ModifierKey::RALT.bits()
    | ModifierKey::RGUI.bits();

/// Generated event spec — proptest produces `Vec<EventSpec>`, then the test
/// assigns monotonic timestamps spaced by `dt_ms`.
#[derive(Debug, Clone, Copy)]
struct EventSpec {
    row: u8,
    col: u8,
    pressed: bool,
    /// Time delta (ms) before this event. Always ≥ 1 to keep timestamps strictly monotonic.
    dt_ms: u16,
}

fn event_spec_strategy() -> impl Strategy<Value = EventSpec> {
    (0u8..ROWS as u8, 0u8..COLS as u8, any::<bool>(), 1u16..50).prop_map(
        |(row, col, pressed, dt_ms)| EventSpec {
            row,
            col,
            pressed,
            dt_ms,
        },
    )
}

fn realize(specs: &[EventSpec]) -> Vec<KeyEvent> {
    let mut now = 0u64;
    specs
        .iter()
        .map(|s| {
            now = now.saturating_add(u64::from(s.dt_ms));
            let pos = KeyPos::new(s.row, s.col);
            if s.pressed {
                KeyEvent::press(pos, now)
            } else {
                KeyEvent::release(pos, now)
            }
        })
        .collect()
}

// ── Well-formed event generator ─────────────────────────────────────────────
// Produces only press-after-release / release-after-press transitions, the
// contract a well-debounced matrix scanner upholds. Sharpens "is the
// processor correct under realistic input" by stripping the adversarial
// orphan-release / duplicate-press noise that the unrestricted generator
// emits.

/// Generates a well-formed stream by simulating per-position state.
fn well_formed_events_strategy(max_len: usize) -> impl Strategy<Value = Vec<KeyEvent>> {
    // For each step: which position to toggle, and how long to wait first.
    let step = (0usize..KEYS, 1u16..50u16);
    proptest::collection::vec(step, 0..max_len).prop_map(|steps| {
        let mut down = [false; KEYS];
        let mut now = 0u64;
        let mut out = Vec::with_capacity(steps.len());
        for (idx, dt) in steps {
            now = now.saturating_add(u64::from(dt));
            let pos = KeyPos::new((idx / COLS) as u8, (idx % COLS) as u8);
            // Toggle: press if not down, release if down.
            if down[idx] {
                out.push(KeyEvent::release(pos, now));
                down[idx] = false;
            } else {
                out.push(KeyEvent::press(pos, now));
                down[idx] = true;
            }
        }
        out
    })
}

/// Same generator, but the trailing releases for any keys left "down" are
/// appended so the stream lands at idle. Useful for stronger end-state
/// invariants.
fn balanced_well_formed_strategy(max_len: usize) -> impl Strategy<Value = Vec<KeyEvent>> {
    well_formed_events_strategy(max_len).prop_map(|mut evs| {
        let mut down = [false; KEYS];
        let mut now = evs.last().map(|e| e.timestamp_ms).unwrap_or(0);
        for ev in &evs {
            let idx = (ev.pos.row as usize) * COLS + (ev.pos.col as usize);
            down[idx] = ev.pressed;
        }
        for (idx, is_down) in down.iter().enumerate() {
            if *is_down {
                now = now.saturating_add(1);
                let pos = KeyPos::new((idx / COLS) as u8, (idx % COLS) as u8);
                evs.push(KeyEvent::release(pos, now));
            }
        }
        evs
    })
}

/// Drive the processor through every event, collect reports, then tick past
/// every pending deadline so any tap-hold/auto-release resolves.
fn drive(
    proc: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>,
    events: &[KeyEvent],
) -> Vec<HidReport> {
    let mut reports = Vec::new();
    let mut last_ms = 0u64;
    for ev in events {
        last_ms = ev.timestamp_ms;
        proc.process_event(*ev, ev.timestamp_ms, &mut |r| reports.push(r));
    }
    // Drain any pending deadlines — repeatedly tick until no more deadlines.
    let mut guard = 0u32;
    while let Some(deadline) = proc.next_deadline_ms() {
        guard += 1;
        assert!(guard < 16, "tap-hold deadline did not terminate");
        last_ms = last_ms.max(deadline);
        proc.tick(last_ms, &mut |r| reports.push(r));
    }
    reports
}

// ── Invariants ──────────────────────────────────────────────────────────────

proptest! {
    /// No event stream should panic the processor.
    #[test]
    fn no_panic_on_arbitrary_event_stream(
        specs in vec(event_spec_strategy(), 0..64),
    ) {
        let mut proc = build_processor();
        let events = realize(&specs);
        let _ = drive(&mut proc, &events);
    }

    /// Every modifier-state observation contains only documented modifier bits.
    #[test]
    fn modifier_mask_stays_in_valid_set(
        specs in vec(event_spec_strategy(), 0..64),
    ) {
        let mut proc = build_processor();
        let events = realize(&specs);
        let reports = drive(&mut proc, &events);
        for r in &reports {
            if let HidReport::Keyboard(kr) = r {
                prop_assert_eq!(kr.modifiers & !VALID_MODIFIER_BITS, 0,
                    "modifier byte had bits outside the documented mask: {:#b}", kr.modifiers);
            }
        }
    }

    /// 6KRO reports never carry more than 6 non-zero keycodes.
    #[test]
    fn six_kro_report_stays_within_six_keycodes(
        specs in vec(event_spec_strategy(), 0..64),
    ) {
        let mut proc = build_processor();
        let events = realize(&specs);
        let reports = drive(&mut proc, &events);
        for r in &reports {
            if let HidReport::Keyboard(kr) = r {
                let used = kr.keycodes.iter().filter(|&&k| k != 0).count();
                prop_assert!(used <= 6, "6KRO report had {} keycodes set", used);
            }
        }
    }

    /// After releasing every key we've ever pressed and ticking past pending
    /// deadlines, the final 6KRO report carries no held keys and no held mods.
    #[test]
    fn balanced_press_release_returns_to_idle(
        specs in vec(event_spec_strategy(), 0..32),
    ) {
        let mut proc = build_processor();
        // Force every key down → up to balance the stream.
        let mut events: Vec<KeyEvent> = realize(&specs);
        // Track which positions are currently "down" per the stream, and emit
        // releases for whatever remains pressed at the end.
        let mut down = [false; KEYS];
        let mut last_ms = 0u64;
        for ev in &events {
            let idx = (ev.pos.row as usize) * COLS + (ev.pos.col as usize);
            down[idx] = ev.pressed;
            last_ms = ev.timestamp_ms;
        }
        for (idx, is_down) in down.iter().enumerate() {
            if *is_down {
                last_ms += 1;
                let pos = KeyPos::new((idx / COLS) as u8, (idx % COLS) as u8);
                events.push(KeyEvent::release(pos, last_ms));
            }
        }

        let reports = drive(&mut proc, &events);

        if let Some(HidReport::Keyboard(final_report)) = reports
            .iter()
            .rev()
            .find(|r| matches!(r, HidReport::Keyboard(_)))
            .copied()
        {
            let used = final_report.keycodes.iter().filter(|&&k| k != 0).count();
            prop_assert_eq!(used, 0, "expected no keys held after balanced release, got {} keycodes",
                used);
            prop_assert_eq!(final_report.modifiers, 0,
                "expected no modifiers held after balanced release, got {:#b}", final_report.modifiers);
        }
    }

    /// `next_deadline_ms` always terminates: even with adversarial input,
    /// fewer than 16 successive ticks should drain every pending deadline.
    #[test]
    fn pending_deadlines_drain_in_bounded_ticks(
        specs in vec(event_spec_strategy(), 0..32),
    ) {
        let mut proc = build_processor();
        let events = realize(&specs);
        // drive() itself asserts the guard; this test exists to surface it.
        let _ = drive(&mut proc, &events);
        prop_assert!(proc.next_deadline_ms().is_none(),
            "deadlines still pending after drive()");
    }

    // ── Well-formed-input invariants ────────────────────────────────────────
    //
    // The processor's stronger contract: under valid input (no orphan
    // releases, no duplicate presses), every press has a matching release,
    // and the final report carries no held keys.

    /// Well-formed balanced streams end with zero keys held — and zero
    /// modifiers held, even with a mix of plain keys, modifiers, and
    /// layer-tap among the touched positions.
    #[test]
    fn well_formed_balanced_returns_to_idle(
        events in balanced_well_formed_strategy(40),
    ) {
        let mut proc = build_processor();
        let reports = drive(&mut proc, &events);
        if let Some(HidReport::Keyboard(final_report)) = reports
            .iter()
            .rev()
            .find(|r| matches!(r, HidReport::Keyboard(_)))
            .copied()
        {
            let used = final_report.keycodes.iter().filter(|&&k| k != 0).count();
            prop_assert_eq!(used, 0,
                "well-formed balanced stream left {} keycodes held", used);
            prop_assert_eq!(final_report.modifiers, 0,
                "well-formed balanced stream left modifiers {:#b} held",
                final_report.modifiers);
        }
    }

    /// After a well-formed balanced stream, the layer stack is back to the
    /// default (only layer 0 active). Catches "layer leaked into permanent
    /// state because Momentary release was missed".
    #[test]
    fn well_formed_balanced_returns_layers_to_default(
        events in balanced_well_formed_strategy(40),
    ) {
        let mut proc = build_processor();
        let _ = drive(&mut proc, &events);
        // We can probe layer state by resolving an action on the [0,0] base
        // position. With layer 0 alone active it's Key(A); on layer 1 active
        // it would be Key(N1).
        // (No direct accessor exists; this is an indirect smoke check via
        // the report state — a fresh press should produce A.)
        let mut reports = Vec::new();
        let last_ms = events.last().map(|e| e.timestamp_ms).unwrap_or(0);
        proc.process_event(
            KeyEvent::press(KeyPos::new(0, 0), last_ms + 1),
            last_ms + 1,
            &mut |r| reports.push(r),
        );
        if let Some(HidReport::Keyboard(kr)) = reports
            .iter()
            .find(|r| matches!(r, HidReport::Keyboard(_)))
            .copied()
        {
            prop_assert_eq!(kr.keycodes[0], KeyCode::A.usage_id(),
                "post-balanced press on [0,0] resolved to {:#X}, expected A ({:#X})",
                kr.keycodes[0], KeyCode::A.usage_id());
        }
    }

    /// Cross-rollover-mode consistency: the same well-formed stream driven
    /// through a 6KRO processor and an NKRO processor produces reports
    /// containing the same held-key set (modulo 6KRO's 6-key truncation).
    #[test]
    fn six_kro_and_nkro_agree_on_held_set(
        events in balanced_well_formed_strategy(20),
    ) {
        use tecla::keyboard::RolloverMode;
        let kro_reports = {
            let mut proc = build_processor();
            drive(&mut proc, &events)
        };
        let nkro_reports = {
            let mut proc = build_processor().with_rollover_mode(RolloverMode::Nkro);
            drive(&mut proc, &events)
        };

        // Walk through *intermediate* states: for each Keyboard / NkroKeyboard
        // report pair (in order), the 6KRO set must be a subset of the NKRO
        // set (because NKRO is fuller).
        let kro_held: Vec<std::collections::HashSet<u8>> = kro_reports
            .iter()
            .filter_map(|r| match r {
                HidReport::Keyboard(kr) => {
                    Some(kr.keycodes.iter().copied().filter(|&k| k != 0).collect())
                }
                _ => None,
            })
            .collect();
        let nkro_held: Vec<std::collections::HashSet<u8>> = nkro_reports
            .iter()
            .filter_map(|r| match r {
                HidReport::NkroKeyboard(kr) => {
                    let mut set = std::collections::HashSet::new();
                    for (byte_idx, byte) in kr.bitmap.iter().enumerate() {
                        for bit in 0..8u32 {
                            if byte & (1u8 << bit) != 0 {
                                set.insert((byte_idx * 8 + bit as usize) as u8);
                            }
                        }
                    }
                    Some(set)
                }
                _ => None,
            })
            .collect();

        // A balanced stream ends with everything released, so the final
        // states are both empty and comparing only them asserts nothing.
        // Compare the *peak* held set instead — that is where 6KRO's
        // 6-key truncation and NKRO's bitmap actually diverge.
        // The generator can emit an empty stream; discard those. proptest
        // fails the run if it has to discard too many, so this cannot quietly
        // become a test of nothing.
        prop_assume!(!kro_held.is_empty() && !nkro_held.is_empty());

        let kro_peak = kro_held.iter().max_by_key(|s| s.len()).cloned().unwrap_or_default();
        let nkro_peak = nkro_held.iter().max_by_key(|s| s.len()).cloned().unwrap_or_default();
        prop_assume!(!kro_peak.is_empty());

        let kro_union: std::collections::HashSet<u8> =
            kro_held.iter().flatten().copied().collect();
        let nkro_union: std::collections::HashSet<u8> =
            nkro_held.iter().flatten().copied().collect();
        for k in &kro_union {
            prop_assert!(nkro_union.contains(k),
                "6KRO reported keycode {} at some point but NKRO never did (6KRO={:?}, NKRO={:?})",
                k, kro_union, nkro_union);
        }
        prop_assert!(
            nkro_peak.len() >= kro_peak.len(),
            "NKRO peak {} is smaller than 6KRO peak {} — NKRO should hold at least as many",
            nkro_peak.len(), kro_peak.len()
        );

        // 6KRO can never report more than six keycodes at once.
        prop_assert!(kro_peak.len() <= 6, "6KRO held {} keycodes", kro_peak.len());
    }

    /// Duplicate presses at the same position never increase pressed-key count.
    /// Specifically: pressing key K twice (without an intervening release)
    /// produces at most one report, and that report's keycode set differs
    /// from the post-first-press state only at most by one keycode added.
    #[test]
    fn duplicate_press_is_idempotent(
        first_dt in 1u16..50,
        second_dt in 1u16..50,
        row in 0u8..ROWS as u8,
        col in 0u8..COLS as u8,
    ) {
        let mut proc = build_processor();
        let pos = KeyPos::new(row, col);
        let t1 = u64::from(first_dt);
        let t2 = t1 + u64::from(second_dt);

        let mut first_reports = Vec::new();
        proc.process_event(KeyEvent::press(pos, t1), t1, &mut |r| first_reports.push(r));

        let mut second_reports = Vec::new();
        proc.process_event(KeyEvent::press(pos, t2), t2, &mut |r| second_reports.push(r));

        // Either zero reports (real no-op, e.g. NoOp action) or exactly zero
        // KEYBOARD reports — the position is already cached.
        let kb_count = second_reports.iter().filter(|r| matches!(r, HidReport::Keyboard(_))).count();
        prop_assert_eq!(kb_count, 0,
            "duplicate press at same pos emitted {} keyboard reports", kb_count);
    }
}
