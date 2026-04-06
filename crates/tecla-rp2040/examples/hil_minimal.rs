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

/// Override embassy-rp's weak `__pre_init` to skip RP2040 clock/reset
/// initialization — the stub Renode platform has no clock hardware.
#[no_mangle]
pub unsafe extern "C" fn __pre_init() {}

#[entry]
fn main() -> ! {
    hprintln!("MINIMAL OK");
    debug::exit(EXIT_SUCCESS);
    loop {}
}
