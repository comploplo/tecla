//! Host tests for Voyager LED mapping, bounded bus writes, and driver recovery.
#![cfg(feature = "embassy")]
#![allow(
    clippy::tests_outside_test_module,
    missing_docs,
    reason = "integration tests exercise the board drivers on a host"
)]
#![expect(clippy::unwrap_used, reason = "test assertions")]

use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use embedded_hal_async::i2c::{ErrorKind, ErrorType, I2c, Operation};
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::rgb::{LedDriver, Rgb};

#[path = "../../tecla-stm32f3/src/is31fl3731.rs"]
mod is31fl3731;
#[cfg(feature = "embassy")]
#[path = "../../tecla-stm32f3/src/shared_i2c.rs"]
mod shared_i2c;
#[path = "../../tecla-stm32f3/src/voyager_leds.rs"]
mod voyager_leds;

use is31fl3731::{Is31fl3731, ADDR_LEFT, ADDR_RIGHT};
use voyager_leds::{VoyagerLeds, LEDS, LED_INDEX, LED_POSITIONS};

#[derive(Debug)]
struct Write {
    address: u8,
    bytes: Vec<u8>,
}

struct BusState {
    writes: Vec<Write>,
    attempts: usize,
    fail_at: Option<usize>,
    pages: [u8; 2],
    registers: [[u8; 256]; 2],
}

impl Default for BusState {
    fn default() -> Self {
        Self {
            writes: Vec::new(),
            attempts: 0,
            fail_at: None,
            pages: [0; 2],
            registers: [[0; 256]; 2],
        }
    }
}

impl BusState {
    fn pwm_writes(&self) -> impl Iterator<Item = &Write> {
        self.writes
            .iter()
            .filter(|write| (0x24..0xB4).contains(&write.bytes[0]))
    }

    fn pwm(&self, chip: usize) -> &[u8] {
        &self.registers[chip][0x24..0xB4]
    }
}

#[derive(Clone, Default)]
struct FakeBus(Rc<RefCell<BusState>>);

impl ErrorType for FakeBus {
    type Error = ErrorKind;
}

impl I2c for FakeBus {
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        state.attempts += 1;
        if state.fail_at == Some(state.attempts) {
            state.fail_at = None;
            return Err(ErrorKind::Other);
        }
        for operation in operations {
            let Operation::Write(bytes) = operation else {
                panic!("the LED driver should only write");
            };
            state.writes.push(Write {
                address,
                bytes: bytes.to_vec(),
            });
            if let Some(chip) = [ADDR_LEFT, ADDR_RIGHT]
                .iter()
                .position(|candidate| *candidate == address)
            {
                if bytes[0] == 0xFD {
                    state.pages[chip] = bytes[1];
                } else if state.pages[chip] == 0 {
                    let start = usize::from(bytes[0]);
                    state.registers[chip][start..start + bytes.len() - 1]
                        .copy_from_slice(&bytes[1..]);
                }
            }
        }
        Ok(())
    }
}

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

fn leds(bus: &FakeBus) -> VoyagerLeds<FakeBus> {
    VoyagerLeds::new(
        Is31fl3731::new(bus.clone(), ADDR_LEFT),
        Is31fl3731::new(bus.clone(), ADDR_RIGHT),
    )
}

#[test]
fn initialization_yields_after_each_transaction_and_bounds_zero_pwm_chunks() {
    let bus = FakeBus::default();
    let mut driver = Is31fl3731::new(bus.clone(), ADDR_LEFT);
    let mut initialization = pin!(driver.init());
    let mut context = Context::from_waker(Waker::noop());
    loop {
        let before = bus.0.borrow().writes.len();
        let result = initialization.as_mut().poll(&mut context);
        let after = bus.0.borrow().writes.len();
        assert!(after - before <= 1, "initialization monopolized the bus");
        if let Poll::Ready(result) = result {
            result.unwrap();
            break;
        }
    }
    let state = bus.0.borrow();
    let pwm: Vec<_> = state.pwm_writes().collect();
    assert_eq!(pwm.len(), 18);
    for (chunk, write) in pwm.into_iter().enumerate() {
        assert_eq!(usize::from(write.bytes[0]), 0x24 + chunk * 8);
        assert_eq!(&write.bytes[1..], &[0; 8]);
    }
}

