//! Tecla board support crate for the nRF52840 microcontroller.
//!
//! This crate provides the hardware-specific glue between the tecla firmware
//! core and the nRF52840 GPIO / USB peripherals via the
//! [Embassy](https://embassy.dev) async runtime and `embassy-nrf`.
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
//! Build for `thumbv7em-none-eabihf` using the provided `.cargo/config.toml`.
//! Flash with `probe-rs` (SWD/JTAG).

#![no_std]

pub mod power;
pub mod reporter;
pub mod scanner;
pub mod telemetry;

#[cfg(feature = "ble")]
#[allow(
    dead_code,
    reason = "trouble-host GATT macros generate struct fields read by the BLE stack at runtime"
)]
pub mod ble_reporter;

#[cfg(feature = "ble")]
pub mod battery;

pub use power::Nrf52840PowerManager;
pub use reporter::CompositeUsbReporter;
pub use scanner::{DirectPinScanner, GpioMatrixScanner};
pub use telemetry::CdcTelemetryWriter;

#[cfg(feature = "ble")]
pub use ble_reporter::run_ble_peripheral;

/// Combined HID report descriptor: keyboard (Report ID 1) + consumer control (Report ID 2).
pub const KEYBOARD_REPORT_DESCRIPTOR: &[u8] = &[
    // ---- Report ID 1: 6KRO Keyboard ----
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x06, // Usage (Keyboard)
    0xA1, 0x01, // Collection (Application)
    0x85, 0x01, //   Report ID (1)
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0xE0, //   Usage Minimum (Left Control)
    0x29, 0xE7, //   Usage Maximum (Right GUI)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x08, //   Report Count (8)
    0x81, 0x02, //   Input (Data, Variable, Absolute) — modifiers
    0x95, 0x01, //   Report Count (1)
    0x75, 0x08, //   Report Size (8)
    0x81, 0x01, //   Input (Constant) — reserved byte
    0x95, 0x05, //   Report Count (5)
    0x75, 0x01, //   Report Size (1)
    0x05, 0x08, //   Usage Page (LEDs)
    0x19, 0x01, //   Usage Minimum (Num Lock)
    0x29, 0x05, //   Usage Maximum (Kana)
    0x91, 0x02, //   Output (Data, Variable, Absolute) — LED report
    0x95, 0x01, //   Report Count (1)
    0x75, 0x03, //   Report Size (3)
    0x91, 0x01, //   Output (Constant) — LED padding
    0x95, 0x06, //   Report Count (6)
    0x75, 0x08, //   Report Size (8)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0xDD, //   Logical Maximum (221)
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0x00, //   Usage Minimum (0)
    0x29, 0xDD, //   Usage Maximum (221)
    0x81, 0x00, //   Input (Data, Array) — keycodes
    0xC0, // End Collection
    // ---- Report ID 2: Consumer Control ----
    0x05, 0x0C, // Usage Page (Consumer)
    0x09, 0x01, // Usage (Consumer Control)
    0xA1, 0x01, // Collection (Application)
    0x85, 0x02, //   Report ID (2)
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xFF, 0x0F, //   Logical Maximum (4095)
    0x19, 0x00, //   Usage Minimum (0)
    0x2A, 0xFF, 0x0F, //   Usage Maximum (4095)
    0x75, 0x10, //   Report Size (16)
    0x95, 0x01, //   Report Count (1)
    0x81, 0x00, //   Input (Data, Array)
    0xC0, // End Collection
];
