//! I2C bus probe for the Voyager — finds what is actually on the TRRS bus.
//!
//! The Voyager is not a QMK `split` board: the right half's matrix is read over
//! I2C through an MCP23018, so the TRRS cable carries I2C1 (SCL PB6, SDA PB7).
//! A ZSA Navigator daisy-chains onto that same bus, which is why it ships with
//! two TRRS cables and why ZSA's own setup guide says the module "will not
//! respond" until you reflash the keyboard — the STM32 has to drive it.
//!
//! Tecla has no pointing-device driver yet ([`AsyncPointingScanner`] has no
//! implementors), so before writing one this answers the two questions a driver
//! needs: what address does the Navigator answer on, and what does it report.
//!
//! [`AsyncPointingScanner`]: tecla::devices::mouse::pointing::AsyncPointingScanner
//!
//! # Reading the output
//!
//! There is no debug probe on a DFU-flashed Voyager, so results are **typed
//! over USB HID** — open a text editor and watch. The bus is re-probed every
//! two seconds and a line is typed only when the set of devices *changes*, so
//! you can plug the Navigator in live and watch it appear.
//!
//! Each device is one token:
//!
//! ```text
//! scan a20k a74k a77k end3        <- bare keyboard: expander + two LED chips
//! scan a20k a74k a77k a5cx31 end4 <- something new answered at 0x5c, reg0 = 0x31
//! ```
//!
//! - `a<hex>` — 7-bit address that ACKed
//! - `k` — known Voyager hardware (MCP23018, IS31FL3731 ×2)
//! - `x<hex>` — *not* known hardware; the hex is register 0x00, which on most
//!   PixArt sensors is the product ID. `xnak` means it ACKed its address but
//!   refused the register read.
//! - `end<n>` — device count, so a truncated line is obvious
//!
//! # Watch mode
//!
//! As soon as an unknown device is on the bus, this stops idling between scans
//! and polls that device with a plain read (no register pointer), typing the
//! bytes whenever they change:
//!
//! ```text
//! m 03fe000000 00   <- six raw bytes; roll the ball and watch which move
//! mnak              <- it ACKed its address but refused a plain read
//! ```
//!
//! Roll the ball slowly in one axis at a time, then click: whichever byte
//! tracks each motion is that axis, and a bit that flips only on click is the
//! button mask. That is the layout a driver needs.
//!
//! Bootmagic (top-left key held at any time) still enters the Ignition
//! bootloader, so this cannot strand the board.
//!
//! ```sh
//! just flash --tecla-config voyager_i2c_scan
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
use embassy_usb::driver::Driver as UsbDriverTrait;
use embassy_usb::Builder;
use embedded_hal_async::i2c::{I2c, Operation};
use panic_probe as _;
use static_cell::StaticCell;
use tecla::matrix::AsyncMatrixScanner;
use tecla_macros::keyboard_main;
use tecla_stm32f3::bitbang_i2c::BitBangI2c;
use tecla_stm32f3::bootloader::{enter_ignition_bootloader, idle_ignition_pins};
use tecla_stm32f3::is31fl3731::{ADDR_LEFT, ADDR_RIGHT};
use tecla_stm32f3::mcp23018::{Mcp23018, DEFAULT_ADDRESS};
use tecla_stm32f3::scanner::VoyagerScanner;
use tecla_stm32f3::shared_i2c::SharedI2c;
use tecla_stm32f3::KEYBOARD_REPORT_DESCRIPTOR;

bind_interrupts!(struct Irqs {
    USB_LP_CAN_RX0 => InterruptHandler<USB>;
});

type Bus = BitBangI2c<'static>;
type SharedBus = SharedI2c<'static, NoopRawMutex, Bus>;

/// Hardware the Voyager is already known to have — everything else is news.
const KNOWN: [u8; 3] = [DEFAULT_ADDRESS, ADDR_LEFT, ADDR_RIGHT];

/// Reserved I2C addresses sit outside `0x08..=0x77`; probing them is meaningless.
const FIRST_ADDR: u8 = 0x08;
const LAST_ADDR: u8 = 0x77;

