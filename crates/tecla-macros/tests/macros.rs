//! Integration tests for tecla-macros proc macros.
#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
use tecla::action::{Action, KeyCode, LayerOp, ModifierKey};
use tecla::pipeline::event::KeyPos;
use tecla::pipeline::keymap::{Layer, LayerStack};
use tecla_macros::{keymap, layer, steno, LT, MT};

// ── layer! ────────────────────────────────────────────────────────────────────

#[test]
fn layer_macro_produces_correct_layer_type() {
    let l: Layer<()> = layer!(Base, A, B, C, D);
    assert_eq!(l.len(), 4);
}

#[test]
fn layer_macro_letter_keys_resolve_correctly() {
    let l: Layer<()> = layer!(Base, A, B, Z);
    assert_eq!(l.get(0), Some(Action::Key(KeyCode::A)));
    assert_eq!(l.get(1), Some(Action::Key(KeyCode::B)));
    assert_eq!(l.get(2), Some(Action::Key(KeyCode::Z)));
}

#[test]
fn layer_macro_transparent_resolves_to_transparent() {
    let l: Layer<()> = layer!(Base, TRNS, TRANSPARENT);
    assert_eq!(l.get(0), Some(Action::Transparent));
    assert_eq!(l.get(1), Some(Action::Transparent));
}

#[test]
fn layer_macro_noop_resolves_to_noop() {
    let l: Layer<()> = layer!(Base, NOOP, NoOp, XXXXXXX);
    assert_eq!(l.get(0), Some(Action::NoOp));
    assert_eq!(l.get(1), Some(Action::NoOp));
    assert_eq!(l.get(2), Some(Action::NoOp));
}

#[test]
fn layer_macro_function_keys_resolve_correctly() {
    let l: Layer<()> = layer!(Fn, F1, F2, F12);
    assert_eq!(l.get(0), Some(Action::Key(KeyCode::F1)));
    assert_eq!(l.get(1), Some(Action::Key(KeyCode::F2)));
    assert_eq!(l.get(2), Some(Action::Key(KeyCode::F12)));
}

#[test]
fn layer_macro_modifier_keys_resolve_correctly() {
    let l: Layer<()> = layer!(Mods, LSFT, LCTL, LALT, LGUI);
    assert_eq!(l.get(0), Some(Action::Key(KeyCode::LShift)));
    assert_eq!(l.get(1), Some(Action::Key(KeyCode::LCtrl)));
    assert_eq!(l.get(2), Some(Action::Key(KeyCode::LAlt)));
    assert_eq!(l.get(3), Some(Action::Key(KeyCode::LGui)));
}

#[test]
fn layer_macro_space_enter_escape_tab_backspace() {
    let l: Layer<()> = layer!(Nav, SPC, ENT, ESC, TAB, BSPC);
    assert_eq!(l.get(0), Some(Action::Key(KeyCode::Space)));
    assert_eq!(l.get(1), Some(Action::Key(KeyCode::Enter)));
    assert_eq!(l.get(2), Some(Action::Key(KeyCode::Escape)));
    assert_eq!(l.get(3), Some(Action::Key(KeyCode::Tab)));
    assert_eq!(l.get(4), Some(Action::Key(KeyCode::Backspace)));
}

#[test]
fn layer_macro_navigation_keys() {
    let l: Layer<()> = layer!(Nav, UP, DOWN, LEFT, RGHT, HOME, END, PGUP, PGDN);
    assert_eq!(l.get(0), Some(Action::Key(KeyCode::Up)));
    assert_eq!(l.get(1), Some(Action::Key(KeyCode::Down)));
    assert_eq!(l.get(2), Some(Action::Key(KeyCode::Left)));
    assert_eq!(l.get(3), Some(Action::Key(KeyCode::Right)));
    assert_eq!(l.get(4), Some(Action::Key(KeyCode::Home)));
    assert_eq!(l.get(5), Some(Action::Key(KeyCode::End)));
    assert_eq!(l.get(6), Some(Action::Key(KeyCode::PageUp)));
    assert_eq!(l.get(7), Some(Action::Key(KeyCode::PageDown)));
}

