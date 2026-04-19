use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_usb::class::hid::{HidBootProtocol, HidSubclass, HidWriter, State};
use embassy_usb::{Builder, Config};
use panic_probe as _;
use static_cell::StaticCell;
use tecla::matrix::debounce::AsymEagerDefer;
use tecla::keyboard::KeyboardProcessor;
use tecla::devices::steno::steno::StenoEngine;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::task::{EventChannel, ReportChannel};
use tecla_macros::{keyboard_main, steno};
use tecla_rp2040::{UsbHidReporter, KEYBOARD_REPORT_DESCRIPTOR};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

const ROWS: usize = 2;
const COLS: usize = 6;

static EVENT_CHANNEL: StaticCell<EventChannel<NoopRawMutex, 16>> = StaticCell::new();
static REPORT_CHANNEL: StaticCell<ReportChannel<NoopRawMutex, 8>> = StaticCell::new();

static HID_STATE: StaticCell<State> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

include!(concat!(env!("OUT_DIR"), "/lapwing_dict.rs"));

#[keyboard_main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(embassy_rp::config::Config::default());

    let driver = Driver::new(p.USB, Irqs);

    let mut config = Config::new(0x6666, 0x0002);
    config.manufacturer = Some("Tecla");
    config.product = Some("Uni V4 Steno");
    config.serial_number = Some("UNI004");
    config.max_power = 100;

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
        hid_boot_protocol: HidBootProtocol::None,
        hid_subclass: HidSubclass::No,
    };
    let hid_writer: HidWriter<'_, _, 8> =
        HidWriter::new(&mut builder, HID_STATE.init(State::new()), hid_config);

    let usb = builder.build();

    let rows: [Output<'_>; ROWS] = [
        Output::new(p.PIN_26, Level::High),
        Output::new(p.PIN_27, Level::High),
    ];
    let cols: [Input<'_>; COLS] = [
        Input::new(p.PIN_2, Pull::Up),
        Input::new(p.PIN_3, Pull::Up),
        Input::new(p.PIN_4, Pull::Up),
        Input::new(p.PIN_5, Pull::Up),
        Input::new(p.PIN_6, Pull::Up),
        Input::new(p.PIN_7, Pull::Up),
    ];

    let scanner = tecla_rp2040::GpioMatrixScanner::new(rows, cols);
    let debouncer = AsymEagerDefer::new(5);

    let keymap = include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../keyboards/tecla/steno_uni_v4/keymaps/steno.rs"
    ));

    let steno_engine = StenoEngine::new(LAPWING_DICT);
    let processor =
        KeyboardProcessor::new(keymap, TapHoldConfig::default()).with_steno(steno_engine);

    let reporter = UsbHidReporter::new(hid_writer);

    let event_ch = EVENT_CHANNEL.init(EventChannel::new());
    let report_ch = REPORT_CHANNEL.init(ReportChannel::new());

    spawner.spawn(usb_device_task(usb).unwrap());
    spawner.spawn(matrix_task(scanner, debouncer, event_ch).unwrap());
    spawner.spawn(keyboard_task(event_ch, processor, report_ch).unwrap());
    tecla::task::usb_task(report_ch.receiver(), reporter).await;
}

#[embassy_executor::task]
async fn usb_device_task(mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    device.run().await
}

#[embassy_executor::task]
async fn matrix_task(
    scanner: tecla_rp2040::GpioMatrixScanner<'static, ROWS, COLS>,
    debouncer: AsymEagerDefer<ROWS, COLS>,
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
) -> ! {
    tecla::task::matrix_task(scanner, debouncer, event_ch.sender()).await
}

#[embassy_executor::task]
async fn keyboard_task(
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
    processor: KeyboardProcessor<ROWS, COLS, 1, 12>,
    report_ch: &'static ReportChannel<NoopRawMutex, 8>,
) -> ! {
    tecla::task::keyboard_task(event_ch.receiver(), processor, report_ch.sender()).await
}
