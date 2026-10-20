use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::Flex;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{with_timeout, Duration, Instant, Timer};
use embassy_usb::class::midi::{MidiClass, Sender};
use embassy_usb::{Builder, Config, Handler};
use panic_probe as _;
use portable_atomic::{AtomicU32, Ordering};
use static_cell::StaticCell;
use tecla::devices::midi::PadMidiOutput;
use tecla::matrix::{AsyncMatrixScanner, MatrixState};
use tecla::pipeline::report::MidiEvent;
use tecla_rp2040::CapTouchScanner;

/// Capacitive pads in the hardware-verified Asterisk scanner order.
pub const KEYS: usize = 26;

mod output {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../keyboards/tecla/asterisk_v1_4_midi/output.rs"
    ));
}

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

const WRITE_BUDGET: Duration = Duration::from_millis(10);
// Chord changes can release old strings and attack new ones in one scan.
const SCAN_EVENTS: usize = 64;

// Bit 0 is connection state; the other bits distinguish successive sessions.
static SESSION: AtomicU32 = AtomicU32::new(0);
static USB_SESSION: StaticCell<UsbSession> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

struct UsbSession {
    configured: bool,
}

impl UsbSession {
    fn changed(connected: bool) {
        // Only USB callbacks write this token; the scan loop only reads it.
        let generation = SESSION.load(Ordering::Relaxed).wrapping_add(2) & !1;
        SESSION.store(generation | u32::from(connected), Ordering::Release);
    }
}

impl Handler for UsbSession {
    fn enabled(&mut self, enabled: bool) {
        if !enabled {
            self.configured = false;
            Self::changed(false);
        }
    }

    fn reset(&mut self) {
        self.configured = false;
        Self::changed(false);
    }

    fn configured(&mut self, configured: bool) {
        self.configured = configured;
        Self::changed(configured);
    }

    fn suspended(&mut self, suspended: bool) {
        Self::changed(self.configured && !suspended);
    }
}

