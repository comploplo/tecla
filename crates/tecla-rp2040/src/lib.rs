//! Tecla board support crate for the RP2040 microcontroller.
//!
//! This crate provides the hardware-specific glue between the tecla firmware
//! core and the RP2040 GPIO / USB peripherals via the
//! [Embassy](https://embassy.dev) async runtime and `embassy-rp`.
//!
//! # What's in this crate
//!
//! - [`scanner::GpioMatrixScanner`] — row-strobe / column-read GPIO matrix
//!   scanner that implements [`tecla::matrix::AsyncMatrixScanner`].
//! - [`reporter::UsbHidReporter`] — embassy-usb HID writer wrapper that
//!   implements [`tecla::hid::AsyncReporter`].
//!
//! # Wiring convention
//!
//! | Signal   | Direction | Idle state | Notes                        |
//! |----------|-----------|------------|------------------------------|
//! | Row pins | Output    | High       | Driven low during scan slot  |
//! | Col pins | Input     | High       | Pull-up enabled; low = press |
//!
//! # Target
//!
//! Build for `thumbv6m-none-eabi` using the provided `.cargo/config.toml`.
//! Flash with `probe-rs` (SWD/JTAG) or drag-and-drop UF2.

#![no_std]

pub mod reporter;
pub mod scanner;

pub use reporter::UsbHidReporter;
pub use scanner::{DirectPinScanner, GpioMatrixScanner};

pub use tecla::report::KEYBOARD_REPORT_DESCRIPTOR;