#[test]
fn layer_macro_punctuation_keys() {
    let l: Layer<()> = layer!(Sym, MINS, EQL, LBRC, RBRC, BSLS, SCLN, QUOT, GRV, COMM, DOT, SLSH);
    assert_eq!(l.get(0), Some(Action::Key(KeyCode::Minus)));
    assert_eq!(l.get(1), Some(Action::Key(KeyCode::Equal)));
    assert_eq!(l.get(2), Some(Action::Key(KeyCode::LeftBrace)));
    assert_eq!(l.get(3), Some(Action::Key(KeyCode::RightBrace)));
    assert_eq!(l.get(4), Some(Action::Key(KeyCode::Backslash)));
    assert_eq!(l.get(5), Some(Action::Key(KeyCode::Semicolon)));
    assert_eq!(l.get(6), Some(Action::Key(KeyCode::Quote)));
    assert_eq!(l.get(7), Some(Action::Key(KeyCode::Grave)));
    assert_eq!(l.get(8), Some(Action::Key(KeyCode::Comma)));
    assert_eq!(l.get(9), Some(Action::Key(KeyCode::Dot)));
    assert_eq!(l.get(10), Some(Action::Key(KeyCode::Slash)));
}

#[test]
fn layer_macro_digits_resolve_correctly() {
    let l: Layer<()> = layer!(Num, N1, N2, N3, N4, N5, N6, N7, N8, N9, N0);
    assert_eq!(l.get(0), Some(Action::Key(KeyCode::N1)));
    assert_eq!(l.get(9), Some(Action::Key(KeyCode::N0)));
}

#[test]
fn layer_macro_mt_inside_layer_produces_modtap() {
    let l: Layer<()> = layer!(Base, MT!(LSFT, A), B, C);
    assert_eq!(
        l.get(0),
        Some(Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        })
    );
    assert_eq!(l.get(1), Some(Action::Key(KeyCode::B)));
}

#[test]
fn layer_macro_lt_inside_layer_produces_layertap() {
    let l: Layer<()> = layer!(Base, LT!(1, SPC), A);
    assert_eq!(
        l.get(0),
        Some(Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space,
        })
    );
}

// ── keymap! ───────────────────────────────────────────────────────────────────

#[test]
fn keymap_macro_produces_correct_layer_stack_type() {
    let stack: LayerStack<2, 4, ()> = keymap! {
        layer!(Base, A, B, C, D),
        layer!(Fn,   F1, F2, F3, F4),
    };
    assert_eq!(stack.state(), 0);
}

#[test]
fn keymap_macro_resolves_base_layer_key() {
    let stack: LayerStack<2, 3, ()> = keymap! {
        layer!(Base, A, B, C),
        layer!(Fn,   TRNS, TRNS, TRNS),
    };
    let action = stack.resolve(KeyPos::new(0, 0), 3);
    assert_eq!(action, Action::Key(KeyCode::A));
}

#[test]
fn keymap_macro_transparent_falls_through_to_base() {
    let mut stack: LayerStack<2, 3, ()> = keymap! {
        layer!(Base, A,    B,    C),
        layer!(Fn,   TRNS, F1,   TRNS),
    };
    stack.apply(LayerOp::On(1));
    let action = stack.resolve(KeyPos::new(0, 0), 3);
    assert_eq!(action, Action::Key(KeyCode::A));
    let action2 = stack.resolve(KeyPos::new(0, 1), 3);
    assert_eq!(action2, Action::Key(KeyCode::F1));
}

#[test]
fn keymap_macro_single_layer() {
    let stack: LayerStack<1, 2, ()> = keymap! {
        layer!(Base, SPC, ENT),
    };
    assert_eq!(stack.default_state(), 1);
}

// ── steno! ───────────────────────────────────────────────────────────────────

#[test]
fn steno_macro_produces_correct_layer_stack_type() {
    let stack: LayerStack<1, 12, ()> = steno! {
        layer!(Base, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11),
    };
    assert_eq!(stack.state(), 0);
}

#[test]
fn steno_macro_resolves_steno_actions_correctly() {
    let stack: LayerStack<1, 3, ()> = steno! {
        layer!(Base, 0, 1, 2),
    };
    assert_eq!(stack.resolve(KeyPos::new(0, 0), 3), Action::Steno(0));
    assert_eq!(stack.resolve(KeyPos::new(0, 1), 3), Action::Steno(1));
    assert_eq!(stack.resolve(KeyPos::new(0, 2), 3), Action::Steno(2));
}

// ── layer name resolution ─────────────────────────────────────────────────────

