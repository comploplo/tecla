use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::yield_now;
use embassy_stm32::gpio::{Flex, Input, Level, Output, Pull, Speed};
use embassy_stm32::peripherals::USB;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_stm32::{bind_interrupts, Config};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{with_timeout, Duration, Instant, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, Sender as CdcSender, State as CdcState};
use embassy_usb::class::midi::{MidiClass, Receiver, Sender};
use embassy_usb::{Builder, Handler};
use panic_probe as _;
use portable_atomic::{AtomicBool, AtomicU32, Ordering};
use static_cell::StaticCell;
use tecla::devices::midi::{output, PadMidiOutput};
use tecla::matrix::debounce::{AsymEagerDefer, Debouncer};
use tecla::matrix::{AsyncMatrixScanner, MatrixState};
use tecla::pipeline::report::MidiEvent;
use tecla::rgb::{LedDriver, Rgb};
use tecla_stm32f3::bitbang_i2c::BitBangI2c;
use tecla_stm32f3::bootloader::{enter_ignition_bootloader, idle_ignition_pins};
use tecla_stm32f3::is31fl3731::{Is31fl3731, ADDR_LEFT, ADDR_RIGHT};
use tecla_stm32f3::mcp23018::{Mcp23018, DEFAULT_ADDRESS};
use tecla_stm32f3::navigator::Navigator;
use tecla_stm32f3::scanner::VoyagerScanner;
use tecla_stm32f3::shared_i2c::SharedI2c;
use tecla_stm32f3::voyager_leds::VoyagerLeds;

use super::expression::{Expression, MotionGate};
use super::instrument::{Instrument, Orientation, CHANNEL, LEDS, MAX_EVENTS, POSITIONS, STOP};
use super::scan_guard::ScanGuard;

bind_interrupts!(struct Irqs {
    USB_LP_CAN_RX0 => InterruptHandler<USB>;
});

type Bus = BitBangI2c<'static>;
type SharedBus = SharedI2c<'static, NoopRawMutex, Bus>;
type Matrix = MatrixState<12, 7>;

const WRITE_BUDGET: Duration = Duration::from_millis(10);
const CONTROL_WRITE_BUDGET: Duration = Duration::from_millis(2);
// The damper can retain pitches across octave changes. Recovery must cover
// every MIDI pitch, rather than just the current physical lattice.
const CLEANUP_NOTES: [u8; 128] = {
    let mut notes = [0; 128];
    let mut i = 0;
    while i < notes.len() {
        notes[i] = i as u8;
        i += 1;
    }
    notes
};

struct ScanSample {
    state: Matrix,
    completed_ms: u64,
    right_ok: bool,
}

struct MotionSample {
    dx: i16,
    dy: i16,
    since_ms: u64,
}

struct LedUpdate {
    frame: [Rgb; LEDS],
    // Recovered chips lost their RAM even if the requested colours match.
    epoch: u32,
}

static SCANS: Channel<CriticalSectionRawMutex, ScanSample, 4> = Channel::new();
static MOTION: Channel<CriticalSectionRawMutex, MotionSample, 4> = Channel::new();
static COLOURS: Signal<CriticalSectionRawMutex, LedUpdate> = Signal::new();
static I2C_BUS: StaticCell<Mutex<NoopRawMutex, Bus>> = StaticCell::new();
static SESSION: AtomicU32 = AtomicU32::new(0);
static USB_SESSION: StaticCell<UsbSession> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
type DiagnosticSignal = Signal<NoopRawMutex, heapless::String<2560>>;
static DIAGNOSTICS: StaticCell<DiagnosticSignal> = StaticCell::new();
static CDC_STATE: StaticCell<CdcState<'static>> = StaticCell::new();
static DIAGNOSTIC_OPEN: AtomicBool = AtomicBool::new(false);
static DIAGNOSTIC_EPOCH: AtomicU32 = AtomicU32::new(0);
static DIAGNOSTIC_DROPPED: AtomicU32 = AtomicU32::new(0);

pub use super::instrument::diagnostics::DiagnosticStatus;

struct UsbSession {
    configured: bool,
}

