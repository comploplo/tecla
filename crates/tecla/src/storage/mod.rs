//! Persistent storage and configuration protocols.
//!
//! Wear-leveled flash storage for keymaps and runtime state, EEPROM
//! emulation shim, and the Via configuration protocol.

#[cfg(feature = "storage")]
pub mod state;
#[cfg(feature = "storage")]
pub use state::*;

#[cfg(feature = "storage")]
pub mod eeprom;

#[cfg(feature = "via")]
pub mod via;
