//! Production NOS75's two IS31FL3733-compatible I²C LED controllers.
//!
//! The acquired image selects 7-bit addresses `0x50`/`0x53` and maps 89 RGB
//! LEDs through the channel table at `0x0800CA54`. Register setup follows its
//! `0x0800817C` initializer, with global current reduced from stock `0xFF` to
//! `0x3F` for bring-up. The board's current-setting resistors are unverified;
//! callers must also limit frame brightness. No measured current is implied.
//! The caller owns PB6/PB7 I²C setup and the stock PC13/PB9 HIGH outputs.
//!
//! The protocol is described in the manufacturer's datasheet:
//! <https://www.lumissil.com/assets/pdf/core/IS31FL3733_DS.pdf>.

use embedded_hal_async::{delay::DelayNs, i2c::I2c};
use tecla::rgb::{LedDriver, Rgb};

/// Production key and side LED count (81 + 8).
pub const LED_COUNT: usize = 89;

/// Production `[controller, red, green, blue]` channel addresses.
///
/// Raw table SHA256: `3d557dbf6c7eebe37906a83aa96d23b191dc692dc4c58bc1d56d3333d2afd8e9`.
pub const CHANNEL_MAP: [[u8; 4]; LED_COUNT] = [
    [0, 11, 27, 43],
    [0, 10, 26, 42],
    [0, 9, 25, 41],
    [0, 8, 24, 40],
    [0, 59, 75, 91],
    [0, 58, 74, 90],
    [0, 57, 73, 89],
    [0, 56, 72, 88],
    [1, 58, 74, 90],
    [1, 57, 73, 89],
    [1, 56, 72, 88],
    [1, 104, 120, 136],
    [1, 55, 71, 87],
    [1, 150, 166, 182],
    [0, 0, 16, 32],
    [0, 1, 17, 33],
    [0, 2, 18, 34],
    [0, 3, 19, 35],
    [0, 4, 20, 36],
    [0, 5, 21, 37],
    [0, 6, 22, 38],
    [0, 7, 23, 39],
    [1, 48, 64, 80],
    [1, 49, 65, 81],
    [1, 50, 66, 82],
    [1, 51, 67, 83],
    [1, 52, 68, 84],
    [1, 53, 69, 85],
    [1, 103, 119, 135],
    [0, 48, 64, 80],
    [0, 49, 65, 81],
    [0, 50, 66, 82],
    [0, 51, 67, 83],
    [0, 52, 68, 84],
    [0, 53, 69, 85],
    [0, 54, 70, 86],
    [0, 55, 71, 87],
    [1, 96, 112, 128],
    [1, 97, 113, 129],
    [1, 98, 114, 130],
    [1, 99, 115, 131],
    [1, 100, 116, 132],
    [1, 101, 117, 133],
    [1, 54, 70, 86],
    [0, 96, 112, 128],
    [0, 97, 113, 129],
    [0, 98, 114, 130],
    [0, 99, 115, 131],
    [0, 100, 116, 132],
    [0, 101, 117, 133],
    [0, 102, 118, 134],
    [0, 103, 119, 135],
    [1, 144, 160, 176],
    [1, 145, 161, 177],
    [1, 146, 162, 178],
    [1, 147, 163, 179],
    [1, 148, 164, 180],
    [1, 102, 118, 134],
    [0, 144, 160, 176],
    [0, 145, 161, 177],
    [0, 146, 162, 178],
    [0, 147, 163, 179],
    [0, 148, 164, 180],
    [0, 149, 165, 181],
    [0, 150, 166, 182],
    [0, 151, 167, 183],
    [1, 106, 122, 138],
    [1, 105, 121, 137],
    [1, 154, 170, 186],
    [1, 153, 169, 185],
    [1, 149, 165, 181],
    [0, 107, 123, 139],
    [0, 106, 122, 138],
    [0, 105, 121, 137],
    [0, 104, 120, 136],
    [0, 155, 171, 187],
    [0, 154, 170, 186],
    [0, 153, 169, 185],
    [0, 152, 168, 184],
    [1, 152, 168, 184],
    [1, 151, 167, 183],
    [1, 3, 19, 35],
    [1, 2, 18, 34],
    [1, 1, 17, 33],
    [1, 0, 16, 32],
    [1, 4, 20, 36],
    [1, 5, 21, 37],
    [1, 6, 22, 38],
    [1, 7, 23, 39],
];

const ADDRESSES: [u8; 2] = [0x50, 0x53];
const CHANNELS: u8 = 192;
const CONTROL_BYTES: u8 = 24;
const CHUNK: u8 = 16;

