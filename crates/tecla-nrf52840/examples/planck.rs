//! Planck 4×12 ortholinear keyboard example.
//!
//! This example wires a 48-key Planck-style layout to an nRF52840 using:
//! - P0.00–P0.03 : row outputs (active-low)
//! - P0.04–P0.08, P0.11–P0.17 : column inputs (internal pull-up)
//! - USB HID keyboard (6KRO boot protocol)
//!
//! # Flashing
//!
//! ```sh
//! cargo run --example planck --release
//! ```
//!
//! Requires `probe-rs` on PATH and an SWD debugger (e.g. J-Link or CMSIS-DAP).
//!
//! # Keymap
//!
//! ```text
//! Base layer:
//! ┌─────┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬──────┐
//! │ Tab │ Q │ W │ E │ R │ T │ Y │ U │ I │ O │ P │ Bspc │
//! ├─────┼───┼───┼───┼───┼───┼───┼───┼───┼───┼───┼──────┤
//! │ Esc │ A │ S │ D │ F │ G │ H │ J │ K │ L │ ; │ Ent  │
//! ├─────┼───┼───┼───┼───┼───┼───┼───┼───┼───┼───┼──────┤
//! │Shft │ Z │ X │ C │ V │ B │ N │ M │ , │ . │ / │ Shft │
//! ├─────┼───┼───┼───┼───┼───┼───┼───┼───┼───┼───┼──────┤
//! │Ctrl │GUI│Alt│Sft│Fn/│Spc│Spc│Sft│ ← │ ↓ │ ↑ │  →   │
//! │     │   │   │   │Spc│   │   │   │   │   │   │      │
//! └─────┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴──────┘
//!
//! Fn layer (hold Fn/Spc):
//! ┌─────┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬──────┐
//! │  `  │ 1 │ 2 │ 3 │ 4 │ 5 │ 6 │ 7 │ 8 │ 9 │ 0 │ Del  │
//! ├─────┼───┼───┼───┼───┼───┼───┼───┼───┼───┼───┼──────┤
//! │     │F1 │F2 │F3 │F4 │F5 │F6 │ - │ = │ [ │ ] │  \   │
//! ├─────┼───┼───┼───┼───┼───┼───┼───┼───┼───┼───┼──────┤
//! │     │F7 │F8 │F9 │F10│F11│F12│   │   │   │   │      │
//! ├─────┼───┼───┼───┼───┼───┼───┼───┼───┼───┼───┼──────┤
//! │     │   │   │   │   │   │   │   │   │   │   │      │
//! └─────┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴──────┘
//! ```

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! emits unsafe impl blocks")]
#![expect(
    clippy::unwrap_used,
    reason = "embassy task spawn is infallible in firmware entry points"
)]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::bind_interrupts;
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::peripherals::USBD;
use embassy_nrf::usb::vbus_detect::HardwareVbusDetect;
use embassy_nrf::usb::Driver;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::{Builder, Config};
use panic_probe as _;
use static_cell::StaticCell;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::matrix::debounce::AsymEagerDefer;
use tecla::task::{EventChannel, ReportChannel};
use tecla_macros::{keyboard_main, keymap};
use tecla_nrf52840::{GpioMatrixScanner, UsbHidReporter, KEYBOARD_REPORT_DESCRIPTOR};

bind_interrupts!(struct Irqs {
    USBD => embassy_nrf::usb::InterruptHandler<USBD>;
    CLOCK_POWER => embassy_nrf::usb::vbus_detect::InterruptHandler;
});

const ROWS: usize = 4;
const COLS: usize = 12;

static EVENT_CHANNEL: StaticCell<EventChannel<NoopRawMutex, 16>> = StaticCell::new();
static REPORT_CHANNEL: StaticCell<ReportChannel<NoopRawMutex, 8>> = StaticCell::new();

static HID_STATE: StaticCell<State> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

#[keyboard_main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(embassy_nrf::config::Config::default());

    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));

    let mut config = Config::new(0x6666, 0x0001);
    config.manufacturer = Some("Tecla");
    config.product = Some("Planck Keyboard");
    config.serial_number = Some("TECLA001");
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
        Output::new(p.P0_00, Level::High, OutputDrive::Standard),
        Output::new(p.P0_01, Level::High, OutputDrive::Standard),
        Output::new(p.P0_02, Level::High, OutputDrive::Standard),
        Output::new(p.P0_03, Level::High, OutputDrive::Standard),
    ];
    let cols: [Input<'_>; COLS] = [
        Input::new(p.P0_04, Pull::Up),
        Input::new(p.P0_05, Pull::Up),
        Input::new(p.P0_06, Pull::Up),
        Input::new(p.P0_07, Pull::Up),
        Input::new(p.P0_08, Pull::Up),
        Input::new(p.P0_11, Pull::Up),
        Input::new(p.P0_12, Pull::Up),
        Input::new(p.P0_13, Pull::Up),
        Input::new(p.P0_14, Pull::Up),
        Input::new(p.P0_15, Pull::Up),
        Input::new(p.P0_16, Pull::Up),
        Input::new(p.P0_17, Pull::Up),
    ];

    let scanner = GpioMatrixScanner::new(rows, cols);
    let debouncer = AsymEagerDefer::new(5);

    let keymap = keymap! {
        layer!(Base,
            TAB,  Q, W, E, R, T, Y, U, I, O, P,    BSPC,
            ESC,  A, S, D, F, G, H, J, K, L, SCLN, ENT,
            LSFT, Z, X, C, V, B, N, M, COMM, DOT,  SLSH, RSFT,
            LCTL, LGUI, LALT, LSFT, LT!(Fn, SPC), SPC, SPC, RSFT, LEFT, DOWN, UP, RGHT
        ),
        layer!(Fn,
            GRV,  N1, N2, N3, N4,  N5,  N6,  N7,   N8,  N9,   N0,   DEL,
            TRNS, F1, F2, F3, F4,  F5,  F6,  MINS, EQL, LBRC, RBRC, BSLS,
            TRNS, F7, F8, F9, F10, F11, F12, TRNS, TRNS, TRNS, TRNS, TRNS,
            TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS
        ),
    };

    let processor = KeyboardProcessor::new(keymap, TapHoldConfig::default());
    let reporter = UsbHidReporter::new(hid_writer);

    let event_ch = EVENT_CHANNEL.init(EventChannel::new());
    let report_ch = REPORT_CHANNEL.init(ReportChannel::new());

    spawner.spawn(usb_device_task(usb).unwrap());
    spawner.spawn(matrix_task(scanner, debouncer, event_ch).unwrap());
    spawner.spawn(keyboard_task(event_ch, processor, report_ch).unwrap());
    tecla::task::usb_task(report_ch.receiver(), reporter).await;
}

#[embassy_executor::task]
async fn usb_device_task(
    mut device: embassy_usb::UsbDevice<'static, Driver<'static, HardwareVbusDetect>>,
) -> ! {
    device.run().await
}

#[embassy_executor::task]
async fn matrix_task(
    scanner: GpioMatrixScanner<'static, ROWS, COLS>,
    debouncer: AsymEagerDefer<ROWS, COLS>,
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
) -> ! {
    tecla::task::matrix_task(scanner, debouncer, event_ch.sender()).await
}

#[embassy_executor::task]
async fn keyboard_task(
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
    mut processor: KeyboardProcessor<ROWS, COLS, 2, 48>,
    report_ch: &'static ReportChannel<NoopRawMutex, 8>,
) -> ! {
    tecla::task::keyboard_task(event_ch.receiver(), &mut processor, report_ch.sender()).await
}
