//! Native NOS75 typing and RGB firmware. Lighting awaits hardware validation.
#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "interrupt bindings and early ROM DFU hook")]
#![allow(missing_docs, reason = "private editable keymap module")]

use core::sync::atomic::{AtomicBool, Ordering};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::select;
use embassy_stm32::gpio::{Flex, Input, Level, Output, Pull, Speed};
use embassy_stm32::i2c::{self, mode::Master, I2c};
use embassy_stm32::mode::Async;
use embassy_stm32::peripherals::USB;
use embassy_stm32::rcc::{mux::Usbsw, Hsi48Config, LsConfig, Sysclk};
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_stm32::{bind_interrupts, Config};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{with_timeout, Delay, Duration, Instant, Ticker, Timer};
use embassy_usb::class::hid::{HidBootProtocol, HidSubclass, HidWriter, State};
use embassy_usb::Builder;
use panic_probe as _;
use static_cell::{ConstStaticCell, StaticCell};
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::matrix::debounce::SymDefer;
use tecla::pipeline::event::KeyEvent;
use tecla::pipeline::report::{
    CONSUMER_REPORT_DESCRIPTOR, CONSUMER_REPORT_SIZE, KEYBOARD_REPORT_DESCRIPTOR,
    KEYBOARD_REPORT_SIZE,
};
use tecla::rgb::{LedDriver, RgbCommand};
use tecla::transport::usb::CompositeUsbReporter;
use tecla_stm32f0::{
    bootloader, is31fl3733::Nos75Leds, keyboard_control::KeyboardControl, matrix::Nos75Matrix,
};

#[path = "../profiles/nos75/keymap.rs"]
mod keymap;
#[path = "../profiles/nos75/lighting.rs"]
mod lighting;
#[path = "../profiles/nos75/usb_power.rs"]
mod usb_power;
use keymap::{COLS, KEYS, LAYERS, ROWS};

type Processor = KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>;
// Initialize in flash/.data rather than constructing 10 KiB on a small stack.
static PROCESSOR: ConstStaticCell<Processor> = ConstStaticCell::new(
    Processor::new(keymap::layers(), TapHoldConfig::new(200)).with_typing_stats(),
);

static CONFIG_DESCRIPTOR: StaticCell<[u8; 128]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 64]> = StaticCell::new();
static MSOS_DESCRIPTOR: StaticCell<[u8; 0]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
static KEYBOARD_STATE: StaticCell<State> = StaticCell::new();
static KEYBOARD_CONTROL: StaticCell<KeyboardControl> = StaticCell::new();
static CONSUMER_STATE: StaticCell<State> = StaticCell::new();
static USB_ACTIVE: AtomicBool = AtomicBool::new(false);
static USB_POWER_CHANGED: Signal<CriticalSectionRawMutex, bool> = Signal::new();
static USB_STATE: StaticCell<UsbState> = StaticCell::new();
static RGB_EVENTS: Channel<CriticalSectionRawMutex, KeyEvent, 4> = Channel::new();
static RGB_COMMANDS: Channel<CriticalSectionRawMutex, RgbCommand, 2> = Channel::new();

bind_interrupts!(struct Irqs {
    USB => InterruptHandler<USB>;
    I2C1 => i2c::EventInterruptHandler<embassy_stm32::peripherals::I2C1>,
            i2c::ErrorInterruptHandler<embassy_stm32::peripherals::I2C1>;
    DMA1_CHANNEL2_3 => embassy_stm32::dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH2>,
                      embassy_stm32::dma::InterruptHandler<embassy_stm32::peripherals::DMA1_CH3>;
});