impl UsbSession {
    fn changed(connected: bool) {
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

fn stm32_config() -> Config {
    use embassy_stm32::rcc::*;
    let mut rcc = embassy_stm32::rcc::Config::default();
    rcc.hse = Some(Hse {
        freq: Hertz(8_000_000),
        mode: HseMode::Oscillator,
    });
    rcc.pll = Some(Pll {
        src: PllSource::HSE,
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL9,
    });
    rcc.sys = Sysclk::PLL1_P;
    rcc.ahb_pre = AHBPrescaler::DIV1;
    rcc.apb1_pre = APBPrescaler::DIV2;
    rcc.apb2_pre = APBPrescaler::DIV2;
    let mut config = embassy_stm32::Config::default();
    config.rcc = rcc;
    config
}

/// A host-testable performance running on the shared Voyager MIDI hardware.
pub trait Performance {
    /// Consume a physical snapshot, including snapshots suppressed by recovery.
    fn update_at(
        &mut self,
        state: &Matrix,
        now_ms: u64,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    );
    /// Colours in physical LED order.
    fn frame(&self) -> [Rgb; LEDS];

    /// Opt this profile into a separate USB CDC diagnostic interface.
    fn diagnostics_enabled(&self) -> bool {
        false
    }

    /// Serialize one bounded authoritative snapshot; false discards the frame.
    fn diagnostic_frame(
        &self,
        _status: DiagnosticStatus,
        _events: &[MidiEvent],
        _out: &mut heapless::String<2560>,
    ) -> bool {
        false
    }
}

impl Performance for Instrument {
    fn update_at(
        &mut self,
        state: &Matrix,
        _now_ms: u64,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) {
        self.update(state, ready, emit);
    }

    fn frame(&self) -> [Rgb; LEDS] {
        Instrument::frame(self)
    }
}

/// Fixed audition profiles keep the same scanner, instrument and expression.
#[allow(
    dead_code,
    reason = "manual Tonnetz entry point; other profiles use run_profile"
)]
pub async fn run(
    spawner: Spawner,
    orientation: Orientation,
    product: &'static str,
    serial: &'static str,
) -> ! {
    run_profile(spawner, Instrument::new(orientation), product, serial, true).await
}

