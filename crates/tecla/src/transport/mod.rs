//! Communication transports.
//!
//! Split keyboard transport traits, UART implementation, BLE types/commands,
//! and the composite USB HID reporter.

pub mod ble;
pub mod traits;
#[cfg(feature = "embassy")]
pub mod uart;
#[cfg(feature = "usb")]
pub mod usb;

pub use traits::*;
