//! Planck 4×12 ortholinear keyboard example for STM32F411 Blackpill.
//!
//! This example wires a 48-key Planck-style layout to an STM32F411 using:
//! - PA0–PA3   : row outputs (active-low)
//! - PA4–PA7, PB0–PB7 : column inputs (internal pull-up)
//! - USB OTG FS HID keyboard (6KRO boot protocol)
//!
//! # Flashing
//!
//! ```sh
//! cargo run --example planck --release
//! ```
//!
//! Requires `probe-rs` on PATH and an SWD debugger.
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
use embassy_stm32::bind_interrupts;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::peripherals::USB_OTG_FS;
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_stm32::{time::Hertz, Config};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::Builder;
use panic_probe as _;
use static_cell::StaticCell;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::matrix::debounce::AsymEagerDefer;
use tecla::task::{EventChannel, ReportChannel};
use tecla_macros::{keyboard_main, keymap};
use tecla_stm32f4::{GpioMatrixScanner, UsbHidReporter, KEYBOARD_REPORT_DESCRIPTOR};

bind_interrupts!(struct Irqs {
    OTG_FS => InterruptHandler<USB_OTG_FS>;
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
static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();

fn stm32_config() -> Config {
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: Hertz(25_000_000),
            mode: HseMode::Oscillator,
        });
        config.rcc.pll_src = PllSource::HSE;
        config.rcc.pll = Some(Pll {
            prediv: PllPreDiv::DIV25,
            mul: PllMul::MUL192,
            divp: Some(PllPDiv::DIV2),
            divq: Some(PllQDiv::DIV4),
            divr: None,
        });
        config.rcc.ahb_pre = AHBPrescaler::DIV1;
        config.rcc.apb1_pre = APBPrescaler::DIV2;
        config.rcc.apb2_pre = APBPrescaler::DIV1;
        config.rcc.sys = Sysclk::PLL1_P;
    }
    config
}

#[keyboard_main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(stm32_config());

    let driver = Driver::new_fs(
        p.USB_OTG_FS,
        Irqs,
        p.PA12,
        p.PA11,
        EP_OUT_BUFFER.init([0; 256]),
        Default::default(),
    );

    let mut config = embassy_usb::Config::new(0x6666, 0x0002);
    config.manufacturer = Some("Tecla");
    config.product = Some("Planck Keyboard (STM32F4)");
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
        Output::new(p.PA0, Level::High, Speed::Low),
        Output::new(p.PA1, Level::High, Speed::Low),
        Output::new(p.PA2, Level::High, Speed::Low),
        Output::new(p.PA3, Level::High, Speed::Low),
    ];
    let cols: [Input<'_>; COLS] = [
        Input::new(p.PA4, Pull::Up),
        Input::new(p.PA5, Pull::Up),
        Input::new(p.PA6, Pull::Up),
        Input::new(p.PA7, Pull::Up),
        Input::new(p.PB0, Pull::Up),
        Input::new(p.PB1, Pull::Up),
        Input::new(p.PB3, Pull::Up),
        Input::new(p.PB4, Pull::Up),
        Input::new(p.PB5, Pull::Up),
        Input::new(p.PB6, Pull::Up),
        Input::new(p.PB7, Pull::Up),
        Input::new(p.PB8, Pull::Up),
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
    mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB_OTG_FS>>,
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
