//! Core1 autocorrect split — runs the autocorrect engine on the RP2040's
//! second core while core0 keeps the matrix→HID path real-time.
//!
//! Core0: USB, matrix scan, [`keyboard_task_autocorrect`] (forwards typed
//! keycodes out, applies corrections in).
//! Core1: [`autocorrect_task`] — the edit-distance trie search lives here, so
//! the (occasionally expensive) lookup never stalls scanning or HID reports.
//!
//! The two are joined by a `KeyCode` channel (core0→core1) and a `Correction`
//! channel (core1→core0), both `CriticalSectionRawMutex` because they cross
//! cores.
//!
//! ## Single-core fallback
//!
//! The exact same `autocorrect_task` runs fine on a single core — drop the
//! `spawn_core1` block and `spawner.spawn(autocorrect_core1(...))` onto the
//! main executor with `NoopRawMutex` channels. The "core1 split" is purely a
//! spawn-site choice; the task body is identical. (For the simplest single-core
//! setup, skip the split entirely and use `KeyboardProcessor::with_autocorrect`
//! + `keyboard_task` — the inline path.)
//!
//! # Flashing
//!
//! ```sh
//! cargo run --example core1_autocorrect --features core1-autocorrect --release
//! ```

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! and static-mut core1 stack")]
#![expect(
    clippy::unwrap_used,
    reason = "embassy task spawn is infallible in firmware entry points"
)]

use defmt_rtt as _;
use embassy_executor::{Executor, Spawner};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::{Builder, Config};
use panic_probe as _;
use static_cell::StaticCell;
use tecla::autocorrect::{AutocorrectEngine, Correction};
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::matrix::debounce::AsymEagerDefer;
use tecla::prelude::KeyCode;
use tecla::task::{EventChannel, ReportChannel};
use tecla_macros::{keyboard_main, keymap};
use tecla_rp2040::{CompositeUsbReporter, GpioMatrixScanner, KEYBOARD_REPORT_DESCRIPTOR};

mod dict {
    include!(concat!(env!("OUT_DIR"), "/autocorrect_demo_dict.rs"));
}

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

const ROWS: usize = 4;
const COLS: usize = 12;
const LAYERS: usize = 2;
const KEYS: usize = 48;

// All four channels share one mutex type because keyboard_task_autocorrect is
// generic over a single mutex; the cross-core ones force CriticalSectionRawMutex.
type KcChannel = Channel<CriticalSectionRawMutex, KeyCode, 32>;
type CorrChannel = Channel<CriticalSectionRawMutex, Correction, 8>;

static EVENT_CHANNEL: StaticCell<EventChannel<CriticalSectionRawMutex, 16>> = StaticCell::new();
static REPORT_CHANNEL: StaticCell<ReportChannel<CriticalSectionRawMutex, 8>> = StaticCell::new();
static KC_CHANNEL: StaticCell<KcChannel> = StaticCell::new();
static CORR_CHANNEL: StaticCell<CorrChannel> = StaticCell::new();

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

static HID_STATE: StaticCell<State> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

#[keyboard_main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(embassy_rp::config::Config::default());

    let driver = Driver::new(p.USB, Irqs);

    let mut config = Config::new(0x6666, 0x0001);
    config.manufacturer = Some("Tecla");
    config.product = Some("Planck (core1 autocorrect)");
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
        Output::new(p.PIN_0, Level::High),
        Output::new(p.PIN_1, Level::High),
        Output::new(p.PIN_2, Level::High),
        Output::new(p.PIN_3, Level::High),
    ];
    let cols: [Input<'_>; COLS] = [
        Input::new(p.PIN_4, Pull::Up),
        Input::new(p.PIN_5, Pull::Up),
        Input::new(p.PIN_6, Pull::Up),
        Input::new(p.PIN_7, Pull::Up),
        Input::new(p.PIN_8, Pull::Up),
        Input::new(p.PIN_9, Pull::Up),
        Input::new(p.PIN_10, Pull::Up),
        Input::new(p.PIN_11, Pull::Up),
        Input::new(p.PIN_12, Pull::Up),
        Input::new(p.PIN_13, Pull::Up),
        Input::new(p.PIN_14, Pull::Up),
        Input::new(p.PIN_15, Pull::Up),
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

    let processor =
        KeyboardProcessor::new(keymap, TapHoldConfig::default()).with_autocorrect_offload();
    let reporter = CompositeUsbReporter::new(hid_writer);

    // Shared (&'static, not &'static mut) so both cores can hold a handle.
    let event_ch: &'static EventChannel<CriticalSectionRawMutex, 16> =
        EVENT_CHANNEL.init(EventChannel::new());
    let report_ch: &'static ReportChannel<CriticalSectionRawMutex, 8> =
        REPORT_CHANNEL.init(ReportChannel::new());
    let kc_ch: &'static KcChannel = KC_CHANNEL.init(Channel::new());
    let corr_ch: &'static CorrChannel = CORR_CHANNEL.init(Channel::new());

    // Core1: autocorrect engine. spawn_core1 hands the second core its own
    // executor; the engine and its trie live entirely over here.
    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|core1_spawner| {
                core1_spawner.spawn(autocorrect_core1(kc_ch, corr_ch).unwrap());
            });
        },
    );

    // Core0: real-time keyboard path.
    spawner.spawn(usb_device_task(usb).unwrap());
    spawner.spawn(matrix_task(scanner, debouncer, event_ch).unwrap());
    spawner.spawn(kb_task(event_ch, processor, report_ch, kc_ch, corr_ch).unwrap());
    tecla::task::usb_task(report_ch.receiver(), reporter).await;
}

#[embassy_executor::task]
async fn autocorrect_core1(kc_ch: &'static KcChannel, corr_ch: &'static CorrChannel) -> ! {
    let engine = AutocorrectEngine::new(dict::AUTOCORRECT_DICT);
    tecla::task::autocorrect_task(kc_ch.receiver(), corr_ch.sender(), engine).await
}

#[embassy_executor::task]
async fn usb_device_task(mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    device.run().await
}

#[embassy_executor::task]
async fn matrix_task(
    scanner: GpioMatrixScanner<'static, ROWS, COLS>,
    debouncer: AsymEagerDefer<ROWS, COLS>,
    event_ch: &'static EventChannel<CriticalSectionRawMutex, 16>,
) -> ! {
    tecla::task::matrix_task(scanner, debouncer, event_ch.sender()).await
}

#[embassy_executor::task]
async fn kb_task(
    event_ch: &'static EventChannel<CriticalSectionRawMutex, 16>,
    mut processor: KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>,
    report_ch: &'static ReportChannel<CriticalSectionRawMutex, 8>,
    kc_ch: &'static KcChannel,
    corr_ch: &'static CorrChannel,
) -> ! {
    tecla::task::keyboard_task_autocorrect(
        event_ch.receiver(),
        &mut processor,
        report_ch.sender(),
        kc_ch.sender(),
        corr_ch.receiver(),
    )
    .await
}
