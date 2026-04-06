//! HIL smoke test for the nRF52840 keyboard firmware.
//!
//! Builds a `KeyboardProcessor` with the standard Planck keymap, reports
//! pass/fail via ARM semihosting so Renode can capture the result.
//!
//! # Running
//!
//! ```sh
//! just hil-nrf52840
//! ```

#![no_std]
#![no_main]
#![allow(
    unsafe_code,
    reason = "cortex_m_rt::entry and defmt global_logger emit unsafe blocks"
)]

use cortex_m_rt::entry;
use cortex_m_semihosting::debug::{self, EXIT_FAILURE, EXIT_SUCCESS};
use cortex_m_semihosting::hprintln;
use embassy_nrf as _;
use tecla::keyboard::KeyboardProcessor;
use tecla::tapping::TapHoldConfig;
use tecla_macros::keymap;

const ROWS: usize = 4;
const COLS: usize = 12;

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
fn panic(_info: &core::panic::PanicInfo) -> ! {
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

    let _processor = KeyboardProcessor::<ROWS, COLS, 2, 48>::new(keymap, TapHoldConfig::default());

    hil_marker();
    hprintln!("HIL OK");
    debug::exit(EXIT_SUCCESS);
    loop {}
}

#[inline(never)]
#[no_mangle]
fn hil_marker() {
    cortex_m::asm::nop();
}