/// A bus failure or a frame whose length does not match the production board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedError<E> {
    /// Exactly 89 RGB colors are required; no bus writes were attempted.
    InvalidLedCount,
    /// The underlying I²C transaction failed; later transactions were skipped.
    Bus(E),
}

/// Owns the bus used by both production NOS75 LED controllers.
pub struct Nos75Leds<I2C> {
    i2c: I2C,
}

impl<I2C: I2c> Nos75Leds<I2C> {
    /// Takes the configured I²C bus without performing hardware transactions.
    #[must_use]
    pub const fn new(i2c: I2C) -> Self {
        Self { i2c }
    }

    async fn register(&mut self, addr: u8, reg: u8, value: u8) -> Result<(), I2C::Error> {
        self.i2c.write(addr, &[reg, value]).await
    }

    async fn page(&mut self, addr: u8, page: u8) -> Result<(), I2C::Error> {
        self.register(addr, 0xFE, 0xC5).await?;
        self.register(addr, 0xFD, page).await
    }

    async fn clear(&mut self, addr: u8, count: u8) -> Result<(), I2C::Error> {
        let mut packet = [0; 17];
        let mut start = 0;
        while start < count {
            let len = CHUNK.min(count - start);
            packet[0] = start;
            self.i2c.write(addr, &packet[..=usize::from(len)]).await?;
            start += len;
        }
        Ok(())
    }

    /// Clears brightness, configures both chips and enables only mapped channels.
    ///
    /// Call after the stock HIGH board outputs have been established. The 1 ms
    /// wake delay per chip matches stock. Software/global current limits must
    /// still be applied to frames; the current-setting resistors are unknown.
    ///
    /// # Errors
    /// Stops immediately on an I²C failure. Retry the entire initialization
    /// successfully before sending animation frames.
    pub async fn init(&mut self, delay: &mut impl DelayNs) -> Result<(), I2C::Error> {
        for addr in ADDRESSES {
            self.page(addr, 0).await?;
            self.clear(addr, CONTROL_BYTES).await?;
            self.page(addr, 1).await?;
            self.clear(addr, CHANNELS).await?;
            self.page(addr, 3).await?;
            self.register(addr, 0x0F, 0).await?;
            self.register(addr, 0x10, 0).await?;
            self.register(addr, 0x01, 0x3F).await?;
            self.register(addr, 0x00, 0x01).await?;
            delay.delay_ms(1).await;
        }

        for (driver, addr) in ADDRESSES.into_iter().enumerate() {
            self.page(addr, 0).await?;
            let mut packet = [0; 17];
            for start in [0, 16] {
                let len = CHUNK.min(CONTROL_BYTES - start);
                packet.fill(0);
                packet[0] = start;
                for &[chip, red, green, blue] in &CHANNEL_MAP {
                    if usize::from(chip) == driver {
                        for channel in [red, green, blue] {
                            let register = channel >> 3;
                            if register >= start && register < start + len {
                                packet[1 + usize::from(register - start)] |= 1 << (channel & 7);
                            }
                        }
                    }
                }
                self.i2c.write(addr, &packet[..=usize::from(len)]).await?;
            }
        }
        Ok(())
    }

    /// Blanks both controllers through their software-shutdown configuration.
    ///
    /// Call [`Self::init`] before resuming frames. Channel data is retained by
    /// the controllers while shut down.
    ///
    /// # Errors
    /// Stops immediately if an unlock, page selection or shutdown write fails.
    pub async fn shutdown(&mut self) -> Result<(), I2C::Error> {
        for addr in ADDRESSES {
            self.page(addr, 3).await?;
            self.register(addr, 0x00, 0x00).await?;
        }
        Ok(())
    }
}

impl<I2C: I2c> LedDriver for Nos75Leds<I2C> {
    type Error = LedError<I2C::Error>;

    async fn write(&mut self, leds: &[Rgb]) -> Result<(), Self::Error> {
        if leds.len() != LED_COUNT {
            return Err(LedError::InvalidLedCount);
        }
        for (driver, addr) in ADDRESSES.into_iter().enumerate() {
            self.page(addr, 1).await.map_err(LedError::Bus)?;
            let mut packet = [0; 17];
            let mut start = 0;
            while start < CHANNELS {
                packet.fill(0);
                packet[0] = start;
                for ([chip, red, green, blue], color) in CHANNEL_MAP.iter().zip(leds) {
                    if usize::from(*chip) == driver {
                        for (channel, value) in
                            [(*red, color.r), (*green, color.g), (*blue, color.b)]
                        {
                            if channel >= start && channel < start + CHUNK {
                                packet[1 + usize::from(channel - start)] = value;
                            }
                        }
                    }
                }
                self.i2c.write(addr, &packet).await.map_err(LedError::Bus)?;
                start += CHUNK;
            }
        }
        Ok(())
    }
}
