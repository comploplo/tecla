//! Tecla board support crate for STM32F3 microcontrollers — ZSA Voyager.
//!
//! Provides the hardware glue between the tecla firmware core and the Voyager's
//! STM32F303 + MCP23018 split matrix, via [Embassy](https://embassy.dev).
//!
//! # What's in this crate
//!
//! - [`mcp23018::Mcp23018`] — minimal blocking I2C driver for the right-hand
//!   GPIO expander.
//! - [`scanner::VoyagerScanner`] — combined left-half-GPIO / right-half-MCP23018
//!   matrix scanner implementing [`tecla::matrix::AsyncMatrixScanner`].
//! - [`reporter::UsbHidReporter`] — embassy-usb HID writer wrapper implementing
//!   [`tecla::pipeline::hid::AsyncReporter`].
//!
//! # Target
//!
//! Build for `thumbv7em-none-eabihf`. The application links at `0x08002000`
//! (above the ZSA DFU bootloader) — see `memory.x` and the `voyager` example.

#![no_std]

pub mod bitbang_i2c;
pub mod bootloader;
pub mod is31fl3731;
pub mod mcp23018;
pub mod reporter;
pub mod scanner;
pub mod shared_i2c;
pub mod voyager_leds;

pub use reporter::UsbHidReporter;
pub use scanner::VoyagerScanner;

pub use tecla::pipeline::report::KEYBOARD_REPORT_DESCRIPTOR;
