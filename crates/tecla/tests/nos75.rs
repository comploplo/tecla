#![allow(
    clippy::tests_outside_test_module,
    missing_docs,
    reason = "integration tests exercise the production board keymap and scan sequence on a host"
)]
#![expect(clippy::expect_used, reason = "test assertions")]
#![expect(clippy::panic, reason = "test assertions")]

mod common;

#[path = "../../../keyboards/tecla/nos75/keymap.rs"]
mod keymap;
#[path = "../../tecla-stm32f0/src/scan.rs"]
mod scan;

use std::cell::RefCell;

use common::{collect, pos, press, release};
use tecla::action::{Action, ConsumerCode, KeyCode, LayerOp};
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::report::{HidReport, KeyboardReport};

type Processor =
    KeyboardProcessor<{ keymap::ROWS }, { keymap::COLS }, { keymap::LAYERS }, { keymap::KEYS }>;

fn processor() -> Processor {
    Processor::new(keymap::layers(), TapHoldConfig::default())
}

fn keyboard(reports: &[HidReport]) -> KeyboardReport {
    reports
        .iter()
        .find_map(|report| match report {
            HidReport::Keyboard(report) => Some(*report),
            _ => None,
        })
        .expect("keyboard report")
}

fn consumer(reports: &[HidReport]) -> Option<ConsumerCode> {
    reports
        .iter()
        .find_map(|report| match report {
            HidReport::Consumer(report) => Some(report.keycode),
            _ => None,
        })
        .expect("consumer report")
}

#[test]
fn all_81_production_positions_survive_dense_102_key_storage() {
    // Wired positions in the official production VIA definition, in matrix order.
    const WIRED: [u32; 6] = [0x01fff, 0x1bfff, 0x17fff, 0x02fff, 0x06ffd, 0x0f647];
    assert_eq!(keymap::BASE.len(), 102);
    assert_eq!(WIRED.iter().map(|row| row.count_ones()).sum::<u32>(), 81);
    let mut key_count = 0;
    let mut fn_count = 0;
    for (row, wired) in WIRED.iter().enumerate() {
        for col in 0..keymap::COLS {
            let action = keymap::BASE[row * keymap::COLS + col];
            assert_eq!(action != Action::NoOp, wired & (1 << col) != 0);
            match action {
                Action::Key(key) => {
                    let mut proc = processor();
                    let key_pos = pos(
                        row.try_into().expect("six rows fit in u8"),
                        col.try_into().expect("17 columns fit in u8"),
                    );
                    let pressed = keyboard(&collect(&mut proc, press(key_pos, 0), 0));
                    if key.is_modifier() {
                        assert_eq!(pressed.modifiers, 1 << (key.usage_id() - 0xe0));
                        assert_eq!(pressed.keycodes, [0; 6]);
                    } else {
                        assert_eq!(pressed.modifiers, 0);
                        assert!(pressed.keycodes.contains(&key.usage_id()));
                    }
                    let released = keyboard(&collect(&mut proc, release(key_pos, 1), 1));
                    assert_eq!(released, KeyboardReport::new());
                    key_count += 1;
                }
                Action::Layer(LayerOp::Momentary(1)) => fn_count += 1,
                Action::NoOp => {}
                other => panic!("unexpected base action at {row},{col}: {other:?}"),
            }
        }
    }
    assert_eq!(key_count, 80);
    assert_eq!(fn_count, 1);
}

#[test]
fn bottom_row_arrows_and_caps_control_press_and_release() {
    for (row, col, key) in [
        (5, 14, KeyCode::Down),
        (5, 15, KeyCode::Right),
        (3, 0, KeyCode::LCtrl),
    ] {
        assert_eq!(
            keymap::BASE[usize::from(row) * keymap::COLS + usize::from(col)],
            Action::Key(key)
        );
        let mut proc = processor();
        let pressed = keyboard(&collect(&mut proc, press(pos(row, col), 0), 0));
        if key == KeyCode::LCtrl {
            assert_eq!(pressed.modifiers, 1);
            assert_eq!(pressed.keycodes, [0; 6]);
        } else {
            assert!(pressed.keycodes.contains(&key.usage_id()));
        }
        assert_eq!(
            keyboard(&collect(&mut proc, release(pos(row, col), 1), 1)),
            KeyboardReport::new()
        );
    }
}

