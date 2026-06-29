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
//! byte ([`Timer`]) so a full scan never blocks USB for more than ~one byte. An
//! earlier all-busy-wait version starved `usb_task` and killed both halves —
//! this does not.
//!
//! Open-drain is emulated with [`Flex`]: "release high" switches the pin to
//! input so the board's external pull-ups raise the line; "drive low" switches
//! to output-low. ~400 kHz SCL (`HALF_PERIOD_CYCLES` at the 72 MHz core).

use embassy_stm32::gpio::{Flex, Pull, Speed};
use embassy_time::Timer;
use embedded_hal_async::i2c::{
    Error, ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation, SevenBitAddress,
};

/// Half a bit period: ~1.25 µs at 72 MHz → ~400 kHz SCL.
const HALF_PERIOD_CYCLES: u32 = 90;
/// Microseconds to yield to the executor after each byte (also inter-byte gap).
const YIELD_US: u64 = 3;

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
    pub fn new(scl: Flex<'d>, sda: Flex<'d>) -> Self {
        let mut this = Self { scl, sda };
        this.release_scl();
        this.release_sda();
        edge_delay();
        this
    }

    fn release_scl(&mut self) {
        self.scl.set_as_input(Pull::None);
    }
    fn drive_scl_low(&mut self) {
        self.scl.set_low();
        self.scl.set_as_output(Speed::Low);
    }
    fn release_sda(&mut self) {
        self.sda.set_as_input(Pull::None);
    }
    fn drive_sda_low(&mut self) {
        self.sda.set_low();
        self.sda.set_as_output(Speed::Low);
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
                        // Yield so USB/other tasks run between bytes.
                        Timer::after_micros(YIELD_US).await;
                    }
                }
                Operation::Read(buf) => {
                    let last = buf.len();
                    for (i, slot) in buf.iter_mut().enumerate() {
                        *slot = self.read_byte(i + 1 != last);
                        Timer::after_micros(YIELD_US).await;
                    }
                }
            }
        }
        self.stop();
        Ok(())
    }
}
