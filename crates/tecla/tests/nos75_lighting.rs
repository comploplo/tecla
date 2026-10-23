#![allow(
    clippy::tests_outside_test_module,
    missing_docs,
    reason = "host integration tests exercise the board lighting model"
)]
#![expect(clippy::expect_used, reason = "test assertions")]

#[path = "../../../keyboards/tecla/nos75/lighting.rs"]
mod lighting;

use lighting::{
    Lighting, CHANNEL_BUDGET, LED_COUNT, LED_POSITIONS, MATRIX_TO_LED, MODE_COUNT, NO_LED,
};
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::rgb::{Rgb, RgbCommand};

// Independent fixture from the actual g_led_config table, not prototype order.
const PRODUCTION_MAP: [[u8; 17]; 6] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 255, 255, 255, 255],
    [
        14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 255, 13, 43,
    ],
    [
        29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 28, 255, 57,
    ],
    [
        44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 255, 56, 255, 255, 255,
    ],
    [
        58, 255, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 255, 69, 70, 255, 255,
    ],
    [
        71, 72, 73, 255, 255, 255, 74, 255, 255, 75, 76, 255, 77, 78, 79, 80, 255,
    ],
];

fn select(model: &mut Lighting, index: u8) {
    for _ in 0..MODE_COUNT {
        if model.mode() == index {
            return;
        }
        model.command(RgbCommand::Next);
    }
    assert_eq!(model.mode(), index);
}

fn sum(frame: &[Rgb]) -> u32 {
    frame
        .iter()
        .map(|rgb| u32::from(rgb.r) + u32::from(rgb.g) + u32::from(rgb.b))
        .sum()
}

fn key(row: usize, col: usize) -> KeyPos {
    KeyPos::new(
        row.try_into().expect("row fits"),
        col.try_into().expect("column fits"),
    )
}

#[test]
fn production_map_contains_all_81_keys_and_eight_separate_side_leds() {
    assert_eq!(MATRIX_TO_LED, PRODUCTION_MAP);
    assert_eq!(LED_COUNT, 89);
    let mut seen = [false; LED_COUNT];
    for (row, indices) in PRODUCTION_MAP.iter().enumerate() {
        for (col, &index) in indices.iter().enumerate() {
            let actual = lighting::led_index(key(row, col));
            if index == NO_LED {
                assert_eq!(actual, None);
            } else {
                let index = usize::from(index);
                assert_eq!(actual, Some(index));
                assert!(index < 81);
                assert!(!seen[index], "each key has one distinct physical LED");
                seen[index] = true;
            }
        }
    }
    assert!(seen[..81].iter().all(|&seen| seen));
    assert!(seen[81..].iter().all(|&seen| !seen));
    assert_eq!(lighting::led_index(KeyPos::new(1, 15)), Some(13));
    assert_eq!(lighting::led_index(KeyPos::new(1, 14)), None);
    assert_eq!(lighting::led_index(KeyPos::new(6, 0)), None);
    assert_eq!(lighting::led_index(KeyPos::new(0, 17)), None);
    assert_eq!(LED_POSITIONS[1], (12, 0));
    assert_eq!(LED_POSITIONS[80], (150, 50));
    assert!(LED_POSITIONS[81..]
        .iter()
        .all(|&position| position == (0, 0)));
}

#[test]
fn default_and_mode_cycle_keep_only_one_effect_and_reset_reactive_history() {
    let mut model = Lighting::default();
    assert_eq!(model.mode(), 1);
    assert_eq!(model.brightness(), 64);
    assert!(model.enabled());
    for expected in [2, 3, 4, 5, 6, 0, 1] {
        model.command(RgbCommand::Next);
        assert_eq!(model.mode(), expected);
    }
    for expected in [0, 6, 5, 4, 3, 2, 1] {
        model.command(RgbCommand::Prev);
        assert_eq!(model.mode(), expected);
    }
    select(&mut model, 3);
    model.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 1));
    assert!(model.tick(1).iter().any(|&rgb| rgb != Rgb::BLACK));
    for _ in 0..MODE_COUNT {
        model.command(RgbCommand::Next);
    }
    assert_eq!(model.mode(), 3);
    assert!(model.tick(2).iter().all(|&rgb| rgb == Rgb::BLACK));
    assert!(core::mem::size_of::<Lighting>() <= 600);
    println!(
        "Lighting model host size: {} bytes",
        core::mem::size_of::<Lighting>()
    );
}

#[test]
fn each_physical_press_reaches_its_production_led_in_reactive_and_heatmap() {
    for mode in [3, 5] {
        for (row, indices) in PRODUCTION_MAP.iter().enumerate() {
            for (col, &index) in indices.iter().enumerate() {
                if index == NO_LED {
                    continue;
                }
                let mut model = Lighting::new();
                select(&mut model, mode);
                model.on_key_event(KeyEvent::press(key(row, col), 1));
                let frame = model.tick(1);
                assert_eq!(frame.len(), LED_COUNT);
                for (slot, &rgb) in frame.iter().enumerate() {
                    assert_eq!(rgb != Rgb::BLACK, slot == usize::from(index));
                }
            }
        }
    }
}

