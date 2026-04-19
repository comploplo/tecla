//! CDC ACM telemetry writer for RP2040.
//!
//! [`CdcTelemetryWriter`] implements [`AsyncTelemetryWriter`] by writing
//! serialized telemetry frames over a USB CDC ACM virtual serial port.
//! The host reads frames from this serial port for live telemetry display.
//!
//! # Setup
//!
//! Add a CDC ACM class to the USB builder alongside the HID class:
//!
//! ```rust,ignore
//! let cdc = CdcAcmClass::new(&mut builder, &mut cdc_state, 64);
//! let (cdc_tx, _cdc_rx) = cdc.split();
//! let telem_writer = CdcTelemetryWriter::new(cdc_tx);
//! ```

use embassy_usb::class::cdc_acm::Sender;
use embassy_usb::driver::Driver;
use tecla::telemetry::transport::AsyncTelemetryWriter;

/// CDC ACM telemetry writer.
///
/// Wraps a CDC ACM [`Sender`] and writes telemetry frames as raw bytes.
/// The host can read these from the virtual serial port.
pub struct CdcTelemetryWriter<'d, D: Driver<'d>> {
    sender: Sender<'d, D>,
}

impl<'d, D: Driver<'d>> CdcTelemetryWriter<'d, D> {
    /// Wrap a CDC ACM sender as a telemetry writer.
    #[must_use]
    pub fn new(sender: Sender<'d, D>) -> Self {
        Self { sender }
    }
}

impl<'d, D: Driver<'d>> AsyncTelemetryWriter for CdcTelemetryWriter<'d, D> {
    async fn write(&mut self, buf: &[u8]) -> Result<(), ()> {
        self.sender.write_packet(buf).await.map_err(|_| ())
    }
}
