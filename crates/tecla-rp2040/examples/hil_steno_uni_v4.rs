//! HIL steno validation for the Uni V4 RP2040 board.
//!
//! This example reuses the Uni V4 matrix/keymap layout and drives a single
//! steno chord through the processor, printing the resulting text via
//! semihosting so Renode can verify the board-specific build.

#![no_std]
#![no_main]
#![allow(unsafe_code)]

use cortex_m_rt::entry;
use cortex_m_semihosting::debug::{self, EXIT_FAILURE, EXIT_SUCCESS};
use cortex_m_semihosting::hprintln;
use embassy_rp as _;
use heapless::String;
use tecla::action::KeyCode;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::devices::steno::StenoEngine;
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::report::HidReport;
use tecla_macros::keymap;

#[defmt::global_logger]
struct HilLogger;
unsafe impl defmt::Logger for HilLogger {
    fn acquire() {}
    unsafe fn flush() {}
    unsafe fn release() {}
    unsafe fn write(_bytes: &[u8]) {}
}
defmt::timestamp!("");

const STENO_DICT: &[u8] = &[
    0x0D, 0x00, 0x00, 0x00, // Root offset
    0x00, 0x00, // child_count: 0
    0x05, 0x00, // translation_len: 5
    0x0B, 0x08, 0x0F, 0x0F, 0x12, // "hello"
    0x01, 0x00, // child_count: 1
    0x10, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // stroke: 0x810
    0x04, 0x00, 0x00, 0x00, // offset: 4
    0x00, 0x00, // translation_len: 0
];

#[entry]
fn main() -> ! {
    let keymap = keymap! {
        layer!(Base,
            STENO(0), STENO(1), STENO(2), STENO(3), STENO(4), STENO(5),
            STENO(6), STENO(7), STENO(8), STENO(9), STENO(10), STENO(11)
        ),
    };

    let steno_engine = StenoEngine::new(STENO_DICT);
    let mut processor: KeyboardProcessor<2, 6, 1, 12> =
        KeyboardProcessor::new(keymap, TapHoldConfig::default()).with_steno(steno_engine);

    let mut seq: String<32> = String::new();
    let key_a = KeyPos::new(0, 4);
    let key_b = KeyPos::new(1, 5);
    let time = 0u64;

    processor.process_event(KeyEvent::press(key_a, time), time, &mut |report| {
        capture(report, &mut seq);
    });
    processor.process_event(KeyEvent::press(key_b, time + 5), time + 5, &mut |report| {
        capture(report, &mut seq);
    });
    processor.process_event(
        KeyEvent::release(key_a, time + 10),
        time + 10,
        &mut |report| {
            capture(report, &mut seq);
        },
    );
    processor.process_event(
        KeyEvent::release(key_b, time + 15),
        time + 15,
        &mut |report| {
            capture(report, &mut seq);
        },
    );

    hprintln!("SEQ:{seq}");
    hprintln!("HIL OK");
    debug::exit(EXIT_SUCCESS);
    loop {
        cortex_m::asm::wfi();
    }
}

fn capture(report: HidReport, seq: &mut String<32>) {
    if let HidReport::Keyboard(kr) = report {
        if let Some(usage_id) = kr.keycodes.iter().copied().find(|kc| *kc != 0) {
            if let Some(ch) = usage_to_char(usage_id, kr.modifiers) {
                let _ = seq.push(ch);
            }
        }
    }
}

fn usage_to_char(usage_id: u8, _mods: u8) -> Option<char> {
    match usage_id {
        0x04..=0x1D => Some(char::from(b'a' + (usage_id - 0x04))),
        x if x == KeyCode::Space.usage_id() => Some(' '),
        _ => None,
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    hprintln!("PANIC: {}", info);
    debug::exit(EXIT_FAILURE);
    loop {
        cortex_m::asm::wfi();
    }
}
