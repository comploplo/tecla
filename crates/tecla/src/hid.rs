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

impl core::fmt::Display for ReportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotConnected => write!(f, "transport not connected"),
            Self::BufferFull => write!(f, "send buffer full, report dropped"),
        }
    }
}

/// Output boundary for HID reports.
///
/// Implementations are provided by board crates (`UsbKeyboardReporter`,
/// `BleReporter`, etc.).  The `keyboard` orchestrator calls `send` after
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

/// Active output transport selection for dual USB+BLE keyboards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Send reports over USB only.
    Usb,
    /// Send reports over BLE only.
    Ble,
}

/// A dual-transport reporter that switches between USB and BLE at runtime.
///
/// Wraps two [`AsyncReporter`] implementations and forwards reports to the
/// one selected by [`set_mode`](Self::set_mode). Defaults to USB.
#[cfg(feature = "embassy")]
pub struct DualReporter<U, B> {
    usb: U,
    ble: B,
    mode: OutputMode,
}

#[cfg(feature = "embassy")]
impl<U: AsyncReporter, B: AsyncReporter> DualReporter<U, B> {
    /// Creates a dual reporter defaulting to USB output.
    #[must_use]
    pub fn new(usb: U, ble: B) -> Self {
        Self {
            usb,
            ble,
            mode: OutputMode::Usb,
        }
    }

    /// Returns the current output mode.
    #[must_use]
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Switches the active output transport.
    pub fn set_mode(&mut self, mode: OutputMode) {
        self.mode = mode;
    }

    /// Returns a mutable reference to the USB reporter.
    pub fn usb_mut(&mut self) -> &mut U {
        &mut self.usb
    }

    /// Returns a mutable reference to the BLE reporter.
    pub fn ble_mut(&mut self) -> &mut B {
        &mut self.ble
    }
}

#[cfg(feature = "embassy")]
impl<U: AsyncReporter, B: AsyncReporter> AsyncReporter for DualReporter<U, B> {
    async fn send(&mut self, report: &HidReport) -> Result<(), ReportError> {
        match self.mode {
            OutputMode::Usb => self.usb.send(report).await,
            OutputMode::Ble => self.ble.send(report).await,
        }
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "test code")]
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

    #[test]
    fn report_error_display_non_empty_and_distinct() {
        use core::fmt::Write;
        struct Buf([u8; 64], usize);
        impl Write for Buf {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let b = s.as_bytes();
                let end = self.1 + b.len();
                let dest = self.0.get_mut(self.1..end).ok_or(core::fmt::Error)?;
                dest.copy_from_slice(b);
                self.1 = end;
                Ok(())
            }

            fn write_char(&mut self, c: char) -> core::fmt::Result {
                self.write_str(c.encode_utf8(&mut [0; 4]))
            }

            fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> core::fmt::Result {
                core::fmt::write(self, args)
            }
        }
        let mut buf_a = Buf([0; 64], 0);
        let mut buf_b = Buf([0; 64], 0);
        write!(buf_a, "{}", ReportError::NotConnected).expect("write failed");
        write!(buf_b, "{}", ReportError::BufferFull).expect("write failed");
        assert!(buf_a.1 > 0);
        assert!(buf_b.1 > 0);
        let slice_a = buf_a.0.get(..buf_a.1).expect("invalid range");
        let slice_b = buf_b.0.get(..buf_b.1).expect("invalid range");
        assert!(slice_a != slice_b);
    }

    #[test]
    fn output_mode_variants_distinct() {
        assert_ne!(OutputMode::Usb, OutputMode::Ble);
    }
}
