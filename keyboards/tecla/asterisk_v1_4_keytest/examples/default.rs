use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::Flex;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_usb::class::hid::{HidBootProtocol, HidSubclass, HidWriter, State};
use embassy_usb::{Builder, Config};
use panic_probe as _;
use static_cell::StaticCell;
use tecla::matrix::debounce::AsymEagerDefer;
use tecla::keyboard::KeyboardProcessor;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::task::{EventChannel, ReportChannel};
use tecla::action::{Action, KeyCode};
use tecla::pipeline::keymap::{Layer, LayerStack};
use tecla_macros::keyboard_main;
use tecla_rp2040::{CapTouchScanner, UsbHidReporter, KEYBOARD_REPORT_DESCRIPTOR};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

const KEYS: usize = 26;

static EVENT_CHANNEL: StaticCell<EventChannel<NoopRawMutex, 16>> = StaticCell::new();
static REPORT_CHANNEL: StaticCell<ReportChannel<NoopRawMutex, 8>> = StaticCell::new();

static HID_STATE: StaticCell<State> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

// Pad-mapping diagnostic: pin index i (in the `pins` order below, which is
// the same as asterisk_v1_4) types the i-th letter. Press the pads in
// physical order and the typed string gives the pin order.
static LETTERS: [Action; KEYS] = [
    Action::Key(KeyCode::A), Action::Key(KeyCode::B), Action::Key(KeyCode::C),
    Action::Key(KeyCode::D), Action::Key(KeyCode::E), Action::Key(KeyCode::F),
    Action::Key(KeyCode::G), Action::Key(KeyCode::H), Action::Key(KeyCode::I),
    Action::Key(KeyCode::J), Action::Key(KeyCode::K), Action::Key(KeyCode::L),
    Action::Key(KeyCode::M), Action::Key(KeyCode::N), Action::Key(KeyCode::O),
    Action::Key(KeyCode::P), Action::Key(KeyCode::Q), Action::Key(KeyCode::R),
    Action::Key(KeyCode::S), Action::Key(KeyCode::T), Action::Key(KeyCode::U),
    Action::Key(KeyCode::V), Action::Key(KeyCode::W), Action::Key(KeyCode::X),
    Action::Key(KeyCode::Y), Action::Key(KeyCode::Z),
];

#[keyboard_main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(embassy_rp::config::Config::default());

    let driver = Driver::new(p.USB, Irqs);

    let mut config = Config::new(0x9000, 0x0001);
    config.manufacturer = Some("StenoKeyboards");
    config.product = Some("Asterisk v1.4 keytest");
    config.serial_number = Some("AST014");
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

    let scanner = CapTouchScanner::new(pins);
    let debouncer = AsymEagerDefer::new(5);

    let mut layers: heapless::Vec<Layer, 1> = heapless::Vec::new();
    layers.push(Layer::new(&LETTERS)).ok();
    let processor = KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default());

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
    scanner: CapTouchScanner<'static, KEYS>,
    debouncer: AsymEagerDefer<1, KEYS>,
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
) -> ! {
    tecla::task::matrix_task(scanner, debouncer, event_ch.sender()).await
}

#[embassy_executor::task]
async fn keyboard_task(
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
    mut processor: KeyboardProcessor<1, KEYS, 1, KEYS>,
    report_ch: &'static ReportChannel<NoopRawMutex, 8>,
) -> ! {
    tecla::task::keyboard_task(event_ch.receiver(), &mut processor, report_ch.sender()).await
}
