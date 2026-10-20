//! Property-based fuzz of individual recognizers in isolation.
//!
//! Each recognizer is unified by the [`Recognizer`] trait; the contract worth
//! holding across all of them is *convergence*: from any event stream, the
//! recognizer must terminate in finite time — never stay `Pending` forever.
//!
//! For [`TapHoldRecognizer`] specifically:
//!
//! - Resolution always happens by `press_ms + tapping_term_ms` at the latest.
//! - `next_deadline_ms` strictly equals `press_ms + tapping_term_ms` while
//!   pending.
//! - After resolution, `next_deadline_ms` returns `None`.
//! - All three modes (Default / `HoldOnOtherKeyPress` / `PermissiveHold`)
//!   eventually resolve.
//! - For each mode, the resolved decision is one of the four valid variants —
//!   never produces nonsense.

#![allow(missing_docs, reason = "integration test")]
#![allow(clippy::tests_outside_test_module, reason = "integration test")]

use proptest::prelude::*;
use tecla::action::{KeyCode, ModifierKey};
use tecla::devices::keyboard::tapping::{
    TapHoldConfig, TapHoldDecision, TapHoldMode, TapHoldRecognizer,
};
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::recognizer::{Recognition, Recognizer};

const TAP_POS_ROW: u8 = 0;
const TAP_POS_COL: u8 = 0;

fn tap_pos() -> KeyPos {
    KeyPos::new(TAP_POS_ROW, TAP_POS_COL)
}

fn mode_strategy() -> impl Strategy<Value = TapHoldMode> {
    prop_oneof![
        Just(TapHoldMode::Default),
        Just(TapHoldMode::HoldOnOtherKeyPress),
        Just(TapHoldMode::PermissiveHold),
    ]
}

/// Generates an event at some non-tap position (so we exercise the "other key"
/// paths instead of an own-key release).
fn other_event_strategy() -> impl Strategy<Value = (u8, bool, u16)> {
    // (col 1..7, pressed, dt_ms 1..30)
    (1u8..8, any::<bool>(), 1u16..30)
}

fn realize(press_ms: u64, specs: &[(u8, bool, u16)]) -> Vec<KeyEvent> {
    let mut t = press_ms;
    specs
        .iter()
        .map(|(col, pressed, dt)| {
            t = t.saturating_add(u64::from(*dt));
            let pos = KeyPos::new(0, *col);
            if *pressed {
                KeyEvent::press(pos, t)
            } else {
                KeyEvent::release(pos, t)
            }
        })
        .collect()
}

fn make_layer_tap(mode: TapHoldMode, term: u64, press_ms: u64) -> TapHoldRecognizer<32> {
    TapHoldRecognizer::layer_tap(
        TapHoldConfig::new(term).with_mode(mode),
        1u8,
        KeyCode::Space,
        KeyEvent::press(tap_pos(), press_ms),
    )
}

fn make_mod_tap(mode: TapHoldMode, term: u64, press_ms: u64) -> TapHoldRecognizer<32> {
    TapHoldRecognizer::mod_tap(
        TapHoldConfig::new(term).with_mode(mode),
        ModifierKey::LSHIFT,
        KeyCode::A,
        KeyEvent::press(tap_pos(), press_ms),
    )
}

/// Drives the recognizer through `events` and a final tick at `press_ms +
/// term + 1`. Returns the eventual recognition (Matched or Failed).
fn drive(
    rec: &mut TapHoldRecognizer<32>,
    events: &[KeyEvent],
    deadline_ms: u64,
) -> Recognition<TapHoldDecision> {
    for ev in events {
        let r = rec.observe(*ev, ev.timestamp_ms);
        if !r.is_pending() {
            return r;
        }
    }
    rec.tick(deadline_ms)
}

