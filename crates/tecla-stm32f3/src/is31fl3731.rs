//! Minimal IS31FL3731 LED-matrix driver.
//!
//! The Voyager drives its 52 per-key RGB LEDs with two of these chips (one per
//! half), addressed `0x74` (left, address pin grounded) and `0x77` (right,
//! address pin tied to VCC). This driver covers exactly what per-key RGB
//! needs: power the chip up, enable every channel, and push a brightness
//! frame. It is generic over any async
//! [`embedded_hal_async::i2c::I2c`] implementation — on the Voyager that is
//! [`crate::shared_i2c::SharedI2c`], since the bus is shared with
//! [`crate::mcp23018::Mcp23018`].
//!
//! # Register map
//!
//! `0xFD` is the page-select (command) register: writing `[0xFD, page]`
//! before any other access selects which page subsequent register writes
//! target. There are 8 frame pages (`0x00`-`0x07`) plus one function page
//! (`0x0B`). Within a frame page: the LED-control bitmap (which of the 144
//! PWM channels are enabled) starts at offset `0x00` (18 bytes); blink-control
//! starts at `0x12` (18 bytes); the PWM brightness table starts at `0x24`
//! (144 bytes, one per channel). On the function page, the Shutdown register
//! is at `0x0A` (`0x00` = shutdown, `0x01` = normal operation).

use embassy_futures::yield_now;
use embedded_hal_async::i2c::I2c;

/// Left-hand chip address (address pin grounded).
pub const ADDR_LEFT: u8 = 0x74;
/// Right-hand chip address (address pin tied to VCC).
pub const ADDR_RIGHT: u8 = 0x77;

const CMD_REG: u8 = 0xFD;
const FUNCTION_PAGE: u8 = 0x0B;
const FRAME_0: u8 = 0x00;
const LED_CONTROL: u8 = 0x00;
const LED_CONTROL_LEN: usize = 18;
const BLINK_CONTROL: u8 = 0x12;
const BLINK_CONTROL_LEN: usize = 18;
const PWM_TABLE: u8 = 0x24;
const PWM_TABLE_LEN: usize = 144;
/// Bound how long an LED write can hold the bus shared with the matrix scan.
const PWM_CHUNK_LEN: usize = 8;
const SHUTDOWN: u8 = 0x0A;

/// A driver for one IS31FL3731 chip bound to an I2C bus and address.
pub struct Is31fl3731<I2C> {
    i2c: I2C,
    addr: u8,
}

impl<I2C: I2c> Is31fl3731<I2C> {
    /// Creates a driver for the chip at `addr` (see [`ADDR_LEFT`]/[`ADDR_RIGHT`]).
    pub fn new(i2c: I2C, addr: u8) -> Self {
        Self { i2c, addr }
    }

    async fn select_page(&mut self, page: u8) -> Result<(), I2C::Error> {
        self.i2c.write(self.addr, &[CMD_REG, page]).await?;
        yield_now().await;
        Ok(())
    }

    /// Powers the chip up, enables every PWM channel, and zeroes brightness.
    ///
    /// After this, frame page 0 is selected and stays selected — this driver
    /// never uses the chip's other frame pages, so [`write_frame`](Self::write_frame)
    /// doesn't need to reselect it each call.
    ///
    /// # Errors
    /// Returns the underlying I2C error if any transfer fails.
    pub async fn init(&mut self) -> Result<(), I2C::Error> {
        self.select_page(FUNCTION_PAGE).await?;
        self.i2c.write(self.addr, &[SHUTDOWN, 0x00]).await?;
        yield_now().await;
        self.i2c.write(self.addr, &[SHUTDOWN, 0x01]).await?;
        yield_now().await;

        self.select_page(FRAME_0).await?;

        let mut led_control = [0xFFu8; LED_CONTROL_LEN + 1];
        led_control[0] = LED_CONTROL;
        self.i2c.write(self.addr, &led_control).await?;
        yield_now().await;

        let mut blink_control = [0u8; BLINK_CONTROL_LEN + 1];
        blink_control[0] = BLINK_CONTROL;
        self.i2c.write(self.addr, &blink_control).await?;
        yield_now().await;

        self.write_frame(&[0; PWM_TABLE_LEN]).await
    }

    /// Pushes a full 144-channel brightness frame.
    ///
    /// # Errors
    /// Returns the underlying I2C error if the transfer fails.
    pub async fn write_frame(&mut self, pwm: &[u8; PWM_TABLE_LEN]) -> Result<(), I2C::Error> {
        self.write_frame_diff(pwm, None).await
    }

    /// Writes changed PWM chunks, yielding the shared bus between transactions.
    ///
    /// `previous` must describe a successfully delivered frame. Pass `None`
    /// after any error or reset so every channel is restored.
    ///
    /// # Errors
    /// Returns the underlying I2C error if a chunk fails to transfer.
    pub async fn write_frame_diff(
        &mut self,
        pwm: &[u8; PWM_TABLE_LEN],
        previous: Option<&[u8; PWM_TABLE_LEN]>,
    ) -> Result<(), I2C::Error> {
        for (index, chunk) in pwm.chunks(PWM_CHUNK_LEN).enumerate() {
            let start = index * PWM_CHUNK_LEN;
            if previous.is_some_and(|old| old[start..start + chunk.len()] == *chunk) {
                continue;
            }
            let mut buf = [0u8; PWM_CHUNK_LEN + 1];
            // The largest PWM offset is 0x24 + 136, safely within one byte.
            #[expect(clippy::cast_possible_truncation, clippy::as_conversions)]
            let register = PWM_TABLE + start as u8;
            buf[0] = register;
            buf[1..=chunk.len()].copy_from_slice(chunk);
            self.i2c.write(self.addr, &buf[..chunk.len() + 1]).await?;
            // SharedI2c releases its mutex when write returns; give scanning a turn.
            yield_now().await;
        }
        Ok(())
    }
}