#[test]
fn ripple_uses_physical_key_coordinates_including_the_rightmost_keys() {
    for (row, indices) in PRODUCTION_MAP.iter().enumerate() {
        for (col, &index) in indices.iter().enumerate() {
            if index == NO_LED {
                continue;
            }
            let mut model = Lighting::new();
            select(&mut model, 6);
            model.on_key_event(KeyEvent::press(key(row, col), 1));
            let frame = model.tick(1);
            assert_ne!(frame[usize::from(index)], Rgb::BLACK);
        }
    }
}

#[test]
fn releases_holes_and_disabled_output_do_not_start_reactive_effects() {
    let mut model = Lighting::new();
    select(&mut model, 3);
    model.on_key_event(KeyEvent::release(KeyPos::new(0, 0), 1));
    model.on_key_event(KeyEvent::press(KeyPos::new(1, 14), 2));
    model.on_key_event(KeyEvent::press(KeyPos::new(255, 255), 3));
    assert!(model.tick(3).iter().all(|&rgb| rgb == Rgb::BLACK));
    model.command(RgbCommand::Toggle);
    assert!(!model.enabled());
    model.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 4));
    assert!(model.tick(4).iter().all(|&rgb| rgb == Rgb::BLACK));
    model.command(RgbCommand::Toggle);
    assert!(model.enabled());
    assert!(model.tick(5).iter().all(|&rgb| rgb == Rgb::BLACK));
}

fn colored_frame(mode: u8, hue_up: bool) -> Vec<Rgb> {
    let mut model = Lighting::new();
    select(&mut model, mode);
    if hue_up {
        model.command(RgbCommand::HueUp);
    }
    model.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 1));
    model.tick(1024).to_vec()
}

#[test]
fn hsv_forwarding_reaches_all_base_color_effects_and_rainbow_speed_is_live() {
    for mode in [0, 2, 3] {
        assert_ne!(colored_frame(mode, false), colored_frame(mode, true));
    }
    let mut model = Lighting::new();
    let original = model.base_color();
    model.command(RgbCommand::HueUp);
    assert_eq!(model.base_color().h, original.h.wrapping_add(8));
    model.command(RgbCommand::HueDown);
    assert_eq!(model.base_color(), original);
    model.command(RgbCommand::SatDown);
    assert_eq!(model.base_color().s, original.s - 8);
    model.command(RgbCommand::SatUp);
    assert_eq!(model.base_color(), original);
    let first = model.tick(4096).to_vec();
    let speed = model.speed();
    model.command(RgbCommand::SpeedDown);
    assert_eq!(model.speed(), speed - 8);
    assert_ne!(model.tick(4096), first);
    for _ in 0..32 {
        model.command(RgbCommand::SpeedDown);
    }
    assert_eq!(model.speed(), 0);
    let frozen = model.tick(1).to_vec();
    assert_eq!(model.tick(8192), frozen);
    model.command(RgbCommand::SpeedUp);
    assert_eq!(model.speed(), 8);
    assert_ne!(model.tick(8192), frozen);
}

#[test]
fn whole_frame_budget_is_enforced_after_brightness_for_all_modes() {
    let mut white = [Rgb::WHITE; LED_COUNT];
    lighting::limit_current(&mut white);
    assert!(sum(&white) <= CHANNEL_BUDGET);
    assert!(white.iter().all(|&rgb| rgb == white[0]));
    let mut below = [Rgb::new(1, 2, 3); LED_COUNT];
    let before = below;
    lighting::limit_current(&mut below);
    assert_eq!(below, before);
    lighting::limit_current(&mut []);
    for mode in 0..MODE_COUNT {
        let mut model = Lighting::new();
        select(&mut model, mode);
        for _ in 0..32 {
            model.command(RgbCommand::BrightUp);
        }
        assert_eq!(model.brightness(), 255);
        for (index, &position) in [KeyPos::new(0, 0), KeyPos::new(1, 15), KeyPos::new(5, 15)]
            .iter()
            .enumerate()
        {
            model.on_key_event(KeyEvent::press(
                position,
                index.try_into().expect("timestamp fits"),
            ));
        }
        for tick in 0..32 {
            assert!(sum(model.tick(tick * 128)) <= CHANNEL_BUDGET);
        }
        model.command(RgbCommand::Toggle);
        assert_eq!(sum(model.tick(8192)), 0);
    }
}

#[test]
fn low_brightness_is_not_needlessly_scaled_and_ambient_modes_light_side_leds() {
    let mut model = Lighting::new();
    select(&mut model, 0);
    for _ in 0..32 {
        model.command(RgbCommand::SatDown);
    }
    for _ in 0..3 {
        model.command(RgbCommand::BrightDown);
    }
    assert_eq!(model.brightness(), 13);
    assert!(model.tick(0).iter().all(|&rgb| rgb == Rgb::new(13, 13, 13)));
    model.command(RgbCommand::BrightUp);
    assert!(sum(model.tick(0)) > 13 * 3 * 89);
    for mode in [0, 1, 2, 4] {
        let mut model = Lighting::new();
        select(&mut model, mode);
        assert!(model.tick(0).iter().all(|&rgb| rgb != Rgb::BLACK));
    }
}
