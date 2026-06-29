//! Minimal MCP23018 I2C GPIO-expander driver.
//!
//! The ZSA Voyager reads its right-hand matrix through an MCP23018 on I2C1.
//! This driver covers exactly what the matrix scan needs: set port direction,
//! configure pull-ups, drive the output latches, and read a port. It is generic over any async
//! [`embedded_hal_async::i2c::I2c`] implementation — on the Voyager that is
//! [`crate::bitbang_i2c::BitBangI2c`], because embassy-stm32's blocking I2C
//! hangs on this board (see that module).
//!
//! Register layout assumes the power-on default `IOCON.BANK = 0`.

use embedded_hal_async::i2c::I2c;

/// MCP23018 7-bit I2C address with A0–A2 tied low (the Voyager's wiring).
pub const DEFAULT_ADDRESS: u8 = 0x20;

/// One of the expander's two 8-bit ports.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Port {
    /// Port A — drives the right-hand row-select lines.
    A,
    /// Port B — low 6 bits read the right-hand columns; top 2 drive status LEDs.
    B,
}

// Register addresses (BANK = 0).
const IODIRA: u8 = 0x00;
const GPPUA: u8 = 0x0C;
const GPIOA: u8 = 0x12;
const OLATA: u8 = 0x14;

impl Port {
    const fn offset(self) -> u8 {
        match self {
            Port::A => 0,
            Port::B => 1,
        }
    }
}

/// A blocking MCP23018 driver bound to one I2C bus and device address.
pub struct Mcp23018<I2C> {
    i2c: I2C,
    addr: u8,
}

impl<I2C: I2c> Mcp23018<I2C> {
    /// Creates a driver for the device at `addr` (see [`DEFAULT_ADDRESS`]).
    pub fn new(i2c: I2C, addr: u8) -> Self {
        Self { i2c, addr }
    }

    /// Sets a port's direction mask: a `1` bit is an input, `0` is an output
    /// (matching the MCP23018 `IODIR` register semantics).
    ///
    /// # Errors
    /// Returns the underlying I2C error if the transfer fails.
    pub async fn set_direction(&mut self, port: Port, mask: u8) -> Result<(), I2C::Error> {
        self.i2c
            .write(self.addr, &[IODIRA + port.offset(), mask])
            .await
    }

    /// Sets a port's pull-up mask. A `1` bit enables the internal pull-up.
    ///
    /// QMK's `mcp23018_set_config` writes the same mask to `IODIR` and `GPPU`;
    /// the Voyager's right-hand column inputs depend on this because they are
    /// active-low.
    ///
    /// # Errors
    /// Returns the underlying I2C error if the transfer fails.
    pub async fn set_pullups(&mut self, port: Port, mask: u8) -> Result<(), I2C::Error> {
        self.i2c
            .write(self.addr, &[GPPUA + port.offset(), mask])
            .await
    }

    /// Sets a port's direction and pull-up masks, matching QMK's helper.
    ///
    /// A `1` bit is an input with pull-up enabled; a `0` bit is an output with
    /// pull-up disabled.
    ///
    /// # Errors
    /// Returns the underlying I2C error if either transfer fails.
    pub async fn set_config(&mut self, port: Port, mask: u8) -> Result<(), I2C::Error> {
        self.set_direction(port, mask).await?;
        self.set_pullups(port, mask).await
    }

    /// Drives a port's output latch (`OLAT`). Only bits configured as outputs
    /// in [`set_direction`](Self::set_direction) affect pins.
    ///
    /// # Errors
    /// Returns the underlying I2C error if the transfer fails.
    pub async fn set_output(&mut self, port: Port, value: u8) -> Result<(), I2C::Error> {
        self.i2c
            .write(self.addr, &[OLATA + port.offset(), value])
            .await
    }

    /// Reads a port's input register (`GPIO`).
    ///
    /// # Errors
    /// Returns the underlying I2C error if the transfer fails.
    pub async fn read_port(&mut self, port: Port) -> Result<u8, I2C::Error> {
        let mut buf = [0u8; 1];
        self.i2c
            .write_read(self.addr, &[GPIOA + port.offset()], &mut buf)
            .await?;
        Ok(buf[0])
    }
}
