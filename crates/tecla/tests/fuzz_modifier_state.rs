//! Property-based fuzz of [`ModifierState`].
//!
//! The state holds three orthogonal masks (real / weak / oneshot) that are
//! OR'd into the HID report byte. The invariants worth checking are small
//! but interaction between the three stacks is subtle:
//!
//! - Press/release on `real` only ever modifies `real`.
//! - `set_weak` replaces, never OR's, and `clear_weak` empties it.
//! - `register_oneshot` OR's; `consume_oneshot` returns and clears.
//! - `report_byte() == real | weak | oneshot` after every operation.

#![allow(missing_docs, reason = "integration test")]
#![allow(clippy::tests_outside_test_module, reason = "integration test")]

use proptest::prelude::*;
use tecla::action::ModifierKey;
use tecla::pipeline::modifier::ModifierState;

#[derive(Debug, Clone, Copy)]
enum Op {
    Press(u8),
    Release(u8),
    SetWeak(u8),
    ClearWeak,
    RegisterOneshot(u8),
    ConsumeOneshot,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<u8>()).prop_map(Op::Press),
        (any::<u8>()).prop_map(Op::Release),
        (any::<u8>()).prop_map(Op::SetWeak),
        Just(Op::ClearWeak),
        (any::<u8>()).prop_map(Op::RegisterOneshot),
        Just(Op::ConsumeOneshot),
    ]
}

fn apply(state: &mut ModifierState, op: Op) {
    match op {
        Op::Press(bits) => state.press(ModifierKey::from_bits(bits)),
        Op::Release(bits) => state.release(ModifierKey::from_bits(bits)),
        Op::SetWeak(bits) => state.set_weak(ModifierKey::from_bits(bits)),
        Op::ClearWeak => state.clear_weak(),
        Op::RegisterOneshot(bits) => state.register_oneshot(ModifierKey::from_bits(bits)),
        Op::ConsumeOneshot => {
            let _ = state.consume_oneshot();
        }
    }
}

proptest! {
    /// `report_byte()` always equals `real | weak | oneshot`.
    #[test]
    fn report_byte_is_union_of_three_stacks(ops in proptest::collection::vec(op_strategy(), 0..64)) {
        let mut state = ModifierState::new();
        for op in &ops {
            apply(&mut state, *op);
            let expected =
                state.real().union(state.weak()).union(state.oneshot());
            prop_assert_eq!(state.report_byte(), expected,
                "report_byte diverged after {:?}", op);
        }
    }

    /// `press(m) + release(m)` clears `m`'s bits and preserves bits *not* in `m`.
    /// (Not full identity — by QMK convention, `release` is unconditional bit-clear,
    /// so any pre-existing bits that overlap with `m` are intentionally lost.)
    #[test]
    fn press_release_clears_m_and_preserves_disjoint_bits(
        existing in any::<u8>(),
        m in any::<u8>(),
    ) {
        let mut state = ModifierState::new();
        state.press(ModifierKey::from_bits(existing));
        state.press(ModifierKey::from_bits(m));
        state.release(ModifierKey::from_bits(m));
        let after = state.real().bits();
        // No bits in `m` remain.
        prop_assert_eq!(after & m, 0,
            "release({:#b}) left bits {:#b} held", m, after & m);
        // Bits in `existing` that are NOT in `m` are preserved.
        let preserved_expected = existing & !m;
        prop_assert_eq!(after & !m, preserved_expected,
            "disjoint bits of existing not preserved: expected {:#b}, got {:#b}",
            preserved_expected, after & !m);
    }

    /// `consume_oneshot` returns exactly what was registered and clears it.
    #[test]
    fn consume_oneshot_returns_and_clears(
        regs in proptest::collection::vec(any::<u8>(), 0..8),
    ) {
        let mut state = ModifierState::new();
        let mut expected = ModifierKey::NONE;
        for r in &regs {
            state.register_oneshot(ModifierKey::from_bits(*r));
            expected = expected.union(ModifierKey::from_bits(*r));
        }
        let consumed = state.consume_oneshot();
        prop_assert_eq!(consumed, expected, "consume_oneshot returned wrong mask");
        prop_assert_eq!(state.oneshot(), ModifierKey::NONE,
            "oneshot not cleared after consume");
    }

    /// `set_weak(m)` replaces (not OR's): immediately followed by `set_weak(0)`
    /// should leave weak empty.
    #[test]
    fn set_weak_replaces_not_unions(
        first in any::<u8>(),
        second in any::<u8>(),
    ) {
        let mut state = ModifierState::new();
        state.set_weak(ModifierKey::from_bits(first));
        state.set_weak(ModifierKey::from_bits(second));
        prop_assert_eq!(state.weak(), ModifierKey::from_bits(second),
            "set_weak OR'd instead of replacing: weak={:#b} after set({:#b}) then set({:#b})",
            state.weak().bits(), first, second);
    }

    /// `clear_weak()` empties weak regardless of prior state.
    #[test]
    fn clear_weak_always_empties(bits in any::<u8>()) {
        let mut state = ModifierState::new();
        state.set_weak(ModifierKey::from_bits(bits));
        state.clear_weak();
        prop_assert_eq!(state.weak(), ModifierKey::NONE);
    }

    /// Stack independence: ops on one stack never affect another.
    #[test]
    fn real_release_does_not_touch_oneshot_or_weak(
        real_bits in any::<u8>(),
        weak_bits in any::<u8>(),
        oneshot_bits in any::<u8>(),
    ) {
        let mut state = ModifierState::new();
        state.press(ModifierKey::from_bits(real_bits));
        state.set_weak(ModifierKey::from_bits(weak_bits));
        state.register_oneshot(ModifierKey::from_bits(oneshot_bits));
        let weak_before = state.weak();
        let oneshot_before = state.oneshot();

        state.release(ModifierKey::from_bits(real_bits));

        prop_assert_eq!(state.weak(), weak_before);
        prop_assert_eq!(state.oneshot(), oneshot_before);
        prop_assert_eq!(state.real(), ModifierKey::NONE);
    }
}