/// Run a MIDI instrument with shared scanning, LEDs, watchdog and USB recovery.
/// Navigator expression is optional so purely keyed instruments do not poll it.
pub async fn run_profile(
    spawner: Spawner,
    mut instrument: impl Performance,
    product: &'static str,
    serial: &'static str,
    navigator_expression: bool,
) -> ! {
    // The Ignition bootloader leaves the app above its 8K flash reservation.
    unsafe {
        (*cortex_m::peripheral::SCB::PTR).vtor.write(0x0800_2000);
    }
    let mut p = embassy_stm32::init(stm32_config());
    let mut boot_sense = Flex::new(p.PA8);
    let mut boot_trigger = Flex::new(p.PA9);
    idle_ignition_pins(&mut boot_sense, &mut boot_trigger);
    let _status_1 = Output::new(p.PB5, Level::Low, Speed::Low);
    let _status_2 = Output::new(p.PB4, Level::Low, Speed::Low);
    let _status_3 = Output::new(p.PB3, Level::Low, Speed::Low);
    // Force a fresh enumeration after the DFU descriptor set.
    {
        let _dp = Output::new(p.PA12.reborrow(), Level::Low, Speed::Low);
        Timer::after_millis(100).await;
    }

    let bus = I2C_BUS.init(Mutex::new(BitBangI2c::new(
        Flex::new(p.PB6),
        Flex::new(p.PB7),
    )));
    let mcp = Mcp23018::new(SharedI2c::new(bus), DEFAULT_ADDRESS);
    let leds = VoyagerLeds::new(
        Is31fl3731::new(SharedI2c::new(bus), ADDR_LEFT),
        Is31fl3731::new(SharedI2c::new(bus), ADDR_RIGHT),
    );
    let driver = Driver::new(p.USB, Irqs, p.PA12, p.PA11);
    let mut config = embassy_usb::Config::new(0x3297, 0x1977);
    config.manufacturer = Some("ZSA Technology Labs");
    config.product = Some(product);
    config.serial_number = Some(serial);
    config.max_power = 100;
    let mut builder = Builder::new(
        driver,
        config,
        USB_DEVICE_BUF.init([0; 256]),
        USB_CONFIG_BUF.init([0; 256]),
        USB_BOS_BUF.init([0; 256]),
        USB_CONTROL_BUF.init([0; 64]),
    );
    builder.handler(USB_SESSION.init(UsbSession { configured: false }));
    let (mut sender, receiver) = MidiClass::new(&mut builder, 1, 1, 64).split();
    let diagnostics: Option<&'static DiagnosticSignal> = if instrument.diagnostics_enabled() {
        let (sender, _receiver) =
            CdcAcmClass::new(&mut builder, CDC_STATE.init(CdcState::new()), 64).split();
        let signal = DIAGNOSTICS.init(Signal::new());
        spawner.spawn(diagnostic_task(sender, signal).unwrap());
        Some(signal)
    } else {
        None
    };
    spawner.spawn(usb_task(builder.build()).unwrap());
    spawner.spawn(midi_input_task(receiver).unwrap());

    let rows = [
        Output::new(p.PB10, Level::Low, Speed::Low),
        Output::new(p.PB11, Level::Low, Speed::Low),
        Output::new(p.PB12, Level::Low, Speed::Low),
        Output::new(p.PB13, Level::Low, Speed::Low),
        Output::new(p.PB14, Level::Low, Speed::Low),
        Output::new(p.PB15, Level::Low, Speed::Low),
    ];
    let cols = [
        Input::new(p.PA0, Pull::Down),
        Input::new(p.PA1, Pull::Down),
        Input::new(p.PA2, Pull::Down),
        Input::new(p.PA3, Pull::Down),
        Input::new(p.PA6, Pull::Down),
        Input::new(p.PA7, Pull::Down),
        Input::new(p.PB0, Pull::Down),
    ];
    let mut scanner = VoyagerScanner::new(rows, cols, mcp);
    for _ in 0..5 {
        let state = match scanner.scan().await {
            Ok(state) => state,
            Err(never) => match never {},
        };
        if state.get(0, 1).unwrap_or(false) {
            enter_ignition_bootloader(&mut boot_sense, &mut boot_trigger).await;
        }
        Timer::after_millis(5).await;
    }
    spawner.spawn(scan_task(scanner).unwrap());
    spawner.spawn(colour_task(leds).unwrap());
    if navigator_expression {
        spawner.spawn(expression_task(Navigator::new(bus)).unwrap());
    }

    let mut expression = Expression::new();
    let mut expression_session = 0;
    let mut motion_gate = MotionGate::new();
    let mut output = PadMidiOutput::new(CHANNEL, CLEANUP_NOTES).unwrap();
    let mut state = Matrix::new();
    let mut health = ScanGuard::new();
    let mut previous_frame = None;
    let mut right_ok = false;
    let mut colour_epoch: u32 = 0;
    let mut last_right_success_ms = None;
    let mut previous_diagnostic = None;
    loop {
        // The watchdog also runs while a scan is waiting on the shared bus.
        // A bounded queue preserves fast release/re-press edges during MIDI IO.
        let sample = with_timeout(Duration::from_millis(1), SCANS.receive())
            .await
            .ok();
        let now_ms = Instant::now().as_millis();
        let was_fresh = health.is_fresh(now_ms);
        let mut recovered_leds = false;
        if let Some(sample) = sample {
            state = sample.state;
            if sample.right_ok {
                health.succeeded(sample.completed_ms);
                last_right_success_ms = Some(sample.completed_ms);
                if !right_ok {
                    colour_epoch = colour_epoch.wrapping_add(1);
                    recovered_leds = true;
                }
            }
            right_ok = sample.right_ok;
        }
        let fresh = health.is_fresh(now_ms);
        // Consume the first recovered snapshot silently: it cannot distinguish
        // a newly touched key from one held through the cable outage.
        let recovered = fresh && !was_fresh;
        let session = SESSION.load(Ordering::Acquire);
        output.observe_session(session);
        if session != expression_session {
            expression.invalidate();
            expression_session = session;
        }
        let can_send = output.is_ready();
        let (stop_row, stop_col) = POSITIONS[STOP];
        let stop = state.get(stop_row, stop_col).unwrap_or(false);
        let accept_motion = navigator_expression && can_send && fresh && !recovered && !stop;
        motion_gate.update(accept_motion, now_ms);
        // Drain even while muted; movements made during recovery cannot replay.
        while let Ok(sample) = MOTION.try_receive() {
            if motion_gate.accepts(sample.since_ms) {
                expression.motion(sample.dx, sample.dy);
            }
        }
        let mut events: heapless::Vec<MidiEvent, MAX_EVENTS> = heapless::Vec::new();
        let mut overflow = false;
        let ready =
            can_send && fresh && !recovered && (!navigator_expression || expression.is_synced());
        let update_started = Instant::now();
        instrument.update_at(&state, now_ms, ready, &mut |event| {
            overflow |= events.push(event).is_err();
        });
        let update_us = update_started
            .elapsed()
            .as_micros()
            .min(u64::from(u32::MAX)) as u32;
        let frame = instrument.frame();
        if previous_frame != Some(frame) || recovered_leds {
            COLOURS.signal(LedUpdate {
                frame,
                epoch: colour_epoch,
            });
            previous_frame = Some(frame);
        }
        if !can_send || overflow {
            events.clear();
        }
        if overflow {
            output.restart_cleanup();
            expression.invalidate();
        }
        let mut tx_fault = overflow;
        let deadline = Instant::now() + WRITE_BUDGET;
        for chunk in events.chunks(output::PACKET_EVENTS) {
            let mut packet = [0; output::PACKET_BYTES];
            let Some(len) = output::pack_events(chunk, &mut packet) else {
                tx_fault = true;
                output.restart_cleanup();
                expression.invalidate();
                break;
            };
            if !send_packet(&mut sender, session, &packet[..len], deadline).await {
                tx_fault = true;
                output.observe_session(SESSION.load(Ordering::Acquire));
                output.restart_cleanup();
                expression.invalidate();
                break;
            }
        }
        if !output.is_ready() && events.is_empty() {
            if let Some(event) = output.next_cleanup() {
                let deadline = Instant::now() + WRITE_BUDGET;
                if send_packet(&mut sender, session, &event.to_usb_packet(), deadline).await {
                    output.cleanup_sent(session, SESSION.load(Ordering::Acquire));
                } else {
                    tx_fault = true;
                    output.observe_session(SESSION.load(Ordering::Acquire));
                }
            }
        }
        // Note edges have priority. Absolute controls can safely retry without
        // clearing a held chord when the host is slow to accept a CC packet.
        if navigator_expression
            && output.is_ready()
            && fresh
            && !recovered
            && !stop
            && Instant::now() < deadline
        {
            if let Some(frame) = expression.pending(Instant::now().as_millis()) {
                let mut controls: heapless::Vec<MidiEvent, 2> = heapless::Vec::new();
                for event in frame.events() {
                    controls.push(event).unwrap();
                }
                let mut packet = [0; output::PACKET_BYTES];
                if let Some(len) = output::pack_events(&controls, &mut packet) {
                    let control_deadline = deadline.min(Instant::now() + CONTROL_WRITE_BUDGET);
                    if send_packet(&mut sender, session, &packet[..len], control_deadline).await {
                        expression.commit(frame);
                    }
                }
            }
        }
        if let Some(signal) = diagnostics.filter(|_| DIAGNOSTIC_OPEN.load(Ordering::Acquire)) {
            let epoch = DIAGNOSTIC_EPOCH.load(Ordering::Acquire);
            let stamp = (
                state.clone(),
                ready,
                right_ok,
                session,
                output.is_ready(),
                recovered,
                tx_fault,
                epoch,
            );
            if previous_diagnostic.as_ref() != Some(&stamp) || !events.is_empty() {
                let replacing = signal.signaled();
                let status = DiagnosticStatus {
                    now_ms,
                    ready,
                    right_age_ms: last_right_success_ms
                        .map_or(u64::MAX, |at| now_ms.saturating_sub(at)),
                    right_ok,
                    usb_session: session,
                    midi_ready: output.is_ready(),
                    recovered,
                    tx_fault,
                    dropped: DIAGNOSTIC_DROPPED
                        .load(Ordering::Relaxed)
                        .wrapping_add(u32::from(replacing)),
                    update_us,
                };
                let mut line = heapless::String::new();
                if instrument.diagnostic_frame(status, &events, &mut line)
                    && (line.ends_with('\n') || line.push('\n').is_ok())
                {
                    if replacing {
                        DIAGNOSTIC_DROPPED.fetch_add(1, Ordering::Relaxed);
                    }
                    signal.signal(line);
                } else {
                    DIAGNOSTIC_DROPPED.fetch_add(1, Ordering::Relaxed);
                }
                previous_diagnostic = Some(stamp);
            }
        }
    }
}