// Cortex-M-RT calls this before .data/.bss initialization. Tail-call the
// constants/MMIO-only hook without depending on initialized RAM.
core::arch::global_asm!(
    ".section .text.__pre_init,\"ax\"",
    ".thumb",
    ".global __pre_init",
    ".type __pre_init,%function",
    ".thumb_func",
    "__pre_init:",
    "ldr r0, ={rom_hook}",
    "bx r0",
    ".ltorg",
    rom_hook = sym bootloader::maybe_enter_rom_dfu,
);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    config.rcc.ls = LsConfig::off(); // PC14/PC15 are rows, not LSE pins.
    config.rcc.hsi48 = Some(Hsi48Config {
        sync_from_usb: true,
    });
    config.rcc.mux.usbsw = Usbsw::HSI48;
    config.rcc.sys = Sysclk::HSI48;
    let p = embassy_stm32::init(config);

    // Tables recovered from the production image at 0800d3b4 and 0800d370.
    let rows = [
        Flex::new(p.PC14),
        Flex::new(p.PC15),
        Flex::new(p.PA0),
        Flex::new(p.PA1),
        Flex::new(p.PA2),
        Flex::new(p.PA3),
    ];
    let cols = [
        Input::new(p.PA4, Pull::Up),
        Input::new(p.PA5, Pull::Up),
        Input::new(p.PA6, Pull::Up),
        Input::new(p.PA7, Pull::Up),
        Input::new(p.PB0, Pull::Up),
        Input::new(p.PB1, Pull::Up),
        Input::new(p.PB10, Pull::Up),
        Input::new(p.PB11, Pull::Up),
        Input::new(p.PB12, Pull::Up),
        Input::new(p.PB13, Pull::Up),
        Input::new(p.PB14, Pull::Up),
        Input::new(p.PB15, Pull::Up),
        Input::new(p.PA8, Pull::Up),
        Input::new(p.PA9, Pull::Up),
        Input::new(p.PA10, Pull::Up),
        Input::new(p.PA15, Pull::Up),
        Input::new(p.PB3, Pull::Up),
    ];
    let mut scanner = Nos75Matrix::new(rows, cols);
    Timer::after_millis(20).await;
    // Physical Esc remains recovery even if its keymap action is edited.
    if scanner.scan_now().get(0, 0) == Some(true) {
        Timer::after_millis(200).await;
        if scanner.scan_now().get(0, 0) == Some(true) {
            bootloader::request_dfu();
        }
    }

    let driver = Driver::new(p.USB, Irqs, p.PA12, p.PA11);
    let (usb, mut keyboard_writer, mut consumer_writer) = setup_usb(driver);
    spawner.spawn(usb_task(usb).unwrap());
    // Start processing keys only when both host endpoints can receive them.
    keyboard_writer.ready().await;
    consumer_writer.ready().await;
    let reporter = CompositeUsbReporter::new(keyboard_writer).with_consumer(consumer_writer);
    let processor = PROCESSOR.take();
    // Stock production startup drives these two board outputs HIGH. Lighting
    // starts after USB configuration; recovery above does not depend on I2C.
    let gate_pins = (
        Output::new(p.PC13, Level::High, Speed::Low),
        Output::new(p.PB9, Level::High, Speed::Low),
    );
    let mut i2c_config = i2c::Config::default();
    i2c_config.frequency = Hertz::khz(400);
    i2c_config.timeout = Duration::from_millis(50);
    let bus = I2c::new(
        p.I2C1, p.PB6, p.PB7, p.DMA1_CH2, p.DMA1_CH3, Irqs, i2c_config,
    );
    spawner.spawn(rgb_task(Nos75Leds::new(bus), gate_pins).unwrap());
    tecla::task::run_with_rgb(
        scanner,
        SymDefer::new(5),
        processor,
        reporter,
        |event| {
            if event.pressed {
                let _ = RGB_EVENTS.try_send(event);
            }
        },
        |command| {
            if RGB_COMMANDS.try_send(command).is_err() {
                defmt::warn!("RGB control queue full");
            }
        },
    )
    .await;
}

type UsbDriver = Driver<'static, USB>;

#[derive(Default)]
struct UsbState {
    power: usb_power::UsbPower,
}

impl UsbState {
    fn update(&self) {
        let active = self.power.lighting_allowed();
        USB_ACTIVE.store(active, Ordering::Relaxed);
        USB_POWER_CHANGED.signal(active);
    }
}

impl embassy_usb::Handler for UsbState {
    fn reset(&mut self) {
        self.power.reset();
        self.update();
    }
    fn enabled(&mut self, enabled: bool) {
        self.power.enabled(enabled);
        self.update();
    }
    fn configured(&mut self, configured: bool) {
        self.power.configured(configured);
        self.update();
    }
    fn suspended(&mut self, suspended: bool) {
        self.power.suspended(suspended);
        self.update();
    }
}

