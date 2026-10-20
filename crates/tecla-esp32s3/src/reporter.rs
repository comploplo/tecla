//! USB HID reporters for ESP32-S3.
//!
//! The reporter implementation itself lives in tecla core as
//! [`CompositeUsbReporter`](tecla::transport::usb::CompositeUsbReporter) —
//! routing a [`HidReport`](tecla::pipeline::report::HidReport) to the right USB
//! interface is board-independent, and five copies of it drifted apart.
//! This module re-exports it plus the descriptors each interface needs.
//!
//! # Usage
//!
//! Build one `HidWriter` per interface you want, using the matching descriptor,
//! then register them:
//!
//! ```text
//! let reporter = CompositeUsbReporter::new(keyboard_writer)
//!     .with_consumer(consumer_writer)
//!     .with_mouse(mouse_writer);
//! ```
//!
//! A report whose interface was not registered returns
//! [`ReportError::Unsupported`](tecla::pipeline::hid::ReportError::Unsupported)
//! rather than being silently discarded.

pub use tecla::pipeline::hid::Capabilities;
pub use tecla::pipeline::report::{
    CONSUMER_REPORT_DESCRIPTOR, CONSUMER_REPORT_SIZE, GAMEPAD_REPORT_DESCRIPTOR,
    GAMEPAD_REPORT_SIZE, KEYBOARD_REPORT_DESCRIPTOR, KEYBOARD_REPORT_SIZE, MOUSE_REPORT_DESCRIPTOR,
    MOUSE_REPORT_SIZE, NKRO_REPORT_DESCRIPTOR, NKRO_REPORT_SIZE,
};
pub use tecla::transport::usb::CompositeUsbReporter;

/// Keyboard-only USB HID reporter.
///
/// Kept as an alias so existing board examples and downstream code keep
/// building; it is a [`CompositeUsbReporter`] with no optional interfaces
/// registered. Prefer naming `CompositeUsbReporter` directly in new code.
pub type UsbHidReporter<'d, D> = CompositeUsbReporter<'d, D>;