proptest! {
    /// Across all modes and event streams (including empty), the recognizer
    /// resolves by `press_ms + tapping_term_ms` at the latest.
    #[test]
    fn always_resolves_by_deadline(
        mode in mode_strategy(),
        term in 50u64..500,
        specs in proptest::collection::vec(other_event_strategy(), 0..16),
    ) {
        let press_ms = 1_000u64;
        let mut rec = make_layer_tap(mode, term, press_ms);
        let events = realize(press_ms, &specs);
        let result = drive(&mut rec, &events, press_ms + term + 1);
        prop_assert!(!result.is_pending(),
            "recognizer still pending past deadline ({:?} mode, term={}ms, {} events)",
            mode, term, events.len());
    }

    /// Before resolution, `next_deadline_ms` equals exactly `press_ms + term`.
    #[test]
    fn next_deadline_equals_press_plus_term(
        mode in mode_strategy(),
        term in 50u64..500,
        press_ms in 0u64..100_000,
    ) {
        let rec = make_layer_tap(mode, term, press_ms);
        prop_assert_eq!(rec.next_deadline_ms(), Some(press_ms.wrapping_add(term)));
    }

    /// After resolution, `next_deadline_ms` returns `None` and further calls
    /// to `tick` stay `Pending` (do not re-fire).
    #[test]
    fn no_deadline_after_resolution(
        mode in mode_strategy(),
        term in 50u64..500,
    ) {
        let press_ms = 1_000u64;
        let mut rec = make_layer_tap(mode, term, press_ms);
        // Force resolve via tick at the deadline.
        let r = rec.tick(press_ms + term);
        prop_assert!(r.is_matched(), "tick at deadline should resolve");
        prop_assert!(rec.next_deadline_ms().is_none(),
            "deadline should be None after resolution");
        let r2 = rec.tick(press_ms + term + 100);
        prop_assert!(r2.is_pending(),
            "second tick after resolution should be Pending, got {:?}", r2);
    }

    /// Mod-tap: release of the tap key BEFORE term → resolves as
    /// `Tap(tap_keycode)`.
    #[test]
    fn mod_tap_release_within_term_resolves_as_tap(
        term in 50u64..500,
        release_offset in 1u64..40,
    ) {
        prop_assume!(release_offset < term);
        let press_ms = 1_000u64;
        let mut rec = make_mod_tap(TapHoldMode::Default, term, press_ms);
        let release = KeyEvent::release(tap_pos(), press_ms + release_offset);
        let r = rec.observe(release, press_ms + release_offset);
        prop_assert_eq!(r, Recognition::Matched(TapHoldDecision::Tap(KeyCode::A)),
            "release within term should resolve as Tap, got {:?}", r);
    }

    /// HoldOnOtherKeyPress: any other-key press during term → immediate
    /// HoldMod / HoldLayer.
    #[test]
    fn hold_on_other_key_press_resolves_immediately(
        term in 100u64..500,
        offset in 1u64..50,
    ) {
        prop_assume!(offset < term);
        let press_ms = 1_000u64;
        let mut rec = make_layer_tap(TapHoldMode::HoldOnOtherKeyPress, term, press_ms);
        let other = KeyEvent::press(KeyPos::new(0, 1), press_ms + offset);
        let r = rec.observe(other, press_ms + offset);
        prop_assert_eq!(r, Recognition::Matched(TapHoldDecision::HoldLayer(1)),
            "other-key press should immediately resolve as HoldLayer, got {:?}", r);
    }

    /// PermissiveHold: other key press THEN release within term → resolve
    /// as Hold.
    #[test]
    fn permissive_hold_press_then_release_resolves(
        term in 100u64..500,
        press_offset in 1u64..50,
        release_offset in 1u64..50,
    ) {
        prop_assume!(press_offset + release_offset < term);
        let press_ms = 1_000u64;
        let mut rec = make_layer_tap(TapHoldMode::PermissiveHold, term, press_ms);
        let other_press = KeyEvent::press(KeyPos::new(0, 1), press_ms + press_offset);
        prop_assert!(rec.observe(other_press, press_ms + press_offset).is_pending());
        let other_release = KeyEvent::release(KeyPos::new(0, 1), press_ms + press_offset + release_offset);
        let r = rec.observe(other_release, press_ms + press_offset + release_offset);
        prop_assert_eq!(r, Recognition::Matched(TapHoldDecision::HoldLayer(1)),
            "press+release within term should resolve as HoldLayer under PermissiveHold");
    }

    /// Default mode: tick AT or PAST `press_ms + term` always resolves
    /// (never panics, never stays pending).
    #[test]
    fn default_mode_tick_at_deadline_resolves(
        term in 50u64..500,
        slack_ms in 0u64..1_000,
    ) {
        let press_ms = 1_000u64;
        let mut rec = make_layer_tap(TapHoldMode::Default, term, press_ms);
        let r = rec.tick(press_ms + term + slack_ms);
        prop_assert!(r.is_matched(), "tick at deadline+{} returned {:?}", slack_ms, r);
    }

    /// A mod-tap recognizer only ever resolves to a mod-tap outcome. The
    /// layer variants belong to layer-tap and must never appear here — a
    /// `matches!` against all four variants would be true by construction.
    #[test]
    fn mod_tap_never_resolves_to_a_layer_decision(
        mode in mode_strategy(),
        term in 50u64..500,
        specs in proptest::collection::vec(other_event_strategy(), 0..16),
    ) {
        let press_ms = 1_000u64;
        let mut rec = make_mod_tap(mode, term, press_ms);
        let events = realize(press_ms, &specs);
        let result = drive(&mut rec, &events, press_ms + term + 1);
        let dec = result.matched();
        prop_assume!(dec.is_some());
        let dec = dec.expect("checked above");
        prop_assert!(
            matches!(dec, TapHoldDecision::Tap(_) | TapHoldDecision::HoldMod(_)),
            "mod-tap resolved to a layer decision: {:?}", dec
        );
    }
}
