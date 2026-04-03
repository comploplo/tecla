//! HID output abstraction.
//!
//! [`Reporter`] is the output boundary trait. Implementations send
//! [`HidReport`]s over USB, BLE, or any other transport.
//!
//! A single board can register multiple reporters (keyboard + MIDI +
//! consumer) via a composite implementation.

use crate::report::HidReport;

/// An error produced when sending a HID report fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportError {
    /// The transport is not currently connected.
    NotConnected,
    /// The send buffer is full; the report was dropped.
    BufferFull,
}

/// Output boundary for HID reports.
///
/// Implementations are provided by board crates (`UsbKeyboardReporter`,
/// `BleReporter`, etc.).  The `keyboard` orchestrator calls [`send`] after
/// every processed event.
///
/// # Note on `async`
///
/// The `send` method is intentionally synchronous here so the trait can be
/// used without an async executor in testing.  The Embassy-based
/// implementation in a board crate wraps it in an async task.
pub trait Reporter {
    /// Sends a single HID report.
    fn send(&mut self, report: &HidReport) -> Result<(), ReportError>;
}

/// Async output boundary for HID reports.
///
/// Implement this in your board crate using `embassy_usb::class::hid::HidWriter`.
/// Pass the implementation to [`crate::task::run`].
///
/// `async fn` in traits is used intentionally here — tecla targets single-threaded
/// Embassy executors where `Send` bounds on futures are not required.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait AsyncReporter {
    /// Sends a single HID report, yielding until the transport accepts it.
    async fn send(&mut self, report: &HidReport) -> Result<(), ReportError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::KeyboardReport;

    struct OkReporter {
        sent: usize,
    }

    impl Reporter for OkReporter {
        fn send(&mut self, _report: &HidReport) -> Result<(), ReportError> {
            self.sent += 1;
            Ok(())
        }
    }

    struct FailReporter;

    impl Reporter for FailReporter {
        fn send(&mut self, _report: &HidReport) -> Result<(), ReportError> {
            Err(ReportError::NotConnected)
        }
    }

    #[test]
    fn ok_reporter_succeeds() {
        let mut r = OkReporter { sent: 0 };
        let report = HidReport::Keyboard(KeyboardReport::new());
        assert!(r.send(&report).is_ok());
        assert_eq!(r.sent, 1);
    }

    #[test]
    fn fail_reporter_returns_not_connected() {
        let mut r = FailReporter;
        let report = HidReport::Keyboard(KeyboardReport::new());
        assert_eq!(r.send(&report), Err(ReportError::NotConnected));
    }

    #[test]
    fn report_error_variants_are_distinct() {
        assert_ne!(ReportError::NotConnected, ReportError::BufferFull);
    }
}
