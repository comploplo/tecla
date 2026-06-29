//! Power management for nRF52840.
//!
//! [`Nrf52840PowerManager`] implements [`PowerManager`] using the nRF52840
//! POWER peripheral for System OFF mode and GPIO SENSE for wake detection.

use tecla::pipeline::power::{PowerManager, WakeReason};

const POWER_BASE: u32 = 0x4000_0000;
const RESETREAS_OFFSET: u32 = 0x400;
const SYSTEMOFF_OFFSET: u32 = 0x500;

const RESETREAS_OFF: u32 = 1 << 16;
const RESETREAS_NFC: u32 = 1 << 19;
const RESETREAS_DIF: u32 = 1 << 18;

const GPIO_P0_BASE: u32 = 0x5000_0000;
const GPIO_P1_BASE: u32 = 0x5000_0300;
const PIN_CNF_OFFSET: u32 = 0x700;
const SENSE_LOW: u32 = 0b11 << 16;

/// nRF52840 power manager using System OFF mode with GPIO SENSE wake.
pub struct Nrf52840PowerManager {
    wake_pin_ports: &'static [u8],
}

impl Nrf52840PowerManager {
    /// Creates a new power manager.
    ///
    /// `wake_pin_ports` lists the embassy-nrf pin-port numbers (e.g. P0.13 = 13,
    /// P1.02 = 34) whose SENSE should be set to low before entering System OFF.
    pub fn new(wake_pin_ports: &'static [u8]) -> Self {
        Self { wake_pin_ports }
    }

    #[allow(
        unsafe_code,
        reason = "direct register read required — no PAC/HAL wrapper for RESETREAS"
    )]
    #[expect(
        clippy::as_conversions,
        reason = "u32 address to pointer for MMIO register access"
    )]
    fn read_reset_reason() -> u32 {
        let addr = (POWER_BASE + RESETREAS_OFFSET) as *const u32;
        unsafe { core::ptr::read_volatile(addr) }
    }

    #[allow(
        unsafe_code,
        reason = "direct register write required — clear RESETREAS after reading"
    )]
    #[expect(
        clippy::as_conversions,
        reason = "u32 address to pointer for MMIO register access"
    )]
    fn clear_reset_reason(bits: u32) {
        let addr = (POWER_BASE + RESETREAS_OFFSET) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, bits) }
    }

    #[allow(
        unsafe_code,
        reason = "System OFF requires writing to POWER->SYSTEMOFF register"
    )]
    #[expect(
        clippy::as_conversions,
        reason = "u32 address to pointer for MMIO register access"
    )]
    fn system_off() -> ! {
        let addr = (POWER_BASE + SYSTEMOFF_OFFSET) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, 1) }
        loop {
            cortex_m::asm::wfi();
        }
    }

    #[allow(
        unsafe_code,
        reason = "GPIO SENSE config requires raw PIN_CNF register writes"
    )]
    #[expect(
        clippy::as_conversions,
        reason = "u32 address to pointer for MMIO register access"
    )]
    fn configure_sense(pin_ports: &[u8]) {
        for &pin_port in pin_ports {
            let port = pin_port / 32;
            let pin = u32::from(pin_port % 32);
            let base = if port == 0 {
                GPIO_P0_BASE
            } else {
                GPIO_P1_BASE
            };
            let cnf_addr = (base + PIN_CNF_OFFSET + pin * 4) as *mut u32;
            unsafe {
                let val = core::ptr::read_volatile(cnf_addr);
                let val = (val & !(0b11 << 16)) | SENSE_LOW;
                core::ptr::write_volatile(cnf_addr, val);
            }
        }
    }
}

impl PowerManager for Nrf52840PowerManager {
    type Error = core::convert::Infallible;

    async fn enter_deep_sleep(&mut self) -> Result<(), Self::Error> {
        Self::configure_sense(self.wake_pin_ports);
        Self::system_off();
    }

    async fn wake_reason(&self) -> WakeReason {
        let reason = Self::read_reset_reason();
        Self::clear_reset_reason(reason);

        if reason & RESETREAS_OFF != 0 {
            return WakeReason::MatrixKey;
        }
        if reason & RESETREAS_NFC != 0 {
            return WakeReason::BleEvent;
        }
        if reason & RESETREAS_DIF != 0 {
            return WakeReason::UsbConnect;
        }
        WakeReason::MatrixKey
    }
}
