//! Planck 4x12 ortholinear keyboard over BLE.
//!
//! Same keymap as the USB `planck` example, but sends HID reports over
//! Bluetooth Low Energy using the `trouble-host` stack with `nrf-sdc`.
//!
//! # Prerequisites
//!
//! The `ble` feature must be enabled. Current dependency versions:
//! `trouble-host` 0.6, `nrf-sdc` 0.3, `embassy-nrf` 0.10, `embassy-sync` 0.7.
//!
//! The `nrf-sdc` build requires ARM sysroot headers for bindgen.
//! Set `LIBCLANG_PATH` to your system's libclang path (see `.cargo/config.toml`).
//!
//! # Flashing
//!
//! ```sh
//! cargo run --example planck_ble --features ble --release
//! ```
//!
//! # Wiring
//!
//! Same as the USB example:
//! - P0.00–P0.03 : row outputs (active-low)
//! - P0.04–P0.08, P0.11–P0.17 : column inputs (internal pull-up)
//!
//! # Keymap
//!
//! Identical to `planck.rs` — see that example for the full layout diagram.

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! and nrf-sdc require unsafe")]
#![expect(
    clippy::unwrap_used,
    reason = "embassy task spawn is infallible in firmware entry points"
)]
#![expect(
    clippy::as_conversions,
    reason = "nrf-sdc configuration constants require as casts"
)]

use defmt::unwrap;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::bind_interrupts;
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::mode::Async;
use embassy_nrf::peripherals::RNG;
use embassy_nrf::rng;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use nrf_sdc::mpsl::MultiprotocolServiceLayer;
use nrf_sdc::{self as sdc, mpsl};
use panic_probe as _;
use static_cell::StaticCell;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::matrix::debounce::AsymEagerDefer;
use tecla::task::{EventChannel, ReportChannel};
use tecla_macros::{keyboard_main, keymap};
use tecla_nrf52840::GpioMatrixScanner;
use trouble_host::prelude::*;

bind_interrupts!(struct Irqs {
    RNG => rng::InterruptHandler<RNG>;
    EGU0_SWI0 => nrf_sdc::mpsl::LowPrioInterruptHandler;
    CLOCK_POWER => nrf_sdc::mpsl::ClockInterruptHandler;
    RADIO => nrf_sdc::mpsl::HighPrioInterruptHandler;
    TIMER0 => nrf_sdc::mpsl::HighPrioInterruptHandler;
    RTC0 => nrf_sdc::mpsl::HighPrioInterruptHandler;
});

const ROWS: usize = 4;
const COLS: usize = 12;

const L2CAP_TXQ: u8 = 3;
const L2CAP_RXQ: u8 = 3;

static EVENT_CHANNEL: StaticCell<EventChannel<NoopRawMutex, 16>> = StaticCell::new();
static REPORT_CHANNEL: StaticCell<ReportChannel<NoopRawMutex, 8>> = StaticCell::new();

fn build_sdc<'d, const N: usize>(
    p: nrf_sdc::Peripherals<'d>,
    rng: &'d mut rng::Rng<Async>,
    mpsl: &'d MultiprotocolServiceLayer,
    mem: &'d mut sdc::Mem<N>,
) -> Result<nrf_sdc::SoftdeviceController<'d>, nrf_sdc::Error> {
    sdc::Builder::new()?
        .support_adv()
        .support_peripheral()
        .peripheral_count(1)?
        .buffer_cfg(
            DefaultPacketPool::MTU as u16,
            DefaultPacketPool::MTU as u16,
            L2CAP_TXQ,
            L2CAP_RXQ,
        )?
        .build(p, rng, mpsl, mem)
}

#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}

#[keyboard_main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(embassy_nrf::config::Config::default());

    let mpsl_p =
        mpsl::Peripherals::new(p.RTC0, p.TIMER0, p.TEMP, p.PPI_CH19, p.PPI_CH30, p.PPI_CH31);
    let lfclk_cfg = mpsl::raw::mpsl_clock_lfclk_cfg_t {
        source: mpsl::raw::MPSL_CLOCK_LF_SRC_RC as u8,
        rc_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_CTIV as u8,
        rc_temp_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_TEMP_CTIV as u8,
        accuracy_ppm: mpsl::raw::MPSL_DEFAULT_CLOCK_ACCURACY_PPM as u16,
        skip_wait_lfclk_started: mpsl::raw::MPSL_DEFAULT_SKIP_WAIT_LFCLK_STARTED != 0,
    };
    static MPSL: StaticCell<MultiprotocolServiceLayer> = StaticCell::new();
    let mpsl = MPSL.init(unwrap!(mpsl::MultiprotocolServiceLayer::new(
        mpsl_p, Irqs, lfclk_cfg
    )));
    spawner.spawn(mpsl_task(mpsl).unwrap());

    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24,
        p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );

    let mut rng = rng::Rng::new(p.RNG, Irqs);
    let mut sdc_mem = sdc::Mem::<4720>::new();
    let sdc = unwrap!(build_sdc(sdc_p, &mut rng, mpsl, &mut sdc_mem));

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

    let event_ch = EVENT_CHANNEL.init(EventChannel::new());
    let report_ch = REPORT_CHANNEL.init(ReportChannel::new());

    spawner.spawn(matrix_task(scanner, debouncer, event_ch).unwrap());
    spawner.spawn(keyboard_task(event_ch, processor, report_ch).unwrap());

    tecla_nrf52840::run_ble_peripheral(sdc, "Tecla Planck").await;
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
