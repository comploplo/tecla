//! Raw matrix bring-up test for the Voyager (STM32F303 + MCP23018).
//!
//! This keeps the real Voyager scanner, but bypasses Tecla's debouncer,
//! keymap, and keyboard processor. It types `m ` as a heartbeat after USB is
//! ready, then types raw press coordinates like `l2c3 ` or `r11c6 ` whenever a
//! key transitions from released to pressed.
//!
//! ```sh
//! just flash --tecla-config voyager_matrix_test
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
use embassy_time::Timer;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::driver::Driver as UsbDriverTrait;
use embassy_usb::Builder;
use panic_probe as _;
use static_cell::StaticCell;
use tecla::matrix::{AsyncMatrixScanner, MatrixState};
use tecla_macros::keyboard_main;
use tecla_stm32f3::bitbang_i2c::BitBangI2c;
use tecla_stm32f3::bootloader::{enter_ignition_bootloader, idle_ignition_pins};
use tecla_stm32f3::mcp23018::{Mcp23018, DEFAULT_ADDRESS};
use tecla_stm32f3::scanner::{VoyagerScanner, COLS, ROWS};
use tecla_stm32f3::KEYBOARD_REPORT_DESCRIPTOR;

bind_interrupts!(struct Irqs {
    USB_LP_CAN_RX0 => InterruptHandler<USB>;
});

type Bus = BitBangI2c<'static>;
const BOOTMAGIC_ROW: usize = 0;
const BOOTMAGIC_COL: usize = 1;

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

fn ascii_usage(byte: u8) -> Option<u8> {
    match byte {
        b'a'..=b'z' => Some(0x04 + (byte - b'a')),
        b'1'..=b'9' => Some(0x1e + (byte - b'1')),
        b'0' => Some(0x27),
        b' ' => Some(0x2c),
        _ => None,
    }
}

async fn send_usage<'d, D: UsbDriverTrait<'d>>(writer: &mut HidWriter<'d, D, 8>, usage: u8) {
    let _ = writer.write(&[0, 0, usage, 0, 0, 0, 0, 0]).await;
    Timer::after_millis(20).await;
    let _ = writer.write(&[0, 0, 0, 0, 0, 0, 0, 0]).await;
    Timer::after_millis(10).await;
}

async fn send_ascii<'d, D: UsbDriverTrait<'d>>(writer: &mut HidWriter<'d, D, 8>, byte: u8) {
    if let Some(usage) = ascii_usage(byte) {
        send_usage(writer, usage).await;
    }
}

async fn send_text<'d, D: UsbDriverTrait<'d>>(writer: &mut HidWriter<'d, D, 8>, text: &[u8]) {
    for &byte in text {
        send_ascii(writer, byte).await;
    }
}

async fn send_decimal<'d, D: UsbDriverTrait<'d>>(writer: &mut HidWriter<'d, D, 8>, value: u8) {
    if value >= 10 {
        send_ascii(writer, b'0' + (value / 10)).await;
    }
    send_ascii(writer, b'0' + (value % 10)).await;
}

async fn send_matrix_token<'d, D: UsbDriverTrait<'d>>(
    writer: &mut HidWriter<'d, D, 8>,
    row: usize,
    col: usize,
) {
    let row = row as u8;
    let col = col as u8;
    send_ascii(writer, if row < 6 { b'l' } else { b'r' }).await;
    send_decimal(writer, row).await;
    send_ascii(writer, b'c').await;
    send_decimal(writer, col).await;
    send_ascii(writer, b' ').await;
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
    let _status_led_1 = Output::new(p.PB5, Level::Low, Speed::Low);
    let _status_led_2 = Output::new(p.PB4, Level::Low, Speed::Low);
    let _status_led_3 = Output::new(p.PB3, Level::Low, Speed::Low);

    {
        let _dp = Output::new(p.PA12.reborrow(), Level::Low, Speed::Low);
        Timer::after_millis(100).await;
    }

    let i2c: Bus = BitBangI2c::new(Flex::new(p.PB6), Flex::new(p.PB7));
    let mcp = Mcp23018::new(i2c, DEFAULT_ADDRESS);

    let driver = Driver::new(p.USB, Irqs, p.PA12, p.PA11);

    let mut usb_config = embassy_usb::Config::new(0x3297, 0x1977);
    usb_config.manufacturer = Some("ZSA Technology Labs");
    usb_config.product = Some("Voyager Matrix Test");
    usb_config.serial_number = Some("VT-MATRIX-1");
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
        report_descriptor: KEYBOARD_REPORT_DESCRIPTOR,
        request_handler: None,
        poll_ms: 1,
        max_packet_size: 8,
        hid_subclass: embassy_usb::class::hid::HidSubclass::Boot,
        hid_boot_protocol: embassy_usb::class::hid::HidBootProtocol::Keyboard,
    };
    let mut writer: HidWriter<'_, _, 8> =
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
    if bootmagic_held(&mut scanner).await {
        enter_ignition_bootloader(&mut boot_sense, &mut boot_trigger).await;
    }
    let mut prev = MatrixState::<ROWS, COLS>::new();
    let mut heartbeat_ticks = 0u16;

    writer.ready().await;
    send_text(&mut writer, b"m ").await;

    loop {
        let state = match scanner.scan().await {
            Ok(state) => state,
            Err(never) => match never {},
        };

        for (row, col, pressed) in state.iter() {
            if pressed && !prev.get(row, col).unwrap_or(false) {
                send_matrix_token(&mut writer, row, col).await;
            }
        }

        prev = state;
        heartbeat_ticks += 1;
        if heartbeat_ticks >= 400 {
            heartbeat_ticks = 0;
            send_text(&mut writer, b"m ").await;
        }

        Timer::after_millis(5).await;
    }
}

async fn bootmagic_held(scanner: &mut VoyagerScanner<'static, Bus>) -> bool {
    for _ in 0..5 {
        let state = match scanner.scan().await {
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
