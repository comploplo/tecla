//! HIL end-to-end keycode verification for nRF52840.

#![no_std]
#![no_main]
#![allow(missing_docs)]
#![allow(
    unsafe_code,
    reason = "cortex_m_rt::entry and defmt global_logger emit unsafe blocks"
)]
#![expect(
    clippy::as_conversions,
    reason = "matrix dimension casts in firmware test loop"
)]

use cortex_m_rt::entry;
use cortex_m_semihosting::debug::{self, EXIT_FAILURE, EXIT_SUCCESS};
use cortex_m_semihosting::hprintln;
use embassy_nrf as _;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::report::HidReport;
use tecla_macros::keymap;

const ROWS: usize = 4;
const COLS: usize = 12;
const MAX_REPORTS: usize = 48;

#[repr(C)]
#[derive(Clone, Copy)]
struct ReportEntry {
    row: u8,
    col: u8,
    modifiers: u8,
    keycode: u8,
}

#[no_mangle]
static mut REPORT_BUF: [ReportEntry; MAX_REPORTS] = [ReportEntry {
    row: 0,
    col: 0,
    modifiers: 0,
    keycode: 0,
}; MAX_REPORTS];
#[no_mangle]
static mut REPORT_COUNT: u32 = 0;

#[defmt::global_logger]
struct HilLogger;
unsafe impl defmt::Logger for HilLogger {
    fn acquire() {}
    unsafe fn flush() {}
    unsafe fn release() {}
    unsafe fn write(_bytes: &[u8]) {}
}
defmt::timestamp!("");

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = hprintln!("PANIC: {}", info);
    debug::exit(EXIT_FAILURE);
    loop {}
}

#[entry]
fn main() -> ! {
    let keymap = keymap! {
        layer!(Base,
            TAB,  Q, W, E, R, T, Y, U, I, O, P,    BSPC,
            ESC,  A, S, D, F, G, H, J, K, L, SCLN, ENT,
            LSFT, Z, X, C, V, B, N, M, COMM, DOT,  SLSH, RSFT,
            LCTL, LGUI, LALT, LSFT, LT!(Fn, SPC), SPC, SPC, RSFT, LEFT, DOWN, UP, RGHT
        ),
        layer!(Fn,
            GRV,  N1, N2, N3, N4,  N5,  N6,  N7,   N8,  N9,   N0,   DEL,
            TRNS, F1, F2, F3, F4,  F5,  F6,  MINS, EQL, LBRC, RBRC, BSLS,
            TRNS, F7, F8, F9, F10, F11, F12, TRNS, TRNS, TRNS, TRNS, TRNS,
            TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS
        ),
    };

    let mut processor =
        KeyboardProcessor::<ROWS, COLS, 2, 48>::new(keymap, TapHoldConfig::default());
    let mut time = 100u64;

    for row in 0..ROWS as u8 {
        for col in 0..COLS as u8 {
            let pos = KeyPos::new(row, col);

            processor.process_event(KeyEvent::press(pos, time), time, &mut |report| {
                if let HidReport::Keyboard(kr) = report {
                    unsafe {
                        let count = REPORT_COUNT as usize;
                        if count < MAX_REPORTS {
                            REPORT_BUF[count] = ReportEntry {
                                row,
                                col,
                                modifiers: kr.modifiers,
                                keycode: kr.keycodes[0],
                            };
                            REPORT_COUNT = (count + 1) as u32;
                        }
                    }
                    report_marker();
                }
            });
            time += 10;

            processor.process_event(KeyEvent::release(pos, time), time, &mut |_| {});
            time += 10;
        }
    }

    hil_marker();
    let _ = hprintln!("HIL OK");
    debug::exit(EXIT_SUCCESS);
    loop {}
}

#[inline(never)]
#[no_mangle]
fn report_marker() {
    cortex_m::asm::nop();
    cortex_m::asm::nop();
}

#[inline(never)]
#[no_mangle]
fn hil_marker() {
    cortex_m::asm::nop();
}
