//! HIL steno stitch verification for RP2040.
//!
//! This firmware does not scan a physical matrix. Instead it runs a tiny
//! scripted steno translation sequence on-device, prints the resulting typed
//! characters over semihosting, and exits.

#![no_std]
#![no_main]
#![allow(unsafe_code)]
#![expect(clippy::unwrap_used)]

use cortex_m_rt::entry;
use cortex_m_semihosting::debug::{self, EXIT_FAILURE, EXIT_SUCCESS};
use cortex_m_semihosting::hprintln;
use embassy_rp as _;
use heapless::String;
use tecla::action::KeyCode;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::devices::steno::steno::StenoEngine;
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
    0x15, 0x00, 0x00, 0x00, // root offset = 21
    0x00, 0x00, // child_count = 0
    0x0D, 0x00, // translation_len = 13
    0x04, // OP_SPACE
    0x0C, // OP_STITCH
    0x03, 0x00, // "ing"
    0x69, 0x6E, 0x67, 0x01, // OP_TEXT
    0x03, 0x00, // "fox"
    0x66, 0x6F, 0x78, 0x01, 0x00, // root child_count = 1
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // stroke bit 0
    0x04, 0x00, 0x00, 0x00, // child offset = 4
    0x00, 0x00, // root translation_len = 0
];

#[entry]
fn main() -> ! {
    let keymap = keymap! {
        layer!(Base,
            STENO(0)
        ),
    };

    let steno_engine = StenoEngine::new(STENO_DICT);
    let mut processor: KeyboardProcessor<1, 1, 1, 1> =
        KeyboardProcessor::new(keymap, TapHoldConfig::default()).with_steno(steno_engine);

    let mut seq: String<32> = String::new();
    let key = KeyPos::new(0, 0);
    let mut time = 0u64;

    processor.process_event(KeyEvent::press(key, time), time, &mut |report| {
        capture(report, &mut seq);
    });
    time += 10;
    processor.process_event(KeyEvent::release(key, time), time, &mut |report| {
        capture(report, &mut seq);
    });

    let _ = hprintln!("SEQ:{seq}");
    let _ = hprintln!("HIL OK");
    debug::exit(EXIT_SUCCESS);
    loop {}
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
    let _ = hprintln!("PANIC: {}", info);
    debug::exit(EXIT_FAILURE);
    loop {}
}