#[test]
fn fn_media_release_uses_pressed_action_after_fn_is_released() {
    for (col, media, base) in [
        (1, ConsumerCode::BrightnessDown, KeyCode::F1),
        (2, ConsumerCode::BrightnessUp, KeyCode::F2),
        (7, ConsumerCode::PrevTrack, KeyCode::F7),
        (8, ConsumerCode::Play, KeyCode::F8),
        (9, ConsumerCode::NextTrack, KeyCode::F9),
        (10, ConsumerCode::Mute, KeyCode::F10),
        (11, ConsumerCode::VolumeDown, KeyCode::F11),
        (12, ConsumerCode::VolumeUp, KeyCode::F12),
    ] {
        let mut proc = processor();
        collect(&mut proc, press(pos(5, 10), 0), 0);
        assert_eq!(
            consumer(&collect(&mut proc, press(pos(0, col), 1), 1)),
            Some(media)
        );
        collect(&mut proc, release(pos(5, 10), 2), 2);
        assert_eq!(
            consumer(&collect(&mut proc, release(pos(0, col), 3), 3)),
            None
        );
        assert!(keyboard(&collect(&mut proc, press(pos(0, col), 4), 4))
            .keycodes
            .contains(&base.usage_id()));
        assert_eq!(
            keyboard(&collect(&mut proc, release(pos(0, col), 5), 5)),
            KeyboardReport::new()
        );
    }
}

#[test]
fn scan_selects_only_one_row_low_and_reads_column_16_after_settling() {
    #[derive(Default)]
    struct Pins {
        active: Option<usize>,
        settled: bool,
        reads: usize,
        rows_completed: usize,
        release_pending: bool,
    }
    let pins = RefCell::new(Pins::default());
    let state = scan::scan(
        |row, selected_low| {
            let mut pins = pins.borrow_mut();
            if selected_low {
                assert_eq!(pins.active, None, "other rows must remain released");
                assert!(!pins.release_pending);
                assert_eq!(row, pins.rows_completed);
                pins.active = Some(row);
                pins.settled = false;
                pins.reads = 0;
            } else {
                assert_eq!(pins.active, Some(row));
                assert_eq!(pins.reads, 17);
                pins.active = None;
                pins.release_pending = true;
            }
        },
        |col| {
            let mut pins = pins.borrow_mut();
            let row = pins.active.expect("read requires one selected-low row");
            assert!(pins.settled);
            assert_eq!(col, pins.reads);
            pins.reads += 1;
            col == row || (row == 5 && col == 16)
        },
        |micros| {
            let mut pins = pins.borrow_mut();
            if pins.active.is_some() {
                assert_eq!(micros, 10);
                assert_eq!(pins.reads, 0);
                assert!(!pins.settled);
                pins.settled = true;
            } else {
                assert_eq!(micros, 3);
                assert!(pins.release_pending);
                pins.release_pending = false;
                pins.rows_completed += 1;
            }
        },
    );
    let pins = pins.borrow();
    assert_eq!(pins.active, None);
    assert!(!pins.release_pending);
    assert_eq!(pins.rows_completed, 6);
    for row in 0..scan::ROWS {
        for col in 0..scan::COLS {
            assert_eq!(
                state.get(row, col),
                Some(col == row || (row == 5 && col == 16))
            );
        }
    }
    assert_eq!(state.get(5, 16), Some(true));
}

#[test]
fn fn_lighting_controls_keep_the_base_typing_actions() {
    use tecla::rgb::RgbCommand;
    for (row, col, command) in [
        (2, 11, RgbCommand::HueDown),
        (2, 12, RgbCommand::HueUp),
        (2, 13, RgbCommand::Toggle),
        (3, 10, RgbCommand::SatDown),
        (3, 11, RgbCommand::SatUp),
        (4, 9, RgbCommand::SpeedDown),
        (4, 10, RgbCommand::SpeedUp),
        (4, 14, RgbCommand::BrightUp),
        (5, 13, RgbCommand::Prev),
        (5, 14, RgbCommand::BrightDown),
        (5, 15, RgbCommand::Next),
    ] {
        let mut proc = processor();
        collect(&mut proc, press(pos(5, 10), 0), 0);
        assert!(collect(&mut proc, press(pos(row, col), 1), 1).is_empty());
        assert_eq!(proc.pending_rgb_command(), Some(command));
        collect(&mut proc, release(pos(row, col), 2), 2);
        collect(&mut proc, release(pos(5, 10), 3), 3);
        assert!(collect(&mut proc, press(pos(row, col), 4), 4)
            .iter()
            .any(|report| matches!(report, HidReport::Keyboard(_))));
        assert_eq!(
            keyboard(&collect(&mut proc, release(pos(row, col), 5), 5)),
            KeyboardReport::new()
        );
    }
}
