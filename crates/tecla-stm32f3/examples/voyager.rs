//! ZSA Voyager firmware (STM32F303 + MCP23018).
//!
//! Left half scans over STM32 GPIO; the right half scans over an MCP23018 on
//! I2C1. Mirrors QMK's `keyboards/zsa/voyager`. The keymap lives in the profile
//! at [`keyboards/tecla/voyager/keymaps/default/keymap.rs`], with its combos
//! alongside at [`keyboards/tecla/voyager/keymaps/default/combos.rs`].
//!
//! # Wiring (from QMK)
//!
//! | Signal            | Pins                                   |
//! |-------------------|----------------------------------------|
//! | Left rows (out)   | PB10 PB11 PB12 PB13 PB14 PB15 (active-high) |
//! | Left cols (in)    | PA0 PA1 PA2 PA3 PA6 PA7 PB0 (pull-down) |
//! | I2C1 → MCP23018   | SCL PB6, SDA PB7                        |
//! | USB FS            | DP PA12, DM PA11                        |
//!
//! # Flashing
//!
//! The Voyager runs a DFU bootloader; this app links at `0x08002000`. Enter the
//! bootloader (small button on the back), then:
//!
//! ```sh
//! cargo build -p tecla-stm32f3 --example voyager --release
//! arm-none-eabi-objcopy -O binary \
//!   target/thumbv7em-none-eabihf/release/examples/voyager voyager.bin
//! dfu-util -a 0 -d 3297:0791 -s 0x08002000:leave -D voyager.bin
//! ```

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! and VTOR relocation")]
#![expect(
    clippy::unwrap_used,
    reason = "embassy task spawn is infallible in firmware entry points"
)]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Flex, Input, Level, Output, Pull, Speed};
use embassy_stm32::peripherals::USB;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_stm32::{bind_interrupts, Config};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::Timer;
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::Builder;
use panic_probe as _;
use static_cell::StaticCell;
use tecla::devices::keyboard::combo::ComboConfig;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::matrix::debounce::AsymEagerDefer;
use tecla::matrix::AsyncMatrixScanner;
use tecla::task::{EventChannel, ReportChannel};
use tecla_macros::{keyboard_main, keymap};
use tecla_stm32f3::bitbang_i2c::BitBangI2c;
use tecla_stm32f3::bootloader::{enter_ignition_bootloader, idle_ignition_pins};
use tecla_stm32f3::mcp23018::{Mcp23018, DEFAULT_ADDRESS};
use tecla_stm32f3::scanner::{VoyagerScanner, COLS, ROWS};
use tecla_stm32f3::{UsbHidReporter, KEYBOARD_REPORT_DESCRIPTOR};

bind_interrupts!(struct Irqs {
    USB_LP_CAN_RX0 => InterruptHandler<USB>;
});

const LAYERS: usize = 3;
const KEYS: usize = ROWS * COLS;
const BOOTMAGIC_ROW: usize = 0;
const BOOTMAGIC_COL: usize = 1;

// Compile-time matrix-fit guard. `keymap!` and `KeyboardProcessor`'s const
// generics already make a layer that isn't exactly `ROWS*COLS` keys a build
// error, so a clean build proves the keymap fits. This restates the invariant
// with a readable message in case someone edits ROWS/COLS/KEYS inconsistently.
const _: () = assert!(
    KEYS == ROWS * COLS,
    "voyager keymap KEYS must equal ROWS*COLS"
);

// Compile-time stack-budget guard. `PROCESSOR.init_with` builds this struct as
// a stack temporary before moving it into the StaticCell (see the comment on
// `PROCESSOR` below) — on a chip with only 40K total SRAM, an unnoticed size
// regression here (e.g. a bigger macro queue or another layer) is exactly the
// kind of change that silently overflows the stack and hard-faults main()
// after usb_device_task is already spawned, stranding USB enumeration. This
// caught a real instance of that bug: fix the size or the budget, don't
// delete this.
const _: () = assert!(
    core::mem::size_of::<KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>>() <= 16 * 1024,
    "KeyboardProcessor grew past its 16K stack budget on a 40K-RAM chip"
);

type Bus = BitBangI2c<'static>;

static EVENT_CHANNEL: StaticCell<EventChannel<NoopRawMutex, 16>> = StaticCell::new();
static REPORT_CHANNEL: StaticCell<ReportChannel<NoopRawMutex, 8>> = StaticCell::new();
// The processor is ~16 KB; park it in a StaticCell so it lives in .bss exactly
// once. Passing it by value through the task future would make rustc reserve
// two copies (param + generator local) on this chip's 40 KB SRAM — init_with
// below constructs it in place instead.
static PROCESSOR: StaticCell<KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>> = StaticCell::new();

static HID_STATE: StaticCell<State> = StaticCell::new();
static USB_DEVICE_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
// Combos as a separate const (not bundled into the keymap tuple): the
// processor is already a ~10K stack temporary at boot, and an extra
// LayerStack copy from a tuple return would add unnecessary stack pressure.
static VOYAGER_COMBO_CONFIG: ComboConfig = include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../keyboards/tecla/voyager/keymaps/default/combos.rs"
));

