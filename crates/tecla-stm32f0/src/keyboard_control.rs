//! Control requests for the NOS75's identical boot and report keyboard packets.

use embassy_usb::class::hid::{HidProtocolMode, RequestHandler};
use embassy_usb::control::OutResponse;

/// Accepts both keyboard protocols because each uses the same eight-byte report.
pub struct KeyboardControl {
    protocol: HidProtocolMode,
}

impl KeyboardControl {
    /// Starts in report protocol, as required after USB enumeration.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            protocol: HidProtocolMode::Report,
        }
    }
}

impl Default for KeyboardControl {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestHandler for KeyboardControl {
    fn get_protocol(&self) -> HidProtocolMode {
        self.protocol
    }

    fn set_protocol(&mut self, protocol: HidProtocolMode) -> OutResponse {
        self.protocol = protocol;
        OutResponse::Accepted
    }
}
