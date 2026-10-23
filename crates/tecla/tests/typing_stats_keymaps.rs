#![allow(
    clippy::tests_outside_test_module,
    missing_docs,
    reason = "integration tests check the production typing keymaps"
)]
#![expect(clippy::expect_used, reason = "test assertions")]

mod common;

#[path = "../../../keyboards/tecla/nos75/keymap.rs"]
mod nos75;

use tecla::action::{Action, KeyCode, LayerOp};
use tecla::devices::keyboard::tapping::{TapHoldConfig, TapHoldMode};
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::keymap::LayerStack;
use tecla::pipeline::report::{HidReport, KeyboardReport};
use tecla_macros::keymap;

#[test]
fn voyager_enter_nav_thumb_and_w_types_wpm_without_changing_base_w() {
    let mut map: LayerStack<3, 84> =
        include!("../../../keyboards/tecla/voyager/keymaps/default/keymap.rs");
    let w = KeyPos::new(1, 3);
    assert_eq!(map.resolve(w, 7), Action::Key(KeyCode::W));
    assert!(matches!(
        map.resolve(KeyPos::new(5, 0), 7),
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::Enter
        }
    ));
    map.apply(LayerOp::Momentary(1));
    assert_eq!(map.resolve(w, 7), Action::TypeWpm);
    assert_eq!(map.resolve(KeyPos::new(1, 2), 7), Action::Key(KeyCode::N1));
    map.apply(LayerOp::Off(1));
    assert_eq!(map.resolve(w, 7), Action::Key(KeyCode::W));
}

#[test]
fn nos75_fn_and_w_types_wpm_without_changing_base_w() {
    let mut map = nos75::layers();
    let w = KeyPos::new(2, 2);
    assert_eq!(map.resolve(w, nos75::COLS), Action::Key(KeyCode::W));
    assert_eq!(
        map.resolve(KeyPos::new(5, 10), nos75::COLS),
        Action::Layer(LayerOp::Momentary(1))
    );
    map.apply(LayerOp::Momentary(1));
    assert_eq!(map.resolve(w, nos75::COLS), Action::TypeWpm);
    assert_eq!(
        map.resolve(KeyPos::new(2, 1), nos75::COLS),
        Action::Key(KeyCode::Q)
    );
    map.apply(LayerOp::Off(1));
    assert_eq!(map.resolve(w, nos75::COLS), Action::Key(KeyCode::W));
    assert_eq!(nos75::ROWS * nos75::COLS, nos75::KEYS);
}

fn physical_fn_w<const R: usize, const C: usize, const L: usize, const K: usize>(
    mut proc: KeyboardProcessor<R, C, L, K>,
    function: KeyPos,
    w: KeyPos,
) {
    for timestamp in (0..=1_000).step_by(100) {
        common::collect(&mut proc, KeyEvent::press(w, timestamp), timestamp);
        common::collect(
            &mut proc,
            KeyEvent::release(w, timestamp + 1),
            timestamp + 1,
        );
    }
    let before = proc.typing_stats();
    let stats = before.expect("board typing stats enabled");
    assert_eq!(stats.characters, 11);
    assert_eq!(stats.active_wpm, 120);

    let mut output = common::collect(&mut proc, KeyEvent::press(function, 4_000), 4_000);
    // Let the Voyager Enter/Nav dual-role thumb settle into its actual hold.
    proc.tick(4_201, &mut |report| output.push(report));
    output.extend(common::collect(&mut proc, KeyEvent::press(w, 4_210), 4_210));
    output.extend(common::collect(
        &mut proc,
        KeyEvent::release(w, 4_211),
        4_211,
    ));
    output.extend(common::collect(
        &mut proc,
        KeyEvent::release(function, 4_212),
        4_212,
    ));

    let attacks: Vec<_> = output
        .iter()
        .filter_map(|report| match report {
            HidReport::Keyboard(report) => Some(report),
            _ => None,
        })
        .flat_map(|report| {
            report
                .keycodes
                .iter()
                .copied()
                .filter(|usage| *usage != 0)
                .map(move |usage| (report.modifiers, usage))
        })
        .collect();
    // This is the complete HID text "120 WPM", with no W/Enter leaked by Fn.
    assert_eq!(
        attacks,
        [
            (0, KeyCode::N1.usage_id()),
            (0, KeyCode::N2.usage_id()),
            (0, KeyCode::N0.usage_id()),
            (0, KeyCode::Space.usage_id()),
            (0x02, KeyCode::W.usage_id()),
            (0x02, KeyCode::P.usage_id()),
            (0x02, KeyCode::M.usage_id()),
        ]
    );
    assert_eq!(
        proc.typing_stats(),
        before,
        "shortcut and readout are excluded"
    );
    assert_eq!(
        output.last(),
        Some(&HidReport::Keyboard(KeyboardReport::new()))
    );

    let normal_w = common::collect(&mut proc, KeyEvent::press(w, 5_000), 5_000);
    assert!(normal_w.iter().any(|report| matches!(
        report,
        HidReport::Keyboard(report) if report.keycodes.contains(&KeyCode::W.usage_id())
    )));
    assert_eq!(proc.typing_stats().expect("enabled").characters, 12);
    common::collect(&mut proc, KeyEvent::release(w, 5_001), 5_001);
}

