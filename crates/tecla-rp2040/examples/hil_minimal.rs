//! Absolute minimum HIL firmware — boot + semihosting exit only.
//! Used to verify the Renode simulation baseline before adding firmware logic.

#![no_std]
#![no_main]
#![allow(
    unsafe_code,
    reason = "cortex_m_rt::entry and global_logger emit unsafe blocks"
)]

use cortex_m_rt::entry;
use cortex_m_semihosting::debug::{self, EXIT_SUCCESS};
use cortex_m_semihosting::hprintln;
use embassy_rp as _;

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
fn panic(_: &core::panic::PanicInfo) -> ! {
    debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    loop {}
}

// No `__pre_init` override here: embassy-rp defines its own (strong) one to
// reset PSM/SIO, and rp2040-minimal.repl maps PSM as plain memory precisely
// so that its write_and_wait read-back loop terminates. Overriding the symbol
// again would also collide with embassy-rp's under fat LTO.

#[entry]
fn main() -> ! {
    hprintln!("MINIMAL OK");
    debug::exit(EXIT_SUCCESS);
    loop {}
}
