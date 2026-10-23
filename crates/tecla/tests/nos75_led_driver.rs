//! Host verification of the NOS75 LED controller protocol and failure recovery.
#![cfg(feature = "embassy")]
#![allow(
    clippy::tests_outside_test_module,
    missing_docs,
    reason = "host integration tests exercise the board driver through a recording I2C bus"
)]
#![expect(clippy::panic, reason = "unexpected mock bus reads fail the test")]
#![expect(clippy::expect_used, reason = "test fixture indices fit in a byte")]

#[path = "../../tecla-stm32f0/src/is31fl3733.rs"]
mod driver;

use std::cell::RefCell;
use std::rc::Rc;

use driver::{LedError, Nos75Leds, CHANNEL_MAP, LED_COUNT};
use embassy_futures::block_on;
use embedded_hal::i2c::{ErrorKind, ErrorType};
use embedded_hal_async::{delay::DelayNs, i2c::I2c, i2c::Operation};
use tecla::rgb::{LedDriver, Rgb};

#[derive(Default)]
struct Trace {
    writes: Vec<(u8, Vec<u8>)>,
    fail_at: Option<usize>,
}

#[derive(Clone, Default)]
struct Bus(Rc<RefCell<Trace>>);

impl ErrorType for Bus {
    type Error = ErrorKind;
}