// Keep descriptor-construction scratch storage out of the persistent async task.
#[inline(never)]
fn setup_usb(
    driver: UsbDriver,
) -> (
    embassy_usb::UsbDevice<'static, UsbDriver>,
    HidWriter<'static, UsbDriver, KEYBOARD_REPORT_SIZE>,
    HidWriter<'static, UsbDriver, CONSUMER_REPORT_SIZE>,
) {
    let mut usb_config = embassy_usb::Config::new(0x19f5, 0x3235);
    usb_config.manufacturer = Some("Tecla");
    usb_config.product = Some("Tecla NOS75");
    usb_config.serial_number = Some("TECLA-NOS75-01");
    usb_config.max_power = 500;
    let mut builder = Builder::new(
        driver,
        usb_config,
        CONFIG_DESCRIPTOR.init([0; 128]),
        BOS_DESCRIPTOR.init([0; 64]),
        MSOS_DESCRIPTOR.init([]),
        CONTROL_BUF.init([0; 64]),
    );
    let keyboard_writer = HidWriter::<_, KEYBOARD_REPORT_SIZE>::new(
        &mut builder,
        KEYBOARD_STATE.init(State::new()),
        embassy_usb::class::hid::Config {
            report_descriptor: KEYBOARD_REPORT_DESCRIPTOR,
            request_handler: Some(KEYBOARD_CONTROL.init(KeyboardControl::new())),
            poll_ms: 1,
            max_packet_size: KEYBOARD_REPORT_SIZE as u16,
            hid_subclass: HidSubclass::Boot,
            hid_boot_protocol: HidBootProtocol::Keyboard,
        },
    );
    let consumer_writer = HidWriter::<_, CONSUMER_REPORT_SIZE>::new(
        &mut builder,
        CONSUMER_STATE.init(State::new()),
        embassy_usb::class::hid::Config {
            report_descriptor: CONSUMER_REPORT_DESCRIPTOR,
            request_handler: None,
            poll_ms: 1,
            max_packet_size: CONSUMER_REPORT_SIZE as u16,
            hid_subclass: HidSubclass::No,
            hid_boot_protocol: HidBootProtocol::None,
        },
    );
    builder.handler(USB_STATE.init(UsbState::default()));
    let usb = builder.build();
    (usb, keyboard_writer, consumer_writer)
}

// Separate poll functions keep USB control-transfer scratch space off the
// keyboard task's call stack on this 16 KiB MCU.
#[embassy_executor::task]
async fn usb_task(mut device: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    device.run().await
}

// DMA-backed I2C keeps LED transfers off the keyboard/USB path. Bus failures
// stop the frame immediately and retry initialization without stopping typing.
#[embassy_executor::task]
async fn rgb_task(
    mut leds: Nos75Leds<I2c<'static, Async, Master>>,
    _gate_pins: (Output<'static>, Output<'static>),
) -> ! {
    let mut lighting = lighting::Lighting::new();
    let start = Instant::now();
    loop {
        if !USB_ACTIVE.load(Ordering::Relaxed) {
            while !USB_ACTIVE.load(Ordering::Relaxed) {
                if matches!(
                    with_timeout(Duration::from_millis(100), leds.shutdown()).await,
                    Ok(Ok(()))
                ) {
                    break;
                }
                // A transient bus failure must not leave LEDs on throughout sleep.
                let _ = select(Timer::after_millis(100), USB_POWER_CHANGED.wait()).await;
            }
            while !USB_ACTIVE.load(Ordering::Relaxed) {
                USB_POWER_CHANGED.wait().await;
            }
        }
        match with_timeout(Duration::from_millis(250), leds.init(&mut Delay)).await {
            Ok(Ok(())) => {}
            _ => {
                defmt::warn!("NOS75 LED initialization failed; retrying");
                Timer::after_millis(1000).await;
                continue;
            }
        }
        let mut frame_clock = Ticker::every(Duration::from_millis(33));
        loop {
            if !USB_ACTIVE.load(Ordering::Relaxed) {
                break;
            }
            let _ = select(frame_clock.next(), USB_POWER_CHANGED.wait()).await;
            if !USB_ACTIVE.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(command) = RGB_COMMANDS.try_receive() {
                lighting.command(command);
            }
            while let Ok(event) = RGB_EVENTS.try_receive() {
                lighting.on_key_event(event);
            }
            // Wrapping elapsed time is intentional for long-running animations.
            let elapsed = start.elapsed().as_millis() as u32;
            let frame = lighting.tick(elapsed);
            if !matches!(
                with_timeout(Duration::from_millis(100), leds.write(frame)).await,
                Ok(Ok(()))
            ) {
                defmt::warn!("NOS75 LED frame failed; reinitializing");
                Timer::after_millis(500).await;
                break;
            }
        }
    }
}