#[test]
fn identical_frames_are_silent_and_sparse_updates_skip_unchanged_chunks() {
    let bus = FakeBus::default();
    let mut driver = leds(&bus);
    let mut colors = [Rgb::BLACK; LEDS];
    run(driver.write(&colors)).unwrap();
    bus.0.borrow_mut().writes.clear();
    run(driver.write(&colors)).unwrap();
    assert!(bus.0.borrow().writes.is_empty());

    colors[0] = Rgb::new(11, 22, 33);
    run(driver.write(&colors)).unwrap();
    let state = bus.0.borrow();
    assert_eq!(state.writes.len(), 3);
    assert!(state
        .writes
        .iter()
        .all(|write| write.address == ADDR_LEFT && write.bytes.len() <= 9));
    let starts: Vec<_> = state.writes.iter().map(|write| write.bytes[0]).collect();
    assert_eq!(starts, [0x24, 0x34, 0x54]);
    assert_eq!(state.pwm(0)[0x11], 11);
    assert_eq!(state.pwm(0)[0x01], 22);
    assert_eq!(state.pwm(0)[0x32], 33);
}

#[test]
fn a_partial_failure_reinitializes_and_restores_both_complete_frames() {
    let bus = FakeBus::default();
    let mut driver = leds(&bus);
    run(driver.write(&[Rgb::BLACK; LEDS])).unwrap();
    {
        let mut state = bus.0.borrow_mut();
        state.fail_at = Some(state.attempts + 3);
    }
    assert!(run(driver.write(&[Rgb::WHITE; LEDS])).is_err());
    bus.0.borrow_mut().writes.clear();
    run(driver.write(&[Rgb::WHITE; LEDS])).unwrap();
    {
        let state = bus.0.borrow();
        // Both initialization clears and both full desired frames are retried.
        assert_eq!(state.pwm_writes().count(), 72);
        assert!(state.pwm_writes().all(|write| write.bytes.len() <= 9));
        for chip in 0..2 {
            assert_eq!(
                state
                    .pwm(chip)
                    .iter()
                    .filter(|&&value| value == 255)
                    .count(),
                78
            );
            assert!(state
                .pwm(chip)
                .iter()
                .all(|&value| value == 0 || value == 255));
        }
    }
    bus.0.borrow_mut().writes.clear();
    run(driver.write(&[Rgb::WHITE; LEDS])).unwrap();
    assert!(bus.0.borrow().writes.is_empty());
}

