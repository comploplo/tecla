//! Composite USB reporter — every [`HidReport`] variant on its own interface.
//!
//! A keyboard that only enumerates the 8-byte boot-protocol interface can
//! carry [`HidReport::Keyboard`] and nothing else: media keys, NKRO, mouse,
//! gamepad, and MIDI have no endpoint to go to. [`CompositeUsbReporter`] holds
//! one writer per variant and routes each report to the matching interface.
//!
//! Interfaces are opt-in, because every extra one costs USB endpoints and
//! descriptor space, and boards differ in what they can spare. A report whose
//! interface was not registered returns [`ReportError::Unsupported`] rather
//! than being silently dropped — a keymap that emits media keys on a
//! keyboard-only build should fail visibly, not do nothing.
//!
//! # Usage
//!
//! Build each interface with the matching descriptor from
//! [`report`](crate::pipeline::report), then register the writers:
//!
//! ```text
//! let reporter = CompositeUsbReporter::new(keyboard_writer)
//!     .with_consumer(consumer_writer)   // CONSUMER_REPORT_DESCRIPTOR
//!     .with_mouse(mouse_writer);        // MOUSE_REPORT_DESCRIPTOR
//! ```

use embassy_usb::class::hid::HidWriter;
#[cfg(feature = "usb-midi")]
use embassy_usb::class::midi::MidiClass;
use embassy_usb::driver::Driver;

use crate::pipeline::hid::{AsyncReporter, Capabilities, ReportError};
use crate::pipeline::report::{
    HidReport, CONSUMER_REPORT_SIZE, GAMEPAD_REPORT_SIZE, KEYBOARD_REPORT_SIZE, MOUSE_REPORT_SIZE,
    NKRO_REPORT_SIZE,
};

/// Routes each [`HidReport`] variant to its own USB interface.
///
/// The keyboard interface is mandatory; everything else is opt-in via the
/// `with_*` builders. See the [module docs](self) for the descriptor each
/// interface must be built with.
pub struct CompositeUsbReporter<'d, D: Driver<'d>> {
    keyboard: HidWriter<'d, D, KEYBOARD_REPORT_SIZE>,
    nkro: Option<HidWriter<'d, D, NKRO_REPORT_SIZE>>,
    consumer: Option<HidWriter<'d, D, CONSUMER_REPORT_SIZE>>,
    mouse: Option<HidWriter<'d, D, MOUSE_REPORT_SIZE>>,
    gamepad: Option<HidWriter<'d, D, GAMEPAD_REPORT_SIZE>>,
    #[cfg(feature = "usb-midi")]
    midi: Option<MidiClass<'d, D>>,
}

impl<'d, D: Driver<'d>> CompositeUsbReporter<'d, D> {
    /// Creates a reporter with only the 6KRO boot-protocol keyboard interface.
    ///
    /// Build `keyboard` with
    /// [`KEYBOARD_REPORT_DESCRIPTOR`](crate::pipeline::report::KEYBOARD_REPORT_DESCRIPTOR).
    #[must_use]
    pub fn new(keyboard: HidWriter<'d, D, KEYBOARD_REPORT_SIZE>) -> Self {
        Self {
            keyboard,
            nkro: None,
            consumer: None,
            mouse: None,
            gamepad: None,
            #[cfg(feature = "usb-midi")]
            midi: None,
        }
    }

    /// Adds the NKRO keyboard interface
    /// ([`NKRO_REPORT_DESCRIPTOR`](crate::pipeline::report::NKRO_REPORT_DESCRIPTOR)).
    ///
    /// Required for [`RolloverMode::Nkro`](crate::keyboard::RolloverMode::Nkro).
    #[must_use]
    pub fn with_nkro(mut self, writer: HidWriter<'d, D, NKRO_REPORT_SIZE>) -> Self {
        self.nkro = Some(writer);
        self
    }

    /// Adds the consumer-control interface
    /// ([`CONSUMER_REPORT_DESCRIPTOR`](crate::pipeline::report::CONSUMER_REPORT_DESCRIPTOR)).
    ///
    /// Required for media keys.
    #[must_use]
    pub fn with_consumer(mut self, writer: HidWriter<'d, D, CONSUMER_REPORT_SIZE>) -> Self {
        self.consumer = Some(writer);
        self
    }

    /// Adds the mouse interface
    /// ([`MOUSE_REPORT_DESCRIPTOR`](crate::pipeline::report::MOUSE_REPORT_DESCRIPTOR)).
    #[must_use]
    pub fn with_mouse(mut self, writer: HidWriter<'d, D, MOUSE_REPORT_SIZE>) -> Self {
        self.mouse = Some(writer);
        self
    }

    /// Adds the gamepad interface
    /// ([`GAMEPAD_REPORT_DESCRIPTOR`](crate::pipeline::report::GAMEPAD_REPORT_DESCRIPTOR)).
    #[must_use]
    pub fn with_gamepad(mut self, writer: HidWriter<'d, D, GAMEPAD_REPORT_SIZE>) -> Self {
        self.gamepad = Some(writer);
        self
    }

    /// Adds a USB-MIDI interface (`embassy_usb::class::midi::MidiClass`).
    ///
    /// Requires the `usb-midi` feature — the MIDI class costs roughly 4KB of
    /// flash, which is not free on a smaller MCU.
    #[cfg(feature = "usb-midi")]
    #[must_use]
    pub fn with_midi(mut self, midi: MidiClass<'d, D>) -> Self {
        self.midi = Some(midi);
        self
    }

    /// Returns which report variants this reporter can actually deliver.
    ///
    /// Useful at boot to log the enumerated interfaces, so a missing one is
    /// visible before a keymap starts emitting reports nothing can carry.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        Capabilities {
            keyboard: true,
            nkro: self.nkro.is_some(),
            consumer: self.consumer.is_some(),
            mouse: self.mouse.is_some(),
            gamepad: self.gamepad.is_some(),
            #[cfg(feature = "usb-midi")]
            midi: self.midi.is_some(),
            #[cfg(not(feature = "usb-midi"))]
            midi: false,
        }
    }
}

