//! Communication transports.
//!
//! Split keyboard transport traits, UART implementation, and BLE
//! types/commands.

pub mod ble;
pub mod traits;
#[cfg(feature = "embassy")]
pub mod uart;

pub use traits::*;