const BOOTMAGIC_ROW: usize = 0;
const BOOTMAGIC_COL: usize = 1;
const RESCAN_MS: u64 = 2_000;

/// Bytes read per poll in watch mode. Six is enough for dx/dy/buttons in any
/// plausible layout without making a typed line unreadable.
const WATCH_LEN: usize = 6;
/// Poll interval in watch mode — fast enough to catch a flick of the ball.
const WATCH_MS: u64 = 50;
/// Watch polls between bootmagic checks (~2s), so DFU is always reachable.
const WATCH_POLLS_PER_BOOTMAGIC: u16 = 40;

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

fn ascii_usage(byte: u8) -> Option<u8> {
    match byte {
        b'a'..=b'z' => Some(0x04 + (byte - b'a')),
        b'1'..=b'9' => Some(0x1e + (byte - b'1')),
        b'0' => Some(0x27),
        b' ' => Some(0x2c),
        _ => None,
    }
}

async fn send_ascii<'d, D: UsbDriverTrait<'d>>(writer: &mut HidWriter<'d, D, 8>, byte: u8) {
    let Some(usage) = ascii_usage(byte) else {
        return;
    };
    let _ = writer.write(&[0, 0, usage, 0, 0, 0, 0, 0]).await;
    Timer::after_millis(20).await;
    let _ = writer.write(&[0, 0, 0, 0, 0, 0, 0, 0]).await;
    Timer::after_millis(10).await;
}

async fn send_text<'d, D: UsbDriverTrait<'d>>(writer: &mut HidWriter<'d, D, 8>, text: &[u8]) {
    for &byte in text {
        send_ascii(writer, byte).await;
    }
}

/// Types `value` as two lowercase hex digits (both are typeable as ASCII).
async fn send_hex<'d, D: UsbDriverTrait<'d>>(writer: &mut HidWriter<'d, D, 8>, value: u8) {
    for nibble in [value >> 4, value & 0x0F] {
        let digit = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + (nibble - 10)
        };
        send_ascii(writer, digit).await;
    }
}

async fn send_decimal<'d, D: UsbDriverTrait<'d>>(writer: &mut HidWriter<'d, D, 8>, value: u8) {
    if value >= 10 {
        send_ascii(writer, b'0' + (value / 10)).await;
    }
    send_ascii(writer, b'0' + (value % 10)).await;
}

/// Probes one address with an address-only write — no data byte follows, so
/// this cannot disturb a device whose register map we do not know yet.
async fn probes(bus: &mut SharedBus, addr: u8) -> bool {
    bus.transaction(addr, &mut [Operation::Write(&[])])
        .await
        .is_ok()
}

/// Reads register `0x00` from an *unknown* device — on PixArt sensors (the
/// Navigator trackball uses a PAW3805EK) that is the product ID. Known Voyager
/// chips are skipped so this never pokes at hardware already in use.
async fn read_reg0(bus: &mut SharedBus, addr: u8) -> Option<u8> {
    let mut buf = [0u8; 1];
    bus.write_read(addr, &[0x00], &mut buf).await.ok()?;
    Some(buf[0])
}

/// Plain read of `WATCH_LEN` bytes — no register pointer written first.
///
/// The scan found the Navigator answering but reporting `0x00` at register
/// 0x00, which is not a plausible product ID. The likely explanation is that
/// the Core presents a byte stream rather than a register map, so this reads it
/// raw and lets motion tell us the layout.
async fn watch_read(bus: &mut SharedBus, addr: u8) -> Option<[u8; WATCH_LEN]> {
    let mut buf = [0u8; WATCH_LEN];
    bus.transaction(addr, &mut [Operation::Read(&mut buf)])
        .await
        .ok()?;
    Some(buf)
}

