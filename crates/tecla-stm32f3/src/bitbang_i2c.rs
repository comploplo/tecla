//! Hang-proof async bit-bang I2C master.
//!
//! embassy-stm32 0.6's blocking I2C v2 errors on the first transfer to this
//! board, then its recovery path spins on `while regs.cr1().read().pe() {}`
//! (v2.rs `soft_reset`, no timeout) forever — wedging the cooperative executor
//! and killing USB. Rather than fight a driver we can't patch, we drive SCL/SDA
//! ourselves with a fixed number of iterations per transfer: this can never
//! hang regardless of bus state.
//!
//! It is **async** and implements [`embedded_hal_async::i2c::I2c`], so
//! [`crate::mcp23018::Mcp23018`] and [`crate::scanner::VoyagerScanner`] use it
//! over the async HAL trait. The microsecond-scale bit edges are timed with a
//! short busy-wait (accurate), but the master yields to the executor after every
//! byte so a full scan never blocks USB for more than ~one byte. An earlier
//! all-busy-wait version starved `usb_task` and killed both halves — this does
//! not.
//!
//! Note what "does not block" means here: the busy-wait holds the CPU for the
//! whole of each byte. The yield only lets other tasks in between bytes.
//!
//! Both pins are set to real open-drain once in [`BitBangI2c::new`]: "release
//! high" lets the board's external pull-ups raise the line, "drive low" pulls
//! it down, and neither touches the pin's mode register.
//!
//! # Speed
//!
//! `HALF_PERIOD_CYCLES` is the tuning knob, but it is not a cycle count:
//! `cortex_m::asm::delay(n)` runs `1 + n/2` iterations of 3–4 cycles, so the
//! delay costs roughly `2n` cycles, not `n`. The resulting SCL is well under
//! the 400 kHz all three slave families support. Raising it is a hardware
//! calibration, not a code change — this bus has four devices plus a TRRS
//! cable's worth of capacitance, and 400 kHz demands a 300 ns rise time. Back
//! it off if ACKs start missing or reads come back garbled.

use embassy_futures::yield_now;
use embassy_stm32::gpio::{Flex, Speed};
use embedded_hal_async::i2c::{
    Error, ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation, SevenBitAddress,
};

/// Half a bit period, as the argument to `asm::delay` — see the module docs:
/// the real cost is about twice this in cycles, so ~2.5 µs at 72 MHz.
const HALF_PERIOD_CYCLES: u32 = 90;
/// Half-periods to wait for a stretched clock before giving up (~1.25 ms).
const STRETCH_TIMEOUT_POLLS: u32 = 1_000;

fn edge_delay() {
    cortex_m::asm::delay(HALF_PERIOD_CYCLES);
}

/// Error returned by the bit-bang master.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitBangError {
    /// A device did not acknowledge its address or a written byte.
    Nack,
}

impl Error for BitBangError {
    fn kind(&self) -> ErrorKind {
        match self {
            BitBangError::Nack => ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown),
        }
    }
}

/// Software I2C master over two open-drain GPIO lines.
pub struct BitBangI2c<'d> {
    scl: Flex<'d>,
    sda: Flex<'d>,
}

impl<'d> BitBangI2c<'d> {
    /// Builds a master on `scl`/`sda`, leaving both lines released (idle high).
    ///
    /// Both pins are put into real open-drain mode once, here. The alternative —
    /// flipping between input and output on every edge to emulate it — costs
    /// three interrupt-masked register writes per transition, roughly 27 of them
    /// per byte, for no benefit the hardware does not already provide.
    pub fn new(mut scl: Flex<'d>, mut sda: Flex<'d>) -> Self {
        scl.set_as_input_output(Speed::Low);
        sda.set_as_input_output(Speed::Low);
        let mut this = Self { scl, sda };
        this.release_scl();
        this.release_sda();
        edge_delay();
        this
    }

