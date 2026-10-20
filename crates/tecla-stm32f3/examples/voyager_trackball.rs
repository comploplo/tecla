//! ZSA Navigator trackball on the Voyager — moves the host cursor.
//!
//! Enumerates as a USB HID mouse and drives it from the Navigator hanging off
//! the TRRS bus. This is the bring-up test for
//! [`tecla_stm32f3::navigator::Navigator`]: if the pointer moves and tracks the
//! ball's direction, the SC18IS606 bridge and the PAW3805EK behind it are both
//! working.
//!
//! There is no keymap here and the module has no button, so the only key that
//! does anything is the bootmagic key (row 0, column 1 — `ESC` on the real
//! keymap), which enters the Ignition bootloader.
//!
//! # If the pointer does not move
//!
//! The status LEDs report bring-up, so a failure is visible without a probe:
//!
//! - **LED 1 on** — the sensor identified itself and the driver is polling.
//! - **LED 2 on** — the bridge answered on I2C but the sensor behind it did
//!   not return its product ID. That is a broken SPI link inside the module,
//!   not a bus problem.
//! - **LED 3 on** — the bridge did not answer on I2C at all: nothing plugged
//!   in, or the TRRS cables are not seated.
//!
//! ```sh
//! just flash --tecla-config voyager_trackball
//! ```

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! and VTOR relocation")]
#![expect(clippy::unwrap_used, reason = "firmware entry point")]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Flex, Input, Level, Output, Pull, Speed};
use embassy_stm32::peripherals::USB;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_stm32::{bind_interrupts, Config};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::Builder;
use panic_probe as _;
use static_cell::StaticCell;
use tecla::matrix::AsyncMatrixScanner;
use tecla::pipeline::report::{MouseReport, MOUSE_REPORT_DESCRIPTOR, MOUSE_REPORT_SIZE};
use tecla_macros::keyboard_main;
use tecla_stm32f3::bitbang_i2c::BitBangI2c;
use tecla_stm32f3::bootloader::{enter_ignition_bootloader, idle_ignition_pins};
use tecla_stm32f3::mcp23018::{Mcp23018, DEFAULT_ADDRESS};
use tecla_stm32f3::navigator::{Navigator, NavigatorError};
use tecla_stm32f3::scanner::VoyagerScanner;
use tecla_stm32f3::shared_i2c::SharedI2c;

bind_interrupts!(struct Irqs {
    USB_LP_CAN_RX0 => InterruptHandler<USB>;
});

type Bus = BitBangI2c<'static>;
type SharedBus = SharedI2c<'static, NoopRawMutex, Bus>;

const BOOTMAGIC_ROW: usize = 0;
const BOOTMAGIC_COL: usize = 1;
/// Motion poll interval — matches the driver's own cadence.
const POLL_MS: u64 = 7;
/// Polls between bootmagic checks (~1s), so DFU is always reachable.
const POLLS_PER_BOOTMAGIC: u16 = 140;
/// Wait before retrying a module that is absent or whose link dropped.
const RETRY_MS: u64 = 1_000;

static I2C_BUS: StaticCell<Mutex<NoopRawMutex, Bus>> = StaticCell::new();
static HID_STATE: StaticCell<State> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

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

/// The three status LEDs, driven as a one-of-three bring-up indicator.
struct StatusLeds<'d> {
    polling: Output<'d>,
    bad_sensor: Output<'d>,
    no_bridge: Output<'d>,
}

impl StatusLeds<'_> {
    fn show(&mut self, state: &Result<(), NavigatorError<impl core::fmt::Debug>>) {
        let (polling, bad_sensor, no_bridge) = match state {
            Ok(()) => (Level::High, Level::Low, Level::Low),
            Err(NavigatorError::SensorId(_)) => (Level::Low, Level::High, Level::Low),
            Err(NavigatorError::Bus(_)) => (Level::Low, Level::Low, Level::High),
        };
        self.polling.set_level(polling);
        self.bad_sensor.set_level(bad_sensor);
        self.no_bridge.set_level(no_bridge);
    }
}