#[test]
fn every_physical_key_routes_its_rgb_channels_and_event_to_the_same_led() {
    // Physical left-to-right rows, independently transcribed from ZSA's layout.
    let mut positions: Vec<_> = (0..3)
        .flat_map(|row| (1..7).map(move |col| (row, col)))
        .collect();
    positions.extend((1..6).map(|col| (3, col)));
    positions.extend([(4, 4), (5, 0), (5, 1)]);
    positions.extend((6..9).flat_map(|row| (0..6).map(move |col| (row, col))));
    positions.push((10, 2));
    positions.extend((1..6).map(|col| (9, col)));
    positions.extend([(11, 5), (11, 6)]);
    assert_eq!(positions.len(), LEDS);

    // Read the vendor's indexed layout and wiring independently: the previous
    // hand-copied channel oracle repeated a six-key bottom-right rotation bug.
    let vendor = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.vendor/qmk_firmware/keyboards/zsa/voyager");
    let read_vendor = |name| {
        std::fs::read_to_string(vendor.join(name)).expect(
            "Voyager RGB fixture missing: run `git submodule update --init .vendor/qmk_firmware` from the repository root",
        )
    };
    let layout: serde_json::Value = serde_json::from_str(&read_vendor("keyboard.json")).unwrap();
    let layout = layout["rgb_matrix"]["layout"].as_array().unwrap();
    let wiring_source = read_vendor("voyager.c");
    let channel_table = wiring_source
        .split("g_is31fl3731_leds[RGB_MATRIX_LED_COUNT] = {")
        .nth(1)
        .unwrap()
        .split("};")
        .next()
        .unwrap();
    let wiring: Vec<(usize, [usize; 3])> = channel_table
        .lines()
        .filter_map(|line| line.trim().strip_prefix('{'))
        .map(|line| {
            let fields: Vec<_> = line.split('}').next().unwrap().split(',').collect();
            let chip = fields[0].trim().parse().unwrap();
            let channels = std::array::from_fn(|index| {
                let (column, row) = fields[index + 1]
                    .trim()
                    .strip_prefix('C')
                    .unwrap()
                    .split_once('_')
                    .unwrap();
                (column.parse::<usize>().unwrap() - 1) * 16 + row.parse::<usize>().unwrap() - 1
            });
            (chip, channels)
        })
        .collect();
    assert_eq!(layout.len(), LEDS);
    assert_eq!(wiring.len(), LEDS);
    assert_eq!(
        wiring[44..50]
            .iter()
            .map(|(_, channels)| channels[0])
            .collect::<Vec<_>>(),
        [0x7B, 0x7A, 0x79, 0x78, 0x1F, 0x1E],
        "the six right-bottom LEDs follow physical N through right Shift order"
    );
    let bus = FakeBus::default();
    let mut driver = leds(&bus);
    for (index, &(row, col)) in positions.iter().enumerate() {
        assert_eq!(layout[index]["matrix"], serde_json::json!([row, col]));
        assert_eq!(
            LED_INDEX[usize::from(row)][usize::from(col)],
            Some(u8::try_from(index).unwrap())
        );
        assert_eq!(LED_POSITIONS[index], (row, col));
        let event = KeyEvent::press(KeyPos::new(row, col), 123);
        let mapped = voyager_leds::remap_for_rgb(event);
        assert_eq!(mapped.pos.flat(16), index);
        assert_eq!((mapped.pressed, mapped.timestamp_ms), (true, 123));

        let mut colors = [Rgb::BLACK; LEDS];
        colors[index] = Rgb::new(11, 22, 33);
        run(driver.write(&colors)).unwrap();
        let mut expected = [[0; 144]; 2];
        let (chip, [red, green, blue]) = wiring[index];
        expected[chip][red] = 11;
        expected[chip][green] = 22;
        expected[chip][blue] = 33;
        let state = bus.0.borrow();
        assert_eq!(state.pwm(0), expected[0], "left chip for key {index}");
        assert_eq!(state.pwm(1), expected[1], "right chip for key {index}");
    }
    let spacer = voyager_leds::remap_for_rgb(KeyEvent::release(KeyPos::new(0, 0), 456));
    assert!(spacer.pos.flat(16) >= LEDS);
}

#[cfg(feature = "embassy")]
#[test]
fn scanning_can_acquire_the_real_shared_bus_between_every_pwm_chunk() {
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    use embassy_sync::mutex::Mutex;

    let bus = FakeBus::default();
    let mutex = Mutex::<NoopRawMutex, _>::new(bus.clone());
    let mut scanner = shared_i2c::SharedI2c::new(&mutex);
    let mut driver = Is31fl3731::new(shared_i2c::SharedI2c::new(&mutex), ADDR_LEFT);
    let mut frame = pin!(driver.write_frame(&[1; 144]));
    let mut context = Context::from_waker(Waker::noop());
    let mut scans = 0;
    loop {
        match frame.as_mut().poll(&mut context) {
            Poll::Pending => {
                let mut scan = pin!(scanner.write(0x20, &[0x01]));
                assert!(matches!(
                    scan.as_mut().poll(&mut context),
                    Poll::Ready(Ok(()))
                ));
                scans += 1;
            }
            Poll::Ready(result) => {
                result.unwrap();
                break;
            }
        }
    }
    assert_eq!(scans, 18);
    let state = bus.0.borrow();
    for pair in state.writes.as_chunks::<2>().0 {
        assert_eq!(pair[0].address, ADDR_LEFT);
        assert!(pair[0].bytes.len() <= 9);
        assert_eq!(pair[1].address, 0x20);
    }
}