    /// Releases SCL and waits for it to actually rise.
    ///
    /// A slave is allowed to hold SCL low to buy time ("clock stretching"), and
    /// the master must not clock on until it lets go. The MCP23018 and the
    /// IS31FL3731s never stretch, so this went unnoticed — but an MCU-based
    /// slave such as the Navigator's SC18IS606 bridge does, and clocking
    /// through a stretch reads back zeros.
    ///
    /// Bounded, so a device holding SCL low forever cannot wedge the executor —
    /// that is the whole reason this bit-bang master exists.
    fn release_scl(&mut self) {
        self.scl.set_high();
        for _ in 0..STRETCH_TIMEOUT_POLLS {
            if self.scl.is_high() {
                return;
            }
            edge_delay();
        }
    }
    fn drive_scl_low(&mut self) {
        self.scl.set_low();
    }
    fn release_sda(&mut self) {
        self.sda.set_high();
    }
    fn drive_sda_low(&mut self) {
        self.sda.set_low();
    }

    fn start(&mut self) {
        self.release_sda();
        self.release_scl();
        edge_delay();
        self.drive_sda_low();
        edge_delay();
        self.drive_scl_low();
        edge_delay();
    }

    fn repeated_start(&mut self) {
        self.release_sda();
        edge_delay();
        self.release_scl();
        edge_delay();
        self.drive_sda_low();
        edge_delay();
        self.drive_scl_low();
        edge_delay();
    }

    fn stop(&mut self) {
        self.drive_sda_low();
        edge_delay();
        self.release_scl();
        edge_delay();
        self.release_sda();
        edge_delay();
    }

    /// Clocks out one byte MSB-first; returns `true` if the slave ACKed.
    fn write_byte(&mut self, byte: u8) -> bool {
        for i in (0..8).rev() {
            if (byte >> i) & 1 == 1 {
                self.release_sda();
            } else {
                self.drive_sda_low();
            }
            edge_delay();
            self.release_scl();
            edge_delay();
            self.drive_scl_low();
            edge_delay();
        }
        // 9th clock: release SDA and sample the ACK (slave pulls it low).
        self.release_sda();
        edge_delay();
        self.release_scl();
        edge_delay();
        let acked = self.sda.is_low();
        self.drive_scl_low();
        edge_delay();
        acked
    }

    /// Clocks in one byte MSB-first; sends ACK afterwards if `ack`, else NACK.
    fn read_byte(&mut self, ack: bool) -> u8 {
        self.release_sda();
        let mut byte = 0u8;
        for _ in 0..8 {
            self.release_scl();
            edge_delay();
            byte = (byte << 1) | u8::from(self.sda.is_high());
            self.drive_scl_low();
            edge_delay();
        }
        if ack {
            self.drive_sda_low();
        } else {
            self.release_sda();
        }
        edge_delay();
        self.release_scl();
        edge_delay();
        self.drive_scl_low();
        self.release_sda();
        edge_delay();
        byte
    }
}

impl ErrorType for BitBangI2c<'_> {
    type Error = BitBangError;
}

impl I2c<SevenBitAddress> for BitBangI2c<'_> {
    async fn transaction(
        &mut self,
        address: SevenBitAddress,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        // A (re)start + address byte is issued whenever the bus direction
        // changes; same-direction consecutive ops continue without a restart.
        let mut prev_read: Option<bool> = None;
        for op in operations.iter_mut() {
            let is_read = matches!(op, Operation::Read(_));
            if prev_read != Some(is_read) {
                if prev_read.is_none() {
                    self.start();
                } else {
                    self.repeated_start();
                }
                let addr_byte = (address << 1) | u8::from(is_read);
                if !self.write_byte(addr_byte) {
                    self.stop();
                    return Err(BitBangError::Nack);
                }
                prev_read = Some(is_read);
            }
            match op {
                Operation::Write(bytes) => {
                    for &b in bytes.iter() {
                        if !self.write_byte(b) {
                            self.stop();
                            return Err(BitBangError::Nack);
                        }
                        // Hand the executor a turn between bytes so USB and the other bus
                        // consumers run. A plain yield, not a timer: I2C has no minimum
                        // inter-byte gap, and a timer here meant a queue insert, an alarm
                        // reprogram, and forced idle tens of thousands of times a second.
                        yield_now().await;
                    }
                }
                Operation::Read(buf) => {
                    let last = buf.len();
                    for (i, slot) in buf.iter_mut().enumerate() {
                        *slot = self.read_byte(i + 1 != last);
                        yield_now().await;
                    }
                }
            }
        }
        self.stop();
        Ok(())
    }
}
