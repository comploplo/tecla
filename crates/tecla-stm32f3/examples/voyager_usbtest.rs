//! Minimal USB bring-up test for the Voyager (STM32F303).
//!
//! Strips out I2C/MCP/matrix/processor entirely: just clock + USB HID, typing
//! `a` every 2 s. Diagnostic bisection — if this enumerates and types, the USB
//! stack, clock, and executor are all good and the bug is in the scanner path.
//!
//! ```sh
//! cargo build -p tecla-stm32f3 --example voyager_usbtest --release
//! arm-none-eabi-objcopy -O binary \
//!   target/thumbv7em-none-eabihf/release/examples/voyager_usbtest vt.bin
//! dfu-util -a 0 -d 3297:0791 -s 0x08002000:leave -D vt.bin
//! ```

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! and VTOR relocation")]
#![expect(clippy::unwrap_used, reason = "firmware entry point")]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Flex, Level, Output, Pull, Speed};
use embassy_stm32::peripherals::USB;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_stm32::{bind_interrupts, Config};
use embassy_time::Timer;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::Builder;
use panic_probe as _;
use static_cell::StaticCell;
use tecla_macros::keyboard_main;
use tecla_stm32f3::bootloader::idle_ignition_pins;
use tecla_stm32f3::KEYBOARD_REPORT_DESCRIPTOR;

bind_interrupts!(struct Irqs {
    USB_LP_CAN_RX0 => InterruptHandler<USB>;
});

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

/// ~5 µs at 72 MHz → ~100 kHz bit-bang clock.
fn bb_delay() {
    cortex_m::asm::delay(360);
}

/// Open-drain "release high" — the external pull-up drives the line up.
fn release(pin: &mut Flex<'_>) {
    pin.set_as_input(Pull::None);
}

/// Open-drain "drive low".
fn drive_low(pin: &mut Flex<'_>) {
    pin.set_low();
    pin.set_as_output(Speed::Low);
}

/// Pure bit-bang: returns true if a device at 7-bit `addr` ACKs a write byte.
/// Fixed iteration count — cannot hang regardless of bus state.
fn mcp_acks(scl: &mut Flex<'_>, sda: &mut Flex<'_>, addr: u8) -> bool {
    release(sda);
    release(scl);
    bb_delay();
    // START: SDA high→low while SCL high.
    drive_low(sda);
    bb_delay();
    drive_low(scl);
    bb_delay();
    // Address byte, write bit (0), MSB first.
    let byte = addr << 1;
    for i in (0..8).rev() {
        if (byte >> i) & 1 == 1 {
            release(sda);
        } else {
            drive_low(sda);
        }
        bb_delay();
        release(scl); // SCL high
        bb_delay();
        drive_low(scl);
        bb_delay();
    }
    // 9th clock: release SDA, pulse SCL, sample ACK (device pulls SDA low).
    release(sda);
    bb_delay();
    release(scl);
    bb_delay();
    let ack = sda.is_low();
    drive_low(scl);
    bb_delay();
    // STOP: SDA low→high while SCL high.
    drive_low(sda);
    bb_delay();
    release(scl);
    bb_delay();
    release(sda);
    bb_delay();
    ack
}

#[keyboard_main]
async fn main(spawner: Spawner) {
    unsafe {
        (*cortex_m::peripheral::SCB::PTR).vtor.write(0x0800_2000);
    }

    let mut p = embassy_stm32::init(stm32_config());

    // Match QMK's Voyager pre-init so the Ignition bootloader sense circuit and
    // status LED pins are in a known state during USB bring-up.
    let mut boot_sense = Flex::new(p.PA8);
    let mut boot_trigger = Flex::new(p.PA9);
    idle_ignition_pins(&mut boot_sense, &mut boot_trigger);
    let _status_led_1 = Output::new(p.PB5, Level::Low, Speed::Low);
    let _status_led_2 = Output::new(p.PB4, Level::Low, Speed::Low);
    let _status_led_3 = Output::new(p.PB3, Level::Low, Speed::Low);

    // DP disconnect pulse (QMK usb_lld_disconnect/connect).
    {
        let _dp = Output::new(p.PA12.reborrow(), Level::Low, Speed::Low);
        Timer::after_millis(100).await;
    }

    // Hang-proof bit-bang ACK probe of the MCP23018 at 0x20 on SCL=PB6/SDA=PB7.
    // embassy's blocking I2C has an unbounded wait loop (v2.rs `while pe()`), so
    // we drive the lines ourselves with fixed iterations — this cannot hang.
    let mcp_ack = {
        let mut scl = Flex::new(p.PB6.reborrow());
        let mut sda = Flex::new(p.PB7.reborrow());
        mcp_acks(&mut scl, &mut sda, 0x20)
    };

    let driver = Driver::new(p.USB, Irqs, p.PA12, p.PA11);

    let mut usb_config = embassy_usb::Config::new(0x3297, 0x1977);
    usb_config.manufacturer = Some("ZSA Technology Labs");
    usb_config.product = Some(if mcp_ack {
        "Voyager MCP-ACK"
    } else {
        "Voyager MCP-NAK"
    });
    // Unique serial per result so macOS can't serve a stale cached descriptor.
    usb_config.serial_number = Some(if mcp_ack { "VT-ACK-3" } else { "VT-NAK-3" });
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

    writer.ready().await;

    loop {
        Timer::after_secs(2).await;
        // 'a' = 0x04 — down then up.
        let _ = writer.write(&[0, 0, 0x04, 0, 0, 0, 0, 0]).await;
        Timer::after_millis(20).await;
        let _ = writer.write(&[0, 0, 0, 0, 0, 0, 0, 0]).await;
    }
}

#[embassy_executor::task]
async fn usb_device_task(mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    device.run().await
}