/// Types the raw bytes from `addr` whenever they change, so rolling the ball
/// reveals which byte is dx, which is dy, and which carry the buttons.
async fn watch<'d, D: UsbDriverTrait<'d>>(
    writer: &mut HidWriter<'d, D, 8>,
    bus: &mut SharedBus,
    addr: u8,
    last: &mut Option<[u8; WATCH_LEN]>,
) {
    let Some(buf) = watch_read(bus, addr).await else {
        // Report a refused read once, not every poll.
        if *last != Some([0xFF; WATCH_LEN]) {
            *last = Some([0xFF; WATCH_LEN]);
            send_text(writer, b"mnak ").await;
        }
        return;
    };
    if *last == Some(buf) {
        return;
    }
    *last = Some(buf);
    send_ascii(writer, b'm').await;
    for byte in buf {
        send_hex(writer, byte).await;
    }
    send_ascii(writer, b' ').await;
}

/// The lowest present address that is not known Voyager hardware.
fn first_unknown(present: u128) -> Option<u8> {
    (FIRST_ADDR..=LAST_ADDR).find(|addr| present & (1u128 << addr) != 0 && !KNOWN.contains(addr))
}

/// Returns a bitmap of every address that ACKed, one bit per 7-bit address.
async fn probe_bus(bus: &mut SharedBus) -> u128 {
    let mut present = 0u128;
    for addr in FIRST_ADDR..=LAST_ADDR {
        if probes(bus, addr).await {
            present |= 1u128 << addr;
        }
        // Yield between addresses so USB keeps servicing during a full sweep.
        Timer::after_micros(200).await;
    }
    present
}

async fn report<'d, D: UsbDriverTrait<'d>>(
    writer: &mut HidWriter<'d, D, 8>,
    bus: &mut SharedBus,
    present: u128,
) {
    send_text(writer, b"scan ").await;
    let mut count = 0u8;
    for addr in FIRST_ADDR..=LAST_ADDR {
        if present & (1u128 << addr) == 0 {
            continue;
        }
        count += 1;
        send_ascii(writer, b'a').await;
        send_hex(writer, addr).await;
        if KNOWN.contains(&addr) {
            send_ascii(writer, b'k').await;
        } else {
            send_ascii(writer, b'x').await;
            match read_reg0(bus, addr).await {
                Some(reg0) => send_hex(writer, reg0).await,
                None => send_text(writer, b"nak").await,
            }
        }
        send_ascii(writer, b' ').await;
    }
    send_text(writer, b"end").await;
    send_decimal(writer, count).await;
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

    // Shared so the matrix scanner (bootmagic) and the prober can both reach
    // the one physical bus — see `shared_i2c`.
    let i2c: Bus = BitBangI2c::new(Flex::new(p.PB6), Flex::new(p.PB7));
    let bus = &*I2C_BUS.init(Mutex::new(i2c));
    let mcp = Mcp23018::new(SharedI2c::new(bus), DEFAULT_ADDRESS);
    let mut prober: SharedBus = SharedI2c::new(bus);

    let driver = Driver::new(p.USB, Irqs, p.PA12, p.PA11);

    let mut usb_config = embassy_usb::Config::new(0x3297, 0x1977);
    usb_config.manufacturer = Some("ZSA Technology Labs");
    usb_config.product = Some("Voyager I2C Scan");
    usb_config.serial_number = Some("VT-I2CSCAN-1");
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

    writer.ready().await;

    // `1` is impossible for a real bitmap (address 0 is reserved), so the first
    // sweep always differs and always prints — including the all-empty case.
    let mut last_present = 1u128;
    let mut last_watch: Option<[u8; WATCH_LEN]> = None;
    loop {
        let present = probe_bus(&mut prober).await;
        if present != last_present {
            last_present = present;
            report(&mut writer, &mut prober, present).await;
            last_watch = None;
        }

        // Once something unknown is on the bus the scan has done its job;
        // what we need next is motion, so poll it instead of idling.
        if let Some(addr) = first_unknown(present) {
            for _ in 0..WATCH_POLLS_PER_BOOTMAGIC {
                watch(&mut writer, &mut prober, addr, &mut last_watch).await;
                Timer::after_millis(WATCH_MS).await;
            }
        } else {
            Timer::after_millis(RESCAN_MS).await;
        }

        // Checked every pass, not just at boot: whatever this example does to
        // the bus, the bootmagic key always gets back to DFU.
        if bootmagic_held(&mut scanner).await {
            enter_ignition_bootloader(&mut boot_sense, &mut boot_trigger).await;
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