impl<'d, D: Driver<'d>> AsyncReporter for CompositeUsbReporter<'d, D> {
    async fn send(&mut self, report: &HidReport) -> Result<(), ReportError> {
        // Each arm encodes with the report type's own to_bytes(), so the wire
        // layout lives with the type and its tests, not here.
        match report {
            HidReport::Keyboard(r) => write_hid(&mut self.keyboard, &r.to_bytes()).await,
            HidReport::NkroKeyboard(r) => match self.nkro.as_mut() {
                Some(w) => write_hid(w, &r.to_bytes()).await,
                None => Err(ReportError::Unsupported),
            },
            HidReport::Consumer(r) => match self.consumer.as_mut() {
                Some(w) => write_hid(w, &r.to_bytes()).await,
                None => Err(ReportError::Unsupported),
            },
            HidReport::Mouse(r) => match self.mouse.as_mut() {
                Some(w) => write_hid(w, &r.to_bytes()).await,
                None => Err(ReportError::Unsupported),
            },
            HidReport::Gamepad(r) => match self.gamepad.as_mut() {
                Some(w) => write_hid(w, &r.to_bytes()).await,
                None => Err(ReportError::Unsupported),
            },
            #[cfg(feature = "usb-midi")]
            HidReport::Midi(e) => match self.midi.as_mut() {
                Some(m) => m
                    .write_packet(&e.to_usb_packet())
                    .await
                    .map_err(map_endpoint_error),
                None => Err(ReportError::Unsupported),
            },
            #[cfg(not(feature = "usb-midi"))]
            HidReport::Midi(_) => Err(ReportError::Unsupported),
        }
    }
}

async fn write_hid<'d, D: Driver<'d>, const N: usize>(
    writer: &mut HidWriter<'d, D, N>,
    bytes: &[u8],
) -> Result<(), ReportError> {
    writer.write(bytes).await.map_err(map_endpoint_error)
}

fn map_endpoint_error(e: embassy_usb::driver::EndpointError) -> ReportError {
    match e {
        // The host has not configured the endpoint (unplugged, suspended, or
        // still enumerating).
        embassy_usb::driver::EndpointError::Disabled => ReportError::NotConnected,
        // The report is larger than the endpoint's max packet size — a
        // descriptor/report-size mismatch, not a transient condition.
        embassy_usb::driver::EndpointError::BufferOverflow => ReportError::BufferFull,
    }
}

/// Pumps Via raw-HID packets between USB and the keyboard task.
///
/// The Via protocol mutates the keymap, so it needs `&mut KeyboardProcessor` —
/// which the keyboard task already owns exclusively. Rather than share the
/// processor behind a mutex, this task does only the USB half: it reads a
/// 32-byte command, hands it to the keyboard task over `req_tx`, waits for the
/// reply on `resp_rx`, and writes it back. Pair with
/// [`service_via`](crate::task::service_via) on the other end.
///
/// This function never returns.
#[cfg(feature = "via")]
pub async fn via_usb_task<'d, D, M, const N: usize>(
    mut hid: embassy_usb::class::hid::HidReaderWriter<
        'd,
        D,
        { crate::pipeline::report::VIA_REPORT_SIZE },
        { crate::pipeline::report::VIA_REPORT_SIZE },
    >,
    req_tx: embassy_sync::channel::Sender<'_, M, [u8; crate::pipeline::report::VIA_REPORT_SIZE], N>,
    resp_rx: embassy_sync::channel::Receiver<
        '_,
        M,
        [u8; crate::pipeline::report::VIA_REPORT_SIZE],
        N,
    >,
) -> !
where
    D: Driver<'d>,
    M: embassy_sync::blocking_mutex::raw::RawMutex,
{
    let mut buf = [0u8; crate::pipeline::report::VIA_REPORT_SIZE];
    loop {
        match hid.read(&mut buf).await {
            Ok(_) => {
                req_tx.send(buf).await;
                let response = resp_rx.receive().await;
                if let Err(e) = hid.write(&response).await {
                    defmt::warn!("via: response dropped: {}", defmt::Debug2Format(&e));
                }
            }
            Err(e) => defmt::warn!("via: read failed: {}", defmt::Debug2Format(&e)),
        }
    }
}