#[test]
fn lt_inside_keymap_resolves_name_to_index() {
    let stack: LayerStack<2, 3, ()> = keymap! {
        layer!(Base, A, B, LT!(Fn, SPC)),
        layer!(Fn,   F1, F2, TRNS),
    };
    assert_eq!(
        stack.resolve(KeyPos::new(0, 2), 3),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space
        }
    );
}

#[test]
fn lt_forward_reference_resolves_correctly() {
    let stack: LayerStack<3, 3, ()> = keymap! {
        layer!(Base,   A,    B,    LT!(Nav, SPC)),
        layer!(Sym,    N1,   N2,   LT!(Nav, ENT)),
        layer!(Nav,    UP,   DOWN, TRNS),
    };
    assert_eq!(
        stack.resolve(KeyPos::new(0, 2), 3),
        Action::LayerTap {
            layer: 2,
            tap: KeyCode::Space
        }
    );

    let mut stack2: LayerStack<3, 3, ()> = keymap! {
        layer!(Base,   A,    B,    LT!(Nav, SPC)),
        layer!(Sym,    N1,   N2,   LT!(Nav, ENT)),
        layer!(Nav,    UP,   DOWN, TRNS),
    };
    stack2.apply(LayerOp::On(1));
    assert_eq!(
        stack2.resolve(KeyPos::new(0, 2), 3),
        Action::LayerTap {
            layer: 2,
            tap: KeyCode::Enter
        }
    );
}

#[test]
fn lt_numeric_index_still_works_inside_keymap() {
    let stack: LayerStack<2, 2, ()> = keymap! {
        layer!(Base, LT!(1, A), B),
        layer!(Fn,   TRNS,      F1),
    };
    assert_eq!(
        stack.resolve(KeyPos::new(0, 0), 2),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::A
        }
    );
}

#[test]
fn lt_name_resolves_same_as_numeric_equivalent() {
    let by_name: LayerStack<2, 2, ()> = keymap! {
        layer!(Base, LT!(Fn, SPC), A),
        layer!(Fn,   TRNS,         F1),
    };
    let by_index: LayerStack<2, 2, ()> = keymap! {
        layer!(Base, LT!(1, SPC), A),
        layer!(Fn,   TRNS,        F1),
    };
    assert_eq!(
        by_name.resolve(KeyPos::new(0, 0), 2),
        by_index.resolve(KeyPos::new(0, 0), 2),
    );
}

#[test]
fn lt_self_referential_layer_resolves_to_own_index() {
    let mut stack: LayerStack<2, 2, ()> = keymap! {
        layer!(Base, A,              LT!(Fn, SPC)),
        layer!(Fn,   LT!(Fn, ESC),   TRNS),
    };
    stack.apply(LayerOp::On(1));
    assert_eq!(
        stack.resolve(KeyPos::new(0, 0), 2),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Escape
        }
    );
}

#[test]
fn lt_base_layer_is_index_zero() {
    let mut stack: LayerStack<2, 2, ()> = keymap! {
        layer!(Base, A,              B),
        layer!(Fn,   LT!(Base, ESC), TRNS),
    };
    stack.apply(LayerOp::On(1));
    assert_eq!(
        stack.resolve(KeyPos::new(0, 0), 2),
        Action::LayerTap {
            layer: 0,
            tap: KeyCode::Escape
        }
    );
}

