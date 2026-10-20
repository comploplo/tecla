//! HID output abstraction.
//!
//! [`Reporter`] is the output boundary trait. Implementations send
//! [`HidReport`]s over USB, BLE, or any other transport.
//!
//! A single board can register multiple reporters (keyboard + MIDI +
//! consumer) via a composite implementation.

use super::report::HidReport;

/// An error produced when sending a HID report fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportError {
    /// The transport is not currently connected.
    NotConnected,
    /// The send buffer is full; the report was dropped.
    BufferFull,
    /// This transport has no HID interface for that report variant.
    ///
    /// Returned rather than silently succeeding so a keymap that emits, say,
    /// [`HidReport::Consumer`] on a board
    /// that only enumerates a keyboard interface fails loudly.
    Unsupported,
}

impl core::fmt::Display for ReportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotConnected => write!(f, "transport not connected"),
            Self::BufferFull => write!(f, "send buffer full, report dropped"),
            Self::Unsupported => write!(f, "report variant not supported by this transport"),
        }
    }
}

/// Which [`HidReport`] variants a reporter can actually deliver.
///
/// A transport carries only the variants it has an interface or characteristic
/// for. Reporters expose this so a board can log its real capabilities at boot
/// rather than discovering a missing interface when a keymap first emits a
/// report nothing can carry.
///
/// Lives here rather than next to the USB reporter so it stays testable on the
/// host: `embassy-usb` pulls in `ssmarshal`, which cannot build with `std`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// 6KRO boot-protocol keyboard — always present.
    pub keyboard: bool,
    /// NKRO bitmap keyboard.
    pub nkro: bool,
    /// Consumer control (media keys).
    pub consumer: bool,
    /// Mouse.
    pub mouse: bool,
    /// Gamepad.
    pub gamepad: bool,
    /// USB-MIDI.
    pub midi: bool,
}

impl Capabilities {
    /// Returns `true` if `report` has an interface to be delivered on.
    #[must_use]
    pub const fn supports(self, report: &HidReport) -> bool {
        match report {
            HidReport::Keyboard(_) => self.keyboard,
            HidReport::NkroKeyboard(_) => self.nkro,
            HidReport::Consumer(_) => self.consumer,
            HidReport::Mouse(_) => self.mouse,
            HidReport::Gamepad(_) => self.gamepad,
            HidReport::Midi(_) => self.midi,
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
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test]
    fn report_error_messages() {
        for (err, msg) in [
            (ReportError::NotConnected, "transport not connected"),
            (ReportError::BufferFull, "send buffer full, report dropped"),
            (
                ReportError::Unsupported,
                "report variant not supported by this transport",
            ),
        ] {
            let mut s = heapless::String::<64>::new();
            write!(s, "{err}").unwrap();
            assert_eq!(s, msg);
        }
    }
    use crate::pipeline::report::KeyboardReport;

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
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    use crate::pipeline::report::{
        ConsumerReport, GamepadReport, KeyboardReport, MidiEvent, MouseReport, NkroKeyboardReport,
    };

    const ALL: Capabilities = Capabilities {
        keyboard: true,
        nkro: true,
        consumer: true,
        mouse: true,
        gamepad: true,
        midi: true,
    };

    const KEYBOARD_ONLY: Capabilities = Capabilities {
        keyboard: true,
        nkro: false,
        consumer: false,
        mouse: false,
        gamepad: false,
        midi: false,
    };

    fn one_of_each() -> [HidReport; 6] {
        [
            HidReport::Keyboard(KeyboardReport::new()),
            HidReport::NkroKeyboard(NkroKeyboardReport::new()),
            HidReport::Consumer(ConsumerReport::new()),
            HidReport::Mouse(MouseReport::default()),
            HidReport::Gamepad(GamepadReport::default()),
            HidReport::Midi(MidiEvent::NoteOff {
                channel: 0,
                note: 60,
            }),
        ]
    }

    #[test]
    fn a_fully_populated_transport_supports_every_variant() {
        for r in &one_of_each() {
            assert!(ALL.supports(r), "{r:?} should be supported");
        }
    }

    #[test]
    fn a_keyboard_only_transport_supports_exactly_the_keyboard_report() {
        for r in &one_of_each() {
            assert_eq!(
                KEYBOARD_ONLY.supports(r),
                matches!(r, HidReport::Keyboard(_)),
                "keyboard-only transport gave the wrong answer for {r:?}"
            );
        }
    }

    #[test]
    fn each_flag_gates_exactly_its_own_variant() {
        // Flip one flag on at a time; only that variant may become supported.
        type SetFlag = fn(&mut Capabilities);
        type IsTarget = fn(&HidReport) -> bool;
        let flags: [(SetFlag, IsTarget); 5] = [
            (
                |c| c.nkro = true,
                |r| matches!(r, HidReport::NkroKeyboard(_)),
            ),
            (
                |c| c.consumer = true,
                |r| matches!(r, HidReport::Consumer(_)),
            ),
            (|c| c.mouse = true, |r| matches!(r, HidReport::Mouse(_))),
            (|c| c.gamepad = true, |r| matches!(r, HidReport::Gamepad(_))),
            (|c| c.midi = true, |r| matches!(r, HidReport::Midi(_))),
        ];
        for (set, is_target) in flags {
            let mut caps = KEYBOARD_ONLY;
            set(&mut caps);
            for r in &one_of_each() {
                let expected = matches!(r, HidReport::Keyboard(_)) || is_target(r);
                assert_eq!(caps.supports(r), expected, "wrong gating for {r:?}");
            }
        }
    }
}
