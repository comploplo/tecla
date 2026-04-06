//! USB HID reporter for STM32F4.
//!
//! [`UsbHidReporter`] implements [`AsyncReporter`] using an
//! `embassy_usb::class::hid::HidWriter`. It serialises tecla's
//! [`HidReport::Keyboard`] variant to the 8-byte boot-protocol keyboard
//! report and writes it to the USB HID endpoint.
//!
//! [`HidReport::Consumer`] and [`HidReport::NkroKeyboard`] are silently
//! accepted and discarded — consumer and NKRO support will be added in a
//! later phase.

use embassy_usb::class::hid::HidWriter;
use embassy_usb::driver::Driver;
use tecla::hid::{AsyncReporter, ReportError};
use tecla::report::HidReport;

/// USB HID reporter backed by an embassy-usb [`HidWriter`].
///
/// The const generic `N` is the HID endpoint packet size; use `8` for the
/// standard 6KRO boot-protocol keyboard report.
///
/// # Usage
///
/// ```rust,ignore
/// use embassy_usb::class::hid::HidWriter;
/// use tecla_stm32f4::reporter::UsbHidReporter;
///
/// // After building the USB device and obtaining a HidWriter<'_, D, 8>:
/// let reporter = UsbHidReporter::new(hid_writer);
/// ```
pub struct UsbHidReporter<'d, D: Driver<'d>, const N: usize> {
    writer: HidWriter<'d, D, N>,
}

impl<'d, D: Driver<'d>, const N: usize> UsbHidReporter<'d, D, N> {
    /// Wraps a pre-built [`HidWriter`] as an [`AsyncReporter`].
    #[must_use]
    pub fn new(writer: HidWriter<'d, D, N>) -> Self {
        Self { writer }
    }
}

impl<'d, D: Driver<'d>> AsyncReporter for UsbHidReporter<'d, D, 8> {
    async fn send(&mut self, report: &HidReport) -> Result<(), ReportError> {
        let HidReport::Keyboard(kr) = report else {
            return Ok(());
        };
        let bytes: [u8; 8] = [
            kr.modifiers,
            kr.reserved,
            kr.keycodes[0],
            kr.keycodes[1],
            kr.keycodes[2],
            kr.keycodes[3],
            kr.keycodes[4],
            kr.keycodes[5],
        ];
        self.writer
            .write(&bytes)
            .await
            .map_err(|_e| ReportError::NotConnected)
    }
}