#[test]
fn keymap_with_many_named_layers_all_resolve() {
    let stack: LayerStack<4, 4, ()> = keymap! {
        layer!(Base,   A,             B,             LT!(Sym, SPC),  LT!(Nav, ENT)),
        layer!(Sym,    N1,            N2,            TRNS,           LT!(Fn,  TAB)),
        layer!(Nav,    UP,            DOWN,          LT!(Sym, LEFT), TRNS),
        layer!(Fn,     F1,            F2,            F3,             TRNS),
    };
    assert_eq!(
        stack.resolve(KeyPos::new(0, 2), 4),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space
        }
    );
    assert_eq!(
        stack.resolve(KeyPos::new(0, 3), 4),
        Action::LayerTap {
            layer: 2,
            tap: KeyCode::Enter
        }
    );

    let mut sym_active: LayerStack<4, 4, ()> = keymap! {
        layer!(Base,   A,             B,             LT!(Sym, SPC),  LT!(Nav, ENT)),
        layer!(Sym,    N1,            N2,            TRNS,           LT!(Fn,  TAB)),
        layer!(Nav,    UP,            DOWN,          LT!(Sym, LEFT), TRNS),
        layer!(Fn,     F1,            F2,            F3,             TRNS),
    };
    sym_active.apply(LayerOp::On(1));
    assert_eq!(
        sym_active.resolve(KeyPos::new(0, 3), 4),
        Action::LayerTap {
            layer: 3,
            tap: KeyCode::Tab
        }
    );

    let mut nav_active: LayerStack<4, 4, ()> = keymap! {
        layer!(Base,   A,             B,             LT!(Sym, SPC),  LT!(Nav, ENT)),
        layer!(Sym,    N1,            N2,            TRNS,           LT!(Fn,  TAB)),
        layer!(Nav,    UP,            DOWN,          LT!(Sym, LEFT), TRNS),
        layer!(Fn,     F1,            F2,            F3,             TRNS),
    };
    nav_active.apply(LayerOp::On(2));
    assert_eq!(
        nav_active.resolve(KeyPos::new(0, 2), 4),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Left
        }
    );
}

// ── MT! ───────────────────────────────────────────────────────────────────────

#[test]
fn mt_macro_lshift_a() {
    let action: Action<()> = MT!(LSFT, A);
    assert_eq!(
        action,
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        }
    );
}

#[test]
fn mt_macro_lctrl_escape() {
    let action: Action<()> = MT!(LCTL, ESC);
    assert_eq!(
        action,
        Action::ModTap {
            hold: ModifierKey::LCTRL,
            tap: KeyCode::Escape,
        }
    );
}

#[test]
fn mt_macro_all_modifiers_are_recognized() {
    assert_eq!(
        MT!(LCTL, A),
        Action::<()>::ModTap {
            hold: ModifierKey::LCTRL,
            tap: KeyCode::A
        }
    );
    assert_eq!(
        MT!(RCTL, A),
        Action::<()>::ModTap {
            hold: ModifierKey::RCTRL,
            tap: KeyCode::A
        }
    );
    assert_eq!(
        MT!(LSFT, A),
        Action::<()>::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A
        }
    );
    assert_eq!(
        MT!(RSFT, A),
        Action::<()>::ModTap {
            hold: ModifierKey::RSHIFT,
            tap: KeyCode::A
        }
    );
    assert_eq!(
        MT!(LALT, A),
        Action::<()>::ModTap {
            hold: ModifierKey::LALT,
            tap: KeyCode::A
        }
    );
    assert_eq!(
        MT!(RALT, A),
        Action::<()>::ModTap {
            hold: ModifierKey::RALT,
            tap: KeyCode::A
        }
    );
    assert_eq!(
        MT!(LGUI, A),
        Action::<()>::ModTap {
            hold: ModifierKey::LGUI,
            tap: KeyCode::A
        }
    );
    assert_eq!(
        MT!(RGUI, A),
        Action::<()>::ModTap {
            hold: ModifierKey::RGUI,
            tap: KeyCode::A
        }
    );
}

#[test]
fn mt_macro_aliases_work() {
    let a: Action<()> = MT!(LCTRL, A);
    let b: Action<()> = MT!(LCTL, A);
    assert_eq!(a, b);
}

// ── LT! ───────────────────────────────────────────────────────────────────────

#[test]
fn lt_macro_layer_1_space() {
    let action: Action<()> = LT!(1, SPC);
    assert_eq!(
        action,
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Space,
        }
    );
}

#[test]
fn lt_macro_layer_0_enter() {
    let action: Action<()> = LT!(0, ENT);
    assert_eq!(
        action,
        Action::LayerTap {
            layer: 0,
            tap: KeyCode::Enter,
        }
    );
}

#[test]
fn lt_macro_layer_255_is_valid() {
    let action: Action<()> = LT!(255, A);
    assert_eq!(
        action,
        Action::LayerTap {
            layer: 255,
            tap: KeyCode::A,
        }
    );
}

#[test]
fn mt_and_lt_need_tap_hold() {
    let mt: Action<()> = MT!(LSFT, A);
    let lt: Action<()> = LT!(1, SPC);
    assert!(mt.needs_tap_hold());
    assert!(lt.needs_tap_hold());
}

// ── compile-fail: error diagnostics ──────────────────────────────────────────

#[test]
fn compile_fail_cases() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
