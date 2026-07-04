//! Mutex-guarded I2C bus sharing.
//!
//! The Voyager has exactly one physical I2C1 bus (SCL PB6, SDA PB7) but three
//! logical devices on it: the [`Mcp23018`](crate::mcp23018::Mcp23018) right-hand
//! expander (read by the matrix-scan task) and two
//! [`Is31fl3731`](crate::is31fl3731::Is31fl3731) LED drivers (written by the RGB
//! task). [`BitBangI2c`](crate::bitbang_i2c::BitBangI2c) owns its GPIO pins
//! exclusively, so it can't be handed to more than one driver directly. This
//! wraps the bus in an [`embassy_sync::mutex::Mutex`] and hands out lightweight
//! handles that lock the bus for the duration of one transaction.

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;
use embedded_hal_async::i2c::{ErrorType, I2c, Operation, SevenBitAddress};

/// A handle to a mutex-shared I2C bus. Cheap to create; borrows the mutex.
pub struct SharedI2c<'a, M: RawMutex, BUS> {
    bus: &'a Mutex<M, BUS>,
}

impl<'a, M: RawMutex, BUS> SharedI2c<'a, M, BUS> {
    /// Creates a handle to `bus`, locked for the duration of each transaction.
    pub fn new(bus: &'a Mutex<M, BUS>) -> Self {
        Self { bus }
    }
}

impl<M: RawMutex, BUS: ErrorType> ErrorType for SharedI2c<'_, M, BUS> {
    type Error = BUS::Error;
}

impl<M: RawMutex, BUS: I2c> I2c<SevenBitAddress> for SharedI2c<'_, M, BUS> {
    async fn transaction(
        &mut self,
        address: SevenBitAddress,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        self.bus.lock().await.transaction(address, operations).await
    }
}
