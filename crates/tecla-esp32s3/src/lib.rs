//! Tecla board support crate for the ESP32-S3 microcontroller.
//!
//! This crate provides the hardware-specific glue between the tecla firmware
//! core and the ESP32-S3 GPIO / USB peripherals via the
//! [Embassy](https://embassy.dev) async runtime and `esp-hal`.
//!
//! # What's in this crate
//!
//! - [`scanner::GpioMatrixScanner`] — row-strobe / column-read GPIO matrix
//!   scanner that implements [`tecla::matrix::AsyncMatrixScanner`].
//! - [`reporter::CompositeUsbReporter`] — embassy-usb HID writer wrapper that
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
//! Build for `xtensa-esp32s3-none-elf` using the provided `.cargo/config.toml`.
//! Flash with `espflash`. Requires the esp-rs Xtensa toolchain — install via
//! `espup install`.
//!
//! # ESP32-S3 notes
//!
//! The ESP32-S3 has native USB-OTG, WiFi, and Bluetooth LE — making it
//! suitable for both wired and wireless keyboard builds. This crate currently
//! supports wired USB HID only.

#![no_std]

pub mod reporter;
pub mod scanner;
pub mod telemetry;

pub use reporter::CompositeUsbReporter;
pub use scanner::{DirectPinScanner, GpioMatrixScanner};
pub use telemetry::CdcTelemetryWriter;

pub use tecla::pipeline::report::KEYBOARD_REPORT_DESCRIPTOR;