/// 72 MHz from an 8 MHz HSE crystal; the USB clock is derived automatically as
/// PLL/1.5 = 48 MHz.
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

#[keyboard_main]
async fn main(spawner: Spawner) {
    // The DFU bootloader hands off with the app at 0x08002000; point the vector
    // table there before any interrupt fires.
    unsafe {
        (*cortex_m::peripheral::SCB::PTR).vtor.write(0x0800_2000);
    }

    let mut p = embassy_stm32::init(stm32_config());

    // Match QMK's Voyager pre-init: A8 is the Ignition bootloader sense line,
    // and A9 is held low so the bootloader trigger capacitor discharges after
    // reset. B5/B4/B3 are status LEDs and should be driven off early.
    let mut boot_sense = Flex::new(p.PA8);
    let mut boot_trigger = Flex::new(p.PA9);
    idle_ignition_pins(&mut boot_sense, &mut boot_trigger);
    let _status_led_1 = Output::new(p.PB5, Level::Low, Speed::Low);
    let _status_led_2 = Output::new(p.PB4, Level::Low, Speed::Low);
    let _status_led_3 = Output::new(p.PB3, Level::Low, Speed::Low);

    // F303 USB (usb_v1) has no internal DP pull-up control, so embassy can't
    // signal connect/disconnect for us. Replicate QMK's dance: drive DP (PA12)
    // low to force the host to drop the stale DFU enumeration, then hand the pin
    // to the USB AF (connect) so it re-enumerates with our descriptors.
    {
        let _dp = Output::new(p.PA12.reborrow(), Level::Low, Speed::Low);
        Timer::after_millis(100).await;
    }

    // Right half: async bit-bang I2C to the MCP23018 (SCL=PB6, SDA=PB7).
    // embassy's blocking I2C errors then spins forever in its reset path on this
    // board; the bit-bang yields between bytes so it can't starve USB — see
    // [`BitBangI2c`].
    let i2c: Bus = BitBangI2c::new(Flex::new(p.PB6), Flex::new(p.PB7));
    let mcp = Mcp23018::new(i2c, DEFAULT_ADDRESS);

    let driver = Driver::new(p.USB, Irqs, p.PA12, p.PA11);

    let mut usb_config = embassy_usb::Config::new(0x3297, 0x1977);
    usb_config.manufacturer = Some("ZSA Technology Labs");
    usb_config.product = Some("Voyager");
    usb_config.serial_number = Some("TECLA-BB2");
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
    let hid_writer: HidWriter<'_, _, 8> =
        HidWriter::new(&mut builder, HID_STATE.init(State::new()), hid_config);

    let usb = builder.build();
    // Spawn USB before any matrix/I2C setup so enumeration never waits on (or
    // wedges behind) the right-half expander.
    spawner.spawn(usb_device_task(usb).unwrap());

    // Left half: row strobes idle low (active-high), columns pulled down.
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

    // Right half: MCP23018 over I2C1 (SCL PB6, SDA PB7); `mcp` is the probed
    // handle from above.
    let mut scanner = VoyagerScanner::new(left_rows, left_cols, mcp);
    if bootmagic_held(&mut scanner).await {
        enter_ignition_bootloader(&mut boot_sense, &mut boot_trigger).await;
    }
    // 12 ms deferred release: long enough that a transient scan misread (GPIO
    // settle jitter or an I2C hiccup) can't register a false release on a held
    // key, which would otherwise reset the host's auto-repeat. Tune if releases
    // feel laggy.
    let debouncer = AsymEagerDefer::new(12);

    let keymap = include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../keyboards/tecla/voyager/keymaps/default/keymap.rs"
    ));
    let processor = PROCESSOR.init_with(|| {
        KeyboardProcessor::new(keymap, TapHoldConfig::default()).with_combos(VOYAGER_COMBO_CONFIG)
    });
    let reporter = UsbHidReporter::new(hid_writer);

    let event_ch = EVENT_CHANNEL.init(EventChannel::new());
    let report_ch = REPORT_CHANNEL.init(ReportChannel::new());

    spawner.spawn(matrix_task(scanner, debouncer, event_ch).unwrap());
    spawner.spawn(keyboard_task(event_ch, processor, report_ch).unwrap());
    tecla::task::usb_task(report_ch.receiver(), reporter).await;
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

#[embassy_executor::task]
async fn matrix_task(
    scanner: VoyagerScanner<'static, Bus>,
    debouncer: AsymEagerDefer<ROWS, COLS>,
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
) -> ! {
    tecla::task::matrix_task(scanner, debouncer, event_ch.sender()).await
}

#[embassy_executor::task]
async fn keyboard_task(
    event_ch: &'static EventChannel<NoopRawMutex, 16>,
    processor: &'static mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>,
    report_ch: &'static ReportChannel<NoopRawMutex, 8>,
) -> ! {
    tecla::task::keyboard_task(event_ch.receiver(), processor, report_ch.sender()).await
}
