//! EEPROM emulation over flash storage.
//!
//! Provides a byte-addressed in-memory cache backed by flash persistence,
//! compatible with QMK's EEPROM layout expectations.

use embedded_storage_async::nor_flash::NorFlash;
use sequential_storage::cache::CacheImpl;
use sequential_storage::map::MapStorage;

use crate::storage::StorageKey;

/// Errors returned by [`EepromShim`] operations.
#[derive(Debug)]
pub enum EepromError {
    /// The requested offset + length exceeds the cache size.
    OutOfBounds,
}

/// Byte-addressed EEPROM emulation backed by a fixed-size in-memory cache.
pub struct EepromShim<const SIZE: usize> {
    cache: [u8; SIZE],
    dirty: bool,
}

impl<const SIZE: usize> Default for EepromShim<SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SIZE: usize> EepromShim<SIZE> {
    /// Creates a new shim with the cache zeroed.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cache: [0u8; SIZE],
            dirty: false,
        }
    }

    /// Returns `true` if the cache has been modified since the last flush or load.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Reads a single byte from the in-memory cache.
    pub fn read_byte(&self, addr: u16) -> Result<u8, EepromError> {
        let idx = usize::from(addr);
        self.cache.get(idx).copied().ok_or(EepromError::OutOfBounds)
    }

    /// Writes a single byte into the in-memory cache, marking dirty only if data changed.
    pub fn write_byte(&mut self, addr: u16, value: u8) -> Result<(), EepromError> {
        let idx = usize::from(addr);
        let slot = self.cache.get_mut(idx).ok_or(EepromError::OutOfBounds)?;
        if *slot != value {
            *slot = value;
            self.dirty = true;
        }
        Ok(())
    }

    /// Reads bytes from the in-memory cache into `buf`.
    pub fn read(&self, offset: u16, buf: &mut [u8]) -> Result<(), EepromError> {
        let start = usize::from(offset);
        let end = start
            .checked_add(buf.len())
            .ok_or(EepromError::OutOfBounds)?;
        if end > SIZE {
            return Err(EepromError::OutOfBounds);
        }
        buf.copy_from_slice(&self.cache[start..end]);
        Ok(())
    }

    /// Writes bytes into the in-memory cache, marking dirty only if data changed.
    pub fn write(&mut self, offset: u16, data: &[u8]) -> Result<(), EepromError> {
        let start = usize::from(offset);
        let end = start
            .checked_add(data.len())
            .ok_or(EepromError::OutOfBounds)?;
        if end > SIZE {
            return Err(EepromError::OutOfBounds);
        }
        if self.cache[start..end] != *data {
            self.cache[start..end].copy_from_slice(data);
            self.dirty = true;
        }
        Ok(())
    }

    /// Loads the EEPROM block from flash into the in-memory cache.
    pub async fn load<S: NorFlash, C: CacheImpl<StorageKey>>(
        &mut self,
        storage: &mut MapStorage<StorageKey, S, C>,
        buf: &mut [u8],
    ) {
        if let Ok(Some(bytes)) = storage
            .fetch_item::<heapless::Vec<u8, SIZE>>(buf, &StorageKey::Eeprom)
            .await
        {
            let len = bytes.len().min(SIZE);
            if let Some(src) = bytes.get(..len) {
                if let Some(dst) = self.cache.get_mut(..len) {
                    dst.copy_from_slice(src);
                }
            }
        }
        self.dirty = false;
    }

    /// Persists the in-memory cache to flash as a single entry.
    pub async fn flush<S: NorFlash, C: CacheImpl<StorageKey>>(
        &mut self,
        storage: &mut MapStorage<StorageKey, S, C>,
        buf: &mut [u8],
    ) -> Result<(), sequential_storage::Error<S::Error>> {
        if !self.dirty {
            return Ok(());
        }
        let mut vec: heapless::Vec<u8, SIZE> = heapless::Vec::new();
        for &b in &self.cache {
            let _ = vec.push(b);
        }
        storage.store_item(buf, &StorageKey::Eeprom, &vec).await?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn read_write_round_trip() {
        let mut shim = EepromShim::<64>::new();
        shim.write(4, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        let mut out = [0u8; 4];
        shim.read(4, &mut out).unwrap();
        assert_eq!(out, [0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn read_defaults_to_zero() {
        let shim = EepromShim::<32>::new();
        let mut out = [0xFFu8; 8];
        shim.read(0, &mut out).unwrap();
        assert_eq!(out, [0u8; 8]);
    }

    #[test]
    fn out_of_bounds_read() {
        let shim = EepromShim::<8>::new();
        let mut out = [0u8; 4];
        assert!(shim.read(6, &mut out).is_err());
    }

    #[test]
    fn out_of_bounds_write() {
        let mut shim = EepromShim::<8>::new();
        assert!(shim.write(6, &[1, 2, 3]).is_err());
    }

    #[test]
    fn dirty_flag_tracking() {
        let mut shim = EepromShim::<16>::new();
        assert!(!shim.is_dirty());

        shim.write(0, &[0]).unwrap();
        assert!(!shim.is_dirty());

        shim.write(0, &[1]).unwrap();
        assert!(shim.is_dirty());
    }

    #[test]
    fn write_same_data_not_dirty() {
        let mut shim = EepromShim::<16>::new();
        shim.write(0, &[0, 0, 0]).unwrap();
        assert!(!shim.is_dirty());
    }

    #[test]
    fn read_byte_write_byte_round_trip() {
        let mut shim = EepromShim::<16>::new();
        shim.write_byte(5, 0xAB).unwrap();
        assert_eq!(shim.read_byte(5).unwrap(), 0xAB);
    }

    #[test]
    fn read_byte_out_of_bounds() {
        let shim = EepromShim::<8>::new();
        assert!(shim.read_byte(8).is_err());
    }

    #[test]
    fn write_byte_out_of_bounds() {
        let mut shim = EepromShim::<8>::new();
        assert!(shim.write_byte(8, 1).is_err());
    }

    #[test]
    fn write_byte_same_value_not_dirty() {
        let mut shim = EepromShim::<8>::new();
        shim.write_byte(0, 0).unwrap();
        assert!(!shim.is_dirty());
    }

    #[test]
    fn zero_length_read_write() {
        let mut shim = EepromShim::<8>::new();
        shim.write(8, &[]).unwrap();
        let mut out = [0u8; 0];
        shim.read(8, &mut out).unwrap();
    }
}