fn voyager_processor() -> KeyboardProcessor<12, 7, 3, 84> {
    const COMBOS: tecla::devices::keyboard::combo::ComboConfig =
        include!("../../../keyboards/tecla/voyager/keymaps/default/combos.rs");
    let map = include!("../../../keyboards/tecla/voyager/keymaps/default/keymap.rs");
    let tap_hold = TapHoldConfig {
        tapping_term_ms: 200,
        mode: TapHoldMode::PermissiveHold,
        quick_tap_ms: 150,
        require_prior_idle_ms: 175,
    };
    KeyboardProcessor::<12, 7, 3, 84>::new(map, tap_hold)
        .with_combos(COMBOS)
        .with_typing_stats()
}

#[test]
fn voyager_physical_nav_hold_then_w_types_the_average_without_counting_it() {
    physical_fn_w(voyager_processor(), KeyPos::new(5, 0), KeyPos::new(1, 3));
}

#[test]
fn voyager_nav_pressed_during_fast_typing_still_taps_enter() {
    for first_press in [0, 1_000] {
        let mut proc = voyager_processor();
        let w = KeyPos::new(1, 3);
        let thumb = KeyPos::new(5, 0);
        common::collect(&mut proc, KeyEvent::press(w, first_press), first_press);
        common::collect(
            &mut proc,
            KeyEvent::release(w, first_press + 1),
            first_press + 1,
        );
        let mut reports = common::collect(
            &mut proc,
            KeyEvent::press(thumb, first_press + 50),
            first_press + 50,
        );
        proc.tick(first_press + 251, &mut |report| reports.push(report));
        assert!(reports.is_empty(), "recent typing prevents a Nav hold");
        reports.extend(common::collect(
            &mut proc,
            KeyEvent::release(thumb, first_press + 252),
            first_press + 252,
        ));
        assert!(reports.iter().any(|report| matches!(
            report,
            HidReport::Keyboard(report) if report.keycodes.contains(&KeyCode::Enter.usage_id())
        )));
        assert_eq!(proc.typing_stats().expect("enabled").characters, 2);
    }
}

#[test]
fn nos75_physical_fn_then_w_types_the_average_without_counting_it() {
    let processor = KeyboardProcessor::<
        { nos75::ROWS },
        { nos75::COLS },
        { nos75::LAYERS },
        { nos75::KEYS },
    >::new(nos75::layers(), TapHoldConfig::new(200))
    .with_typing_stats();
    physical_fn_w(processor, KeyPos::new(5, 10), KeyPos::new(2, 2));
}