#[embassy_executor::task]
async fn diagnostic_task(
    mut sender: CdcSender<'static, Driver<'static, USB>>,
    signal: &'static DiagnosticSignal,
) -> ! {
    let mut needs_separator = true;
    loop {
        if !sender.dtr() {
            DIAGNOSTIC_OPEN.store(false, Ordering::Release);
            signal.reset();
            needs_separator = true;
            Timer::after_millis(20).await;
            continue;
        }
        if !DIAGNOSTIC_OPEN.swap(true, Ordering::AcqRel) {
            DIAGNOSTIC_EPOCH.fetch_add(1, Ordering::AcqRel);
        }
        let Ok(line) = with_timeout(Duration::from_millis(20), signal.wait()).await else {
            continue;
        };
        if needs_separator && !diagnostic_write(&mut sender, b"\n").await {
            DIAGNOSTIC_DROPPED.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        needs_separator = false;
        for packet in line.as_bytes().chunks(64) {
            if !diagnostic_write(&mut sender, packet).await {
                needs_separator = true;
                break;
            }
            yield_now().await;
        }
        // CDC transfers ending on a full packet need a short packet to flush.
        if !needs_separator
            && line.len().is_multiple_of(64)
            && !diagnostic_write(&mut sender, &[]).await
        {
            needs_separator = true;
        }
        if needs_separator {
            DIAGNOSTIC_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

async fn diagnostic_write(
    sender: &mut CdcSender<'static, Driver<'static, USB>>,
    packet: &[u8],
) -> bool {
    sender.dtr()
        && matches!(
            with_timeout(Duration::from_millis(10), sender.write_packet(packet)).await,
            Ok(Ok(()))
        )
}

async fn send_packet(
    sender: &mut Sender<'static, Driver<'static, USB>>,
    session: u32,
    packet: &[u8],
    deadline: Instant,
) -> bool {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining == Duration::from_millis(0) {
        return false;
    }
    let write = output::in_session(
        session,
        || SESSION.load(Ordering::Acquire),
        sender.write_packet(packet),
    );
    matches!(with_timeout(remaining, write).await, Ok(Some(Ok(()))))
        && SESSION.load(Ordering::Acquire) == session
}

#[embassy_executor::task]
async fn usb_task(mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    device.run().await
}

#[embassy_executor::task]
async fn midi_input_task(mut receiver: Receiver<'static, Driver<'static, USB>>) -> ! {
    let mut packet = [0; 64];
    loop {
        receiver.wait_connection().await;
        while receiver.read_packet(&mut packet).await.is_ok() {}
    }
}

#[embassy_executor::task]
async fn expression_task(mut ball: Navigator<'static, NoopRawMutex, Bus>) -> ! {
    loop {
        if ball.init().await.is_ok() {
            // Reconnection must not replay sensor counts from before its reset.
            if ball.poll_raw().await.is_ok() {
                let mut sampled_at_ms = Instant::now().as_millis();
                loop {
                    Timer::after_millis(7).await;
                    match ball.poll_raw().await {
                        Ok(motion) => {
                            let completed_ms = Instant::now().as_millis();
                            if let Some((dx, dy)) = motion.filter(|&(dx, dy)| dx != 0 || dy != 0) {
                                MOTION
                                    .send(MotionSample {
                                        dx,
                                        dy,
                                        since_ms: sampled_at_ms,
                                    })
                                    .await;
                            }
                            sampled_at_ms = completed_ms;
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        Timer::after_millis(1_000).await;
    }
}

#[embassy_executor::task]
async fn scan_task(mut scanner: VoyagerScanner<'static, SharedBus>) -> ! {
    let mut debounce = AsymEagerDefer::new(5);
    let mut stable = Matrix::new();
    loop {
        Timer::after_millis(1).await;
        let raw = match scanner.scan().await {
            Ok(raw) => raw,
            Err(never) => match never {},
        };
        let completed_ms = Instant::now().as_millis();
        if let Some(next) = debounce.process(&raw, completed_ms) {
            stable = next;
        }
        SCANS
            .send(ScanSample {
                state: stable.clone(),
                completed_ms,
                right_ok: scanner.right_scan_succeeded(),
            })
            .await;
    }
}

#[embassy_executor::task]
async fn colour_task(mut leds: VoyagerLeds<SharedBus>) -> ! {
    let mut update = COLOURS.wait().await;
    let mut initialized_epoch = None;
    loop {
        let initialized = if initialized_epoch != Some(update.epoch) {
            leds.init().await
        } else {
            Ok(())
        };
        if initialized.is_ok() {
            initialized_epoch = Some(update.epoch);
        }
        if initialized.is_ok() && leds.write(&update.frame).await.is_ok() {
            update = COLOURS.wait().await;
        } else {
            // Retry offline chips without animating or queuing old frames.
            if let Ok(latest) = with_timeout(Duration::from_millis(250), COLOURS.wait()).await {
                update = latest;
            }
        }
    }
}
