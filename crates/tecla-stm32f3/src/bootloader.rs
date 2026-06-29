//! ZSA Ignition bootloader helpers.
//!
//! Voyager uses ZSA's custom Ignition bootloader. QMK enters it by charging
//! the A8/A9 sense circuit, waiting long enough for the bootloader's power-on
//! check, then resetting the MCU.

use embassy_stm32::gpio::{Flex, Pull, Speed};
use embassy_time::Timer;

/// Leaves the Ignition sense pins in the same idle state as QMK.
pub fn idle_ignition_pins(pa8: &mut Flex<'_>, pa9: &mut Flex<'_>) {
    pa8.set_as_input(Pull::None);
    pa9.set_low();
    pa9.set_as_output(Speed::Low);
}

/// Enters ZSA's Ignition bootloader.
pub async fn enter_ignition_bootloader(pa8: &mut Flex<'_>, pa9: &mut Flex<'_>) -> ! {
    pa9.set_high();
    pa9.set_as_output(Speed::Low);
    pa8.set_high();
    pa8.set_as_output(Speed::Low);
    Timer::after_millis(500).await;
    pa9.set_low();

    cortex_m::peripheral::SCB::sys_reset();
}