#[keyboard_main]
async fn main(spawner: Spawner) {
    unsafe {
        (*cortex_m::peripheral::SCB::PTR).vtor.write(0x0800_2000);
    }

    let mut p = embassy_stm32::init(stm32_config());

    let mut boot_sense = Flex::new(p.PA8);
    let mut boot_trigger = Flex::new(p.PA9);
    idle_ignition_pins(&mut boot_sense, &mut boot_trigger);
    let mut leds = StatusLeds {
        polling: Output::new(p.PB5, Level::Low, Speed::Low),
        bad_sensor: Output::new(p.PB4, Level::Low, Speed::Low),
        no_bridge: Output::new(p.PB3, Level::Low, Speed::Low),
    };

    {
        let _dp = Output::new(p.PA12.reborrow(), Level::Low, Speed::Low);
        Timer::after_millis(100).await;
    }

    let i2c: Bus = BitBangI2c::new(Flex::new(p.PB6), Flex::new(p.PB7));
    let bus = &*I2C_BUS.init(Mutex::new(i2c));
    let mcp = Mcp23018::new(SharedI2c::new(bus), DEFAULT_ADDRESS);
    let mut ball = Navigator::new(bus);

    let driver = Driver::new(p.USB, Irqs, p.PA12, p.PA11);

    let mut usb_config = embassy_usb::Config::new(0x3297, 0x1977);
    usb_config.manufacturer = Some("ZSA Technology Labs");
    usb_config.product = Some("Voyager Trackball");
    usb_config.serial_number = Some("VT-BALL-1");
    usb_config.max_power = 100;
    usb_config.max_packet_size_0 = 64;

    let mut builder = Builder::new(
        driver,
        usb_config,
        USB_DEVICE_BUF.init([0; 256]),
        USB_CONFIG_BUF.init([0; 256]),
        USB_BOS_BUF.init([0; 256]),
        USB_CONTROL_BUF.init([0; 64]),
    );

    let hid_config = embassy_usb::class::hid::Config {
        report_descriptor: MOUSE_REPORT_DESCRIPTOR,
        request_handler: None,
        poll_ms: 1,
        max_packet_size: MOUSE_REPORT_SIZE as u16,
        hid_subclass: embassy_usb::class::hid::HidSubclass::Boot,
        hid_boot_protocol: embassy_usb::class::hid::HidBootProtocol::Mouse,
    };
    let mut writer: HidWriter<'_, _, MOUSE_REPORT_SIZE> =
        HidWriter::new(&mut builder, HID_STATE.init(State::new()), hid_config);

    let usb = builder.build();
    spawner.spawn(usb_device_task(usb).unwrap());

    let left_rows: [Output<'static>; 6] = [
        Output::new(p.PB10, Level::Low, Speed::Low),
        Output::new(p.PB11, Level::Low, Speed::Low),
        Output::new(p.PB12, Level::Low, Speed::Low),
        Output::new(p.PB13, Level::Low, Speed::Low),
        Output::new(p.PB14, Level::Low, Speed::Low),
        Output::new(p.PB15, Level::Low, Speed::Low),
    ];
    let left_cols: [Input<'static>; 7] = [
        Input::new(p.PA0, Pull::Down),
        Input::new(p.PA1, Pull::Down),
        Input::new(p.PA2, Pull::Down),
        Input::new(p.PA3, Pull::Down),
        Input::new(p.PA6, Pull::Down),
        Input::new(p.PA7, Pull::Down),
        Input::new(p.PB0, Pull::Down),
    ];
    let mut scanner = VoyagerScanner::new(left_rows, left_cols, mcp);

    writer.ready().await;

    let mut live = ball.init().await;
    leds.show(&live);

    let mut ticks = 0u16;
    loop {
        if live.is_ok() {
            match ball.poll().await {
                Ok(Some((dx, dy))) => {
                    // Full 16-bit: the report carries the sensor's own range,
                    // so a fast flick is no longer clipped to ±127.
                    let report = MouseReport {
                        buttons: 0,
                        dx,
                        dy,
                        scroll: 0,
                    };
                    let _ = writer.write(&report.to_bytes()).await;
                }
                Ok(None) => Timer::after_millis(POLL_MS).await,
                Err(e) => {
                    live = Err(NavigatorError::Bus(e));
                    leds.show(&live);
                }
            }
        } else {
            Timer::after_millis(RETRY_MS).await;
            live = ball.init().await;
            leds.show(&live);
        }

        ticks += 1;
        if ticks >= POLLS_PER_BOOTMAGIC {
            ticks = 0;
            if bootmagic_held(&mut scanner).await {
                enter_ignition_bootloader(&mut boot_sense, &mut boot_trigger).await;
            }
        }
    }
}

async fn bootmagic_held(scanner: &mut VoyagerScanner<'static, SharedBus>) -> bool {
    for _ in 0..5 {
        let state = match AsyncMatrixScanner::scan(scanner).await {
            Ok(state) => state,
            Err(never) => match never {},
        };
        if state.get(BOOTMAGIC_ROW, BOOTMAGIC_COL).unwrap_or(false) {
            return true;
        }
        Timer::after_millis(5).await;
    }
    false
}

#[embassy_executor::task]
async fn usb_device_task(mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    device.run().await
}
