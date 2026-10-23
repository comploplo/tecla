//! Split keyboard example — primary half.
//!
//! Demonstrates the split-keyboard topology on RP2040:
//! - Local matrix scan for the left half (4×6)
//! - Remote event reception for the right half via [`SplitTransport`]
//! - Both feed the same [`EventChannel`], merged by a single [`keyboard_task`]
//!
//! The transport is a placeholder — replace `DummyTransport` with a real
//! UART / I2C / SPI implementation for your board.
//!
//! # Wiring (left half)
//!
//! - GPIO 0–3  : row outputs (active-low)
//! - GPIO 4–9  : column inputs (internal pull-up)
//! - USB HID keyboard (6KRO boot protocol)

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! emits unsafe impl blocks")]
#![expect(
    clippy::unwrap_used,
    reason = "embassy task spawn is infallible in firmware entry points"
)]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::{Builder, Config};
use panic_probe as _;
use static_cell::StaticCell;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::matrix::debounce::AsymEagerDefer;
use tecla::matrix::split::SplitConfig;
use tecla::pipeline::event::KeyEvent;
use tecla::task::{EventChannel, ReportChannel};
use tecla::transport::SplitTransport;
use tecla_macros::{keyboard_main, keymap};
use tecla_rp2040::{CompositeUsbReporter, GpioMatrixScanner, KEYBOARD_REPORT_DESCRIPTOR};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

const ROWS: usize = 4;
const LEFT_COLS: usize = 6;
const TOTAL_COLS: usize = 12;

static EVENT_CHANNEL: StaticCell<EventChannel<NoopRawMutex, 16>> = StaticCell::new();
static REPORT_CHANNEL: StaticCell<ReportChannel<NoopRawMutex, 8>> = StaticCell::new();

static HID_STATE: StaticCell<State> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

struct DummyTransport;

impl SplitTransport for DummyTransport {
    type Error = core::convert::Infallible;

    async fn send(&mut self, _event: KeyEvent) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn recv(&mut self) -> Result<KeyEvent, Self::Error> {
        core::future::pending().await
    }
}

#[keyboard_main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(embassy_rp::config::Config::default());

    let driver = Driver::new(p.USB, Irqs);

    let mut config = Config::new(0x6666, 0x0002);
    config.manufacturer = Some("Tecla");
    config.product = Some("Split Keyboard (Left)");
    config.serial_number = Some("TECLA002");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let mut builder = Builder::new(
        driver,
        config,
        USB_DEVICE_BUF.init([0; 256]),
        USB_CONFIG_BUF.init([0; 256]),
        USB_BOS_BUF.init([0; 256]),
        USB_CONTROL_BUF.init([0; 64]),
    );

    let hid_config = embassy_usb::class::hid::Config {
        report_descriptor: KEYBOARD_REPORT_DESCRIPTOR,
        request_handler: None,
        poll_ms: 1,
        max_packet_size: 8,
        hid_subclass: embassy_usb::class::hid::HidSubclass::Boot,
        hid_boot_protocol: embassy_usb::class::hid::HidBootProtocol::Keyboard,
    };
    let hid_writer: HidWriter<'_, _, 8> =
        HidWriter::new(&mut builder, HID_STATE.init(State::new()), hid_config);

    let usb = builder.build();

    let rows: [Output<'_>; ROWS] = [
        Output::new(p.PIN_0, Level::High),
        Output::new(p.PIN_1, Level::High),
        Output::new(p.PIN_2, Level::High),
        Output::new(p.PIN_3, Level::High),
    ];
    let cols: [Input<'_>; LEFT_COLS] = [
        Input::new(p.PIN_4, Pull::Up),
        Input::new(p.PIN_5, Pull::Up),
        Input::new(p.PIN_6, Pull::Up),
        Input::new(p.PIN_7, Pull::Up),
        Input::new(p.PIN_8, Pull::Up),
        Input::new(p.PIN_9, Pull::Up),
    ];

    let scanner = GpioMatrixScanner::new(rows, cols);
    let debouncer = AsymEagerDefer::new(5);

    let keymap = keymap! {
        layer!(Base,
            TAB,  Q, W, E, R, T,    Y, U, I, O, P,    BSPC,
            ESC,  A, S, D, F, G,    H, J, K, L, SCLN, ENT,
            LSFT, Z, X, C, V, B,    N, M, COMM, DOT,  SLSH, RSFT,
            LCTL, LGUI, LALT, LSFT, LT!(Fn, SPC), SPC, SPC, RSFT, LEFT, DOWN, UP, RGHT
        ),
        layer!(Fn,
            GRV,  N1, N2, N3, N4,  N5,  N6,  N7,   N8,  N9,   N0,   DEL,
            TRNS, F1, F2, F3, F4,  F5,  F6,  MINS, EQL, LBRC, RBRC, BSLS,
            TRNS, F7, F8, F9, F10, F11, F12, TRNS, TRNS, TRNS, TRNS, TRNS,
            TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS
        ),
    };

    let processor =
        KeyboardProcessor::<ROWS, TOTAL_COLS, 2, 48>::new(keymap, TapHoldConfig::default());
    let reporter = CompositeUsbReporter::new(hid_writer);
    let _split_cfg = SplitConfig::primary();
    let transport = DummyTransport;

    let event_ch = EVENT_CHANNEL.init(EventChannel::new());
    let report_ch = REPORT_CHANNEL.init(ReportChannel::new());

    spawner.spawn(usb_device_task(usb).unwrap());
    spawner.spawn(matrix_task(scanner, debouncer, event_ch).unwrap());
    spawner.spawn(remote_event_task(transport, event_ch).unwrap());
    spawner.spawn(keyboard_task(event_ch, processor, report_ch).unwrap());
    tecla::task::usb_task(report_ch.receiver(), reporter).await;
}

#[embassy_executor::task]
async fn usb_device_task(mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    device.run().await
}

#[embassy_executor::task]
async fn matrix_task(
    scanner: GpioMatrixScanner<'static, ROWS, LEFT_COLS>,
    debouncer: AsymEagerDefer<ROWS, LEFT_COLS>,
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
) -> ! {
    tecla::task::matrix_task(scanner, debouncer, event_ch.sender()).await
}

#[embassy_executor::task]
async fn remote_event_task(
    transport: DummyTransport,
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
) -> ! {
    tecla::task::remote_event_task(transport, event_ch.sender()).await
}

#[embassy_executor::task]
async fn keyboard_task(
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
    mut processor: KeyboardProcessor<ROWS, TOTAL_COLS, 2, 48>,
    report_ch: &'static ReportChannel<NoopRawMutex, 8>,
) -> ! {
    tecla::task::keyboard_task(event_ch.receiver(), &mut processor, report_ch.sender()).await
}
