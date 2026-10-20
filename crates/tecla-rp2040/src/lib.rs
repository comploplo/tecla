//! Tecla board support crate for the RP2040 microcontroller.
//!
//! This crate provides the hardware-specific glue between the tecla firmware
//! core and the RP2040 GPIO / USB peripherals via the
//! [Embassy](https://embassy.dev) async runtime and `embassy-rp`.
//!
//! # What's in this crate
//!
//! - [`scanner::GpioMatrixScanner`] — row-strobe / column-read GPIO matrix
//!   scanner that implements [`tecla::matrix::scanner::AsyncMatrixScanner`].
//! - [`audio::PwmBuzzer`] — PWM passive-buzzer driver implementing
//!   [`tecla::audio::BuzzerDriver`], for `Action::Audio` feedback.
//! - [`haptic::Drv2605l`] — I²C DRV2605L driver implementing
//!   [`tecla::haptic::HapticDriver`], for `Action::Haptic` feedback.
//! - [`reporter::CompositeUsbReporter`] — routes each HID report variant to
//!   its own USB interface (keyboard, NKRO, consumer, mouse, gamepad, MIDI)
//!   and implements [`tecla::pipeline::hid::AsyncReporter`].
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

pub mod audio;
pub mod haptic;
pub mod reporter;
pub mod scanner;
pub mod telemetry;

pub use audio::PwmBuzzer;
pub use haptic::Drv2605l;
pub use reporter::{Capabilities, CompositeUsbReporter, UsbHidReporter};
pub use scanner::{CapTouchScanner, DirectPinScanner, GpioMatrixScanner};
pub use telemetry::CdcTelemetryWriter;

pub use tecla::pipeline::report::{
    CONSUMER_REPORT_DESCRIPTOR, CONSUMER_REPORT_SIZE, GAMEPAD_REPORT_DESCRIPTOR,
    GAMEPAD_REPORT_SIZE, KEYBOARD_REPORT_DESCRIPTOR, KEYBOARD_REPORT_SIZE, MOUSE_REPORT_DESCRIPTOR,
    MOUSE_REPORT_SIZE, NKRO_REPORT_DESCRIPTOR, NKRO_REPORT_SIZE,
};
