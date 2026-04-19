//! Tecla board support crate for STM32F4 microcontrollers.
//!
//! This crate provides the hardware-specific glue between the tecla firmware
//! core and STM32F4 GPIO / USB OTG FS peripherals via the
//! [Embassy](https://embassy.dev) async runtime and `embassy-stm32`.
//!
//! # Supported chips
//!
//! - STM32F401 (WeAct Blackpill v1)
//! - STM32F411 (WeAct Blackpill v2)
//!
//! # What's in this crate
//!
//! - [`scanner::GpioMatrixScanner`] — row-strobe / column-read GPIO matrix
//!   scanner that implements [`tecla::matrix::AsyncMatrixScanner`].
//! - [`reporter::UsbHidReporter`] — embassy-usb HID writer wrapper that
//!   implements [`tecla::pipeline::hid::AsyncReporter`].
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
//! Build for `thumbv7em-none-eabihf` using the provided `.cargo/config.toml`.
//! Flash with `probe-rs` (SWD/JTAG).

#![no_std]

pub mod reporter;
pub mod scanner;

pub use reporter::UsbHidReporter;
pub use scanner::{DirectPinScanner, GpioMatrixScanner};

pub use tecla::pipeline::report::KEYBOARD_REPORT_DESCRIPTOR;
