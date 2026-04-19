//! Planck 4×12 ortholinear keyboard example for ESP32-S3.
//!
//! This example wires a 48-key Planck-style layout to an ESP32-S3 using:
//! - GPIO 0–3   : row outputs (active-low)
//! - GPIO 4–15  : column inputs (internal pull-up)
//! - USB HID keyboard (6KRO boot protocol) via native USB-OTG
//!
//! # Flashing
//!
//! ```sh
//! cargo run --example planck --release
//! ```
//!
//! Requires `espflash` on PATH. Install with `cargo install espflash`.
//! Requires the esp-rs Xtensa toolchain — install via `espup install`.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::{Builder, Config};
use esp_backtrace as _;
use esp_hal::gpio::{Input, Level, Output, Pull};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_println as _;
use static_cell::StaticCell;
use tecla::debounce::AsymEagerDefer;
use tecla::keyboard::KeyboardProcessor;
use tecla::task::{EventChannel, ReportChannel};
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla_macros::{keyboard_main, keymap};
use tecla_esp32s3::{GpioMatrixScanner, UsbHidReporter, KEYBOARD_REPORT_DESCRIPTOR};

const ROWS: usize = 4;
const COLS: usize = 12;

static EVENT_CHANNEL: StaticCell<EventChannel<NoopRawMutex, 16>> = StaticCell::new();
static REPORT_CHANNEL: StaticCell<ReportChannel<NoopRawMutex, 8>> = StaticCell::new();

static HID_STATE: StaticCell<State> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timer0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timer0.timer0);

    let usb = esp_hal::otg_fs::Usb::new(peripherals.USB0, peripherals.GPIO19, peripherals.GPIO20);
    let driver = esp_hal::otg_fs::UsbBus::new(usb);

    let mut config = Config::new(0x6666, 0x0001);
    config.manufacturer = Some("Tecla");
    config.product = Some("Planck Keyboard (ESP32-S3)");
    config.serial_number = Some("TECLA-ESP001");
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
    };
    let hid_writer: HidWriter<'_, _, 8> =
        HidWriter::new(&mut builder, HID_STATE.init(State::new()), hid_config);

    let usb_device = builder.build();

    let rows: [Output<'_>; ROWS] = [
        Output::new(peripherals.GPIO0, Level::High),
        Output::new(peripherals.GPIO1, Level::High),
        Output::new(peripherals.GPIO2, Level::High),
        Output::new(peripherals.GPIO3, Level::High),
    ];
    let cols: [Input<'_>; COLS] = [
        Input::new(peripherals.GPIO4, Pull::Up),
        Input::new(peripherals.GPIO5, Pull::Up),
        Input::new(peripherals.GPIO6, Pull::Up),
        Input::new(peripherals.GPIO7, Pull::Up),
        Input::new(peripherals.GPIO8, Pull::Up),
        Input::new(peripherals.GPIO9, Pull::Up),
        Input::new(peripherals.GPIO10, Pull::Up),
        Input::new(peripherals.GPIO11, Pull::Up),
        Input::new(peripherals.GPIO12, Pull::Up),
        Input::new(peripherals.GPIO13, Pull::Up),
        Input::new(peripherals.GPIO14, Pull::Up),
        Input::new(peripherals.GPIO15, Pull::Up),
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

    spawner.spawn(usb_device_task(usb_device)).unwrap();
    spawner
        .spawn(matrix_task(scanner, debouncer, event_ch))
        .unwrap();
    spawner
        .spawn(keyboard_task(event_ch, processor, report_ch))
        .unwrap();
    tecla::task::usb_task(report_ch.receiver(), reporter).await;
}

#[embassy_executor::task]
async fn usb_device_task(
    mut device: embassy_usb::UsbDevice<'static, impl embassy_usb::driver::Driver<'static>>,
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
    processor: KeyboardProcessor<ROWS, COLS, 2, 48>,
    report_ch: &'static ReportChannel<NoopRawMutex, 8>,
) -> ! {
    tecla::task::keyboard_task(event_ch.receiver(), processor, report_ch.sender()).await
}