impl I2c for Bus {
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        for operation in operations {
            let Operation::Write(bytes) = operation else {
                panic!("LED driver must not issue reads");
            };
            let mut trace = self.0.borrow_mut();
            let index = trace.writes.len();
            trace.writes.push((address, bytes.to_vec()));
            if trace.fail_at == Some(index) {
                return Err(ErrorKind::Other);
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Delay(Vec<u32>);

impl DelayNs for Delay {
    async fn delay_ns(&mut self, ns: u32) {
        self.0.push(ns);
    }
}

fn registers(writes: &[(u8, Vec<u8>)]) -> [[[u8; 192]; 4]; 2] {
    let mut registers = [[[0xAA; 192]; 4]; 2];
    let mut pages = [0; 2];
    let mut unlocked = [false; 2];
    for (address, data) in writes {
        let chip = match address {
            0x50 => 0,
            0x53 => 1,
            _ => panic!("unexpected production I2C address"),
        };
        assert!((2..=17).contains(&data.len()));
        match data.as_slice() {
            [0xFE, 0xC5] => unlocked[chip] = true,
            [0xFD, page] => {
                assert!(unlocked[chip], "page changes require a preceding unlock");
                assert!(*page <= 3);
                pages[chip] = usize::from(*page);
                unlocked[chip] = false;
            }
            [start, values @ ..] => {
                let start = usize::from(*start);
                assert!(start + values.len() <= 192);
                registers[chip][pages[chip]][start..start + values.len()].copy_from_slice(values);
            }
            _ => panic!("unexpected register transaction"),
        }
    }
    registers
}

#[test]
fn init_clears_pwm_enables_only_production_channels_and_limits_global_current() {
    let bus = Bus::default();
    let mut driver = Nos75Leds::new(bus.clone());
    assert!(
        bus.0.borrow().writes.is_empty(),
        "construction performs no I/O"
    );
    let mut delay = Delay::default();
    assert_eq!(block_on(driver.init(&mut delay)), Ok(()));
    assert_eq!(delay.0, [1_000_000, 1_000_000]);
    let actual = registers(&bus.0.borrow().writes);
    let mut expected_control = [[0u8; 24]; 2];
    let mut seen = [[false; 192]; 2];
    for [driver, red, green, blue] in CHANNEL_MAP {
        assert!(driver < 2);
        for channel in [red, green, blue] {
            assert!(channel < 192);
            let chip = usize::from(driver);
            assert!(!seen[chip][usize::from(channel)], "channels must be unique");
            seen[chip][usize::from(channel)] = true;
            expected_control[chip][usize::from(channel >> 3)] |= 1 << (channel & 7);
        }
    }
    assert_eq!(seen.iter().flatten().filter(|&&on| on).count(), 89 * 3);
    for chip in 0..2 {
        assert_eq!(&actual[chip][0][..24], &expected_control[chip]);
        assert_eq!(actual[chip][1], [0; 192]);
        assert_eq!(actual[chip][3][0x0F], 0);
        assert_eq!(actual[chip][3][0x10], 0);
        assert_eq!(actual[chip][3][0x01], 0x3F);
        assert_eq!(actual[chip][3][0x00], 1, "both stock chips use sync=0");
    }
}

#[test]
fn frame_maps_first_last_key_and_side_colors_then_black_clears_every_pwm() {
    let bus = Bus::default();
    let mut driver = Nos75Leds::new(bus.clone());
    let mut colors = [Rgb::BLACK; LED_COUNT];
    colors[0] = Rgb::new(0x11, 0x22, 0x33);
    colors[80] = Rgb::new(0x44, 0x55, 0x66);
    colors[88] = Rgb::new(0x77, 0x88, 0x99);
    assert_eq!(block_on(driver.write(&colors)), Ok(()));
    let actual = registers(&bus.0.borrow().writes);
    assert_eq!(bus.0.borrow().writes.len(), 28);
    for (chip, channels, expected) in [
        (0, [11, 27, 43], [0x11, 0x22, 0x33]),
        (1, [151, 167, 183], [0x44, 0x55, 0x66]),
        (1, [7, 23, 39], [0x77, 0x88, 0x99]),
    ] {
        for (channel, value) in channels.into_iter().zip(expected) {
            assert_eq!(actual[chip][1][channel], value);
        }
    }
    assert_eq!(actual[0][1].iter().filter(|&&v| v != 0).count(), 3);
    assert_eq!(actual[1][1].iter().filter(|&&v| v != 0).count(), 6);
    assert_eq!(block_on(driver.write(&[Rgb::BLACK; LED_COUNT])), Ok(()));
    let cleared = registers(&bus.0.borrow().writes);
    assert_eq!(cleared[0][1], [0; 192]);
    assert_eq!(cleared[1][1], [0; 192]);
}

#[test]
fn every_key_and_side_led_writes_its_three_channels_without_lighting_unmapped_outputs() {
    let bus = Bus::default();
    let mut driver = Nos75Leds::new(bus.clone());
    let colors = core::array::from_fn::<_, LED_COUNT, _>(|index| {
        let value = u8::try_from(index + 1).expect("89 LEDs fit in one byte");
        Rgb::new(value, value + 89, 255 - value)
    });
    assert_eq!(block_on(driver.write(&colors)), Ok(()));
    let actual = registers(&bus.0.borrow().writes);
    let mut expected = [[0; 192]; 2];
    for ([chip, red, green, blue], color) in CHANNEL_MAP.iter().zip(colors) {
        let channels = &mut expected[usize::from(*chip)];
        channels[usize::from(*red)] = color.r;
        channels[usize::from(*green)] = color.g;
        channels[usize::from(*blue)] = color.b;
    }
    for chip in 0..2 {
        assert_eq!(actual[chip][1], expected[chip]);
    }
}

#[test]
fn invalid_frame_lengths_never_touch_the_bus() {
    let bus = Bus::default();
    let mut driver = Nos75Leds::new(bus.clone());
    for colors in [
        &[][..],
        &[Rgb::BLACK; LED_COUNT - 1][..],
        &[Rgb::BLACK; LED_COUNT + 1][..],
    ] {
        assert_eq!(
            block_on(driver.write(colors)),
            Err(LedError::InvalidLedCount)
        );
    }
    assert!(bus.0.borrow().writes.is_empty());
}

#[test]
fn bus_failures_stop_page_and_pwm_writes_and_next_frame_reselects_page() {
    for fail_at in [0, 1, 2, 13, 14, 15, 16, 27] {
        let bus = Bus::default();
        bus.0.borrow_mut().fail_at = Some(fail_at);
        let mut driver = Nos75Leds::new(bus.clone());
        assert_eq!(
            block_on(driver.write(&[Rgb::BLACK; LED_COUNT])),
            Err(LedError::Bus(ErrorKind::Other))
        );
        assert_eq!(bus.0.borrow().writes.len(), fail_at + 1);
        bus.0.borrow_mut().fail_at = None;
        bus.0.borrow_mut().writes.clear();
        assert_eq!(block_on(driver.write(&[Rgb::BLACK; LED_COUNT])), Ok(()));
        assert_eq!(bus.0.borrow().writes[0], (0x50, vec![0xFE, 0xC5]));
        assert_eq!(bus.0.borrow().writes[1], (0x50, vec![0xFD, 1]));
    }
}

#[test]
fn initialization_failure_stops_before_later_writes_or_wake_delays() {
    let bus = Bus::default();
    let mut driver = Nos75Leds::new(bus.clone());
    let mut delay = Delay::default();
    assert_eq!(block_on(driver.init(&mut delay)), Ok(()));
    let expected = bus.0.borrow().writes.clone();
    for fail_at in 0..expected.len() {
        let bus = Bus::default();
        bus.0.borrow_mut().fail_at = Some(fail_at);
        let mut driver = Nos75Leds::new(bus.clone());
        let mut delay = Delay::default();
        assert_eq!(block_on(driver.init(&mut delay)), Err(ErrorKind::Other));
        assert_eq!(bus.0.borrow().writes, expected[..=fail_at]);
        let completed_wakes = match fail_at {
            0..24 => 0,
            24..48 => 1,
            _ => 2,
        };
        assert_eq!(delay.0.len(), completed_wakes);

        bus.0.borrow_mut().fail_at = None;
        bus.0.borrow_mut().writes.clear();
        assert_eq!(block_on(driver.init(&mut delay)), Ok(()));
        assert_eq!(bus.0.borrow().writes, expected);
    }
}

#[test]
fn shutdown_unlocks_function_page_and_blanks_both_chips_with_fail_fast_errors() {
    let expected = [
        (0x50, vec![0xFE, 0xC5]),
        (0x50, vec![0xFD, 3]),
        (0x50, vec![0x00, 0x00]),
        (0x53, vec![0xFE, 0xC5]),
        (0x53, vec![0xFD, 3]),
        (0x53, vec![0x00, 0x00]),
    ];
    let bus = Bus::default();
    let mut driver = Nos75Leds::new(bus.clone());
    assert_eq!(block_on(driver.shutdown()), Ok(()));
    assert_eq!(bus.0.borrow().writes, expected);

    for fail_at in 0..expected.len() {
        let bus = Bus::default();
        bus.0.borrow_mut().fail_at = Some(fail_at);
        let mut driver = Nos75Leds::new(bus.clone());
        assert_eq!(block_on(driver.shutdown()), Err(ErrorKind::Other));
        assert_eq!(bus.0.borrow().writes, expected[..=fail_at]);

        bus.0.borrow_mut().fail_at = None;
        bus.0.borrow_mut().writes.clear();
        assert_eq!(block_on(driver.shutdown()), Ok(()));
        assert_eq!(bus.0.borrow().writes, expected);
    }
}