/// Runs an Asterisk MIDI instrument with the same scanner and USB recovery.
pub async fn run<const N: usize>(
    spawner: Spawner,
    product: &'static str,
    serial: &'static str,
    channel: u8,
    notes: [u8; N],
    is_stop: fn(&MatrixState<1, KEYS>) -> bool,
    mut process: impl FnMut(&MatrixState<1, KEYS>, u64, bool, &mut dyn FnMut(MidiEvent)),
) -> ! {
    let p = embassy_rp::init(embassy_rp::config::Config::default());
    let driver = Driver::new(p.USB, Irqs);
    let mut config = Config::new(0x9000, 0x0001);
    config.manufacturer = Some("StenoKeyboards");
    config.product = Some(product);
    config.serial_number = Some(serial);

    let mut builder = Builder::new(
        driver,
        config,
        USB_DEVICE_BUF.init([0; 256]),
        USB_CONFIG_BUF.init([0; 256]),
        USB_BOS_BUF.init([0; 256]),
        USB_CONTROL_BUF.init([0; 64]),
    );
    builder.handler(USB_SESSION.init(UsbSession { configured: false }));
    let midi = MidiClass::new(&mut builder, 1, 1, 64);
    let (mut sender, _receiver) = midi.split();
    spawner.spawn(usb_device_task(builder.build()).unwrap());

    // Preserve the working Asterisk scanner order and its boot calibration.
    let pins: [Flex<'_>; KEYS] = [
        Flex::new(p.PIN_0),
        Flex::new(p.PIN_25),
        Flex::new(p.PIN_5),
        Flex::new(p.PIN_4),
        Flex::new(p.PIN_3),
        Flex::new(p.PIN_2),
        Flex::new(p.PIN_1),
        Flex::new(p.PIN_15),
        Flex::new(p.PIN_24),
        Flex::new(p.PIN_23),
        Flex::new(p.PIN_22),
        Flex::new(p.PIN_21),
        Flex::new(p.PIN_20),
        Flex::new(p.PIN_9),
        Flex::new(p.PIN_8),
        Flex::new(p.PIN_7),
        Flex::new(p.PIN_6),
        Flex::new(p.PIN_14),
        Flex::new(p.PIN_19),
        Flex::new(p.PIN_18),
        Flex::new(p.PIN_17),
        Flex::new(p.PIN_16),
        Flex::new(p.PIN_10),
        Flex::new(p.PIN_11),
        Flex::new(p.PIN_12),
        Flex::new(p.PIN_13),
    ];
    let mut scanner = CapTouchScanner::new(pins);
    let mut previous = MatrixState::<1, { KEYS }>::new();
    let mut output = PadMidiOutput::new(channel, notes).unwrap();
    loop {
        // A relative delay always yields to USB, even when a capacitive scan
        // takes longer than 1ms. A catch-up ticker could starve that task.
        Timer::after_millis(1).await;
        let stable = match scanner.scan().await {
            Ok(stable) => stable,
            Err(never) => match never {},
        };
        let now_ms = Instant::now().as_millis();
        // CapTouchScanner already qualifies touch and release edges.
        let stop_held = is_stop(&stable);
        let stop_pressed = stop_held && !is_stop(&previous);
        previous = stable;

        let session = SESSION.load(Ordering::Acquire);
        output.observe_session(session);
        let send_stop_releases = stop_pressed && output.is_ready();
        if stop_pressed {
            output.restart_cleanup();
        }
        let ready = output.is_ready() && !stop_held;
        let mut batch: heapless::Vec<MidiEvent, SCAN_EVENTS> = heapless::Vec::new();
        let mut overflow = false;
        process(&previous, now_ms, ready, &mut |event| {
            overflow |= batch.push(event).is_err();
        });
        if overflow {
            output.restart_cleanup();
            defmt::warn!("midi: scan batch overflow; clearing notes");
        }
        if send_stop_releases && !overflow {
            // The instrument can release its known notes immediately; the
            // recovery sweep still follows. Suppressed attacks never escape.
            batch.retain(|event| matches!(event, MidiEvent::NoteOff { .. }));
        } else if !ready || overflow {
            batch.clear();
        }

        // The entire batch shares a deadline, so 25 notes cannot stall 25 times.
        let deadline = Instant::now() + WRITE_BUDGET;
        for events in batch.chunks(output::PACKET_EVENTS) {
            let mut packet = [0; output::PACKET_BYTES];
            let Some(len) = output::pack_events(events, &mut packet) else {
                output.restart_cleanup();
                defmt::warn!("midi: packet overflow; clearing notes");
                break;
            };
            if !send_packet(&mut sender, session, &packet[..len], deadline).await {
                output.observe_session(SESSION.load(Ordering::Acquire));
                output.restart_cleanup();
                defmt::warn!("midi: output interrupted; clearing notes");
                break;
            }
        }

        // One cleanup packet per scan leaves the pads responsive. A Stop
        // release batch uses this scan's budget; begin its sweep next scan.
        if !output.is_ready() && batch.is_empty() {
            if let Some(event) = output.next_cleanup() {
                let deadline = Instant::now() + WRITE_BUDGET;
                if send_packet(&mut sender, session, &event.to_usb_packet(), deadline).await {
                    output.cleanup_sent(session, SESSION.load(Ordering::Acquire));
                } else {
                    // Retain the cursor on a slow host so later notes progress.
                    output.observe_session(SESSION.load(Ordering::Acquire));
                }
            }
        }
    }
}

async fn send_packet(
    sender: &mut Sender<'static, Driver<'static, USB>>,
    session: u32,
    packet: &[u8],
    deadline: Instant,
) -> bool {
    if session & 1 == 0 || SESSION.load(Ordering::Acquire) != session {
        return false;
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining == Duration::from_millis(0) {
        return false;
    }
    let write = output::in_session(
        session,
        || SESSION.load(Ordering::Acquire),
        sender.write_packet(packet),
    );
    let result = with_timeout(remaining, write).await;
    let unchanged = SESSION.load(Ordering::Acquire) == session;
    matches!(result, Ok(Some(Ok(())))) && unchanged
}

#[embassy_executor::task]
async fn usb_device_task(mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    device.run().await
}
