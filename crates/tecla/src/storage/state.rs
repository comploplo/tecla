//! Persistent storage for keyboard state.
//!
//! Saves and restores [`crate::keyboard::KeyboardProcessor`]
//! state across power cycles using `sequential_storage` for wear-leveled
//! flash access.
//!
//! # Usage
//!
//! Sketch — `flash` and `FLASH_RANGE` come from the board crate, so this is not
//! a compilable example. See `examples/` in any `tecla-*` board crate.
//!
//! ```text
//! use tecla::storage::{load_state, save_state, StorageKey};
//! use sequential_storage::cache::NoCache;
//! use sequential_storage::map::{MapConfig, MapStorage};
//!
//! let config = MapConfig::new(FLASH_RANGE);
//! let mut storage = MapStorage::new(flash, config, NoCache::new());
//! let mut buf = [0u8; 256];
//!
//! // At boot — restore persisted state
//! let state = load_state(&mut storage, &mut buf).await;
//! processor.restore_state(
//!     state.default_state,
//!     state.locked_layers,
//!     state.rollover_mode,
//!     state.macro_slots,
//! );
//! ble_host_manager.restore_bonds(state.ble_bonds());
//!
//! // On state change — persist
//! save_state(&mut storage, &mut buf, &processor).await;
//! save_ble_bonds(&mut storage, &mut buf, ble_host_manager.bonds()).await;
//! ```

use embedded_storage_async::nor_flash::NorFlash;
use heapless::Deque;
use sequential_storage::cache::CacheImpl;
use sequential_storage::map::{Key, MapStorage, SerializationError};

use crate::action::KeyCode;
use crate::keyboard::{KeyboardProcessor, RolloverMode};
use crate::transport::ble::BondInfo;

/// Storage key identifiers for persisted keyboard state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StorageKey {
    /// Default layer bitmask (u32).
    DefaultState = 0,
    /// Locked layers bitmask (u32).
    LockedLayers = 1,
    /// Rollover mode (u8: 0 = 6KRO, 1 = NKRO).
    RolloverMode = 2,
    /// Dynamic macro slot 0.
    Macro0 = 3,
    /// Dynamic macro slot 1.
    Macro1 = 4,
    /// Dynamic macro slot 2.
    Macro2 = 5,
    /// Dynamic macro slot 3.
    Macro3 = 6,
    /// EEPROM emulation block.
    Eeprom = 7,
    /// BLE bond slot 0.
    BleHost0 = 8,
    /// BLE bond slot 1.
    BleHost1 = 9,
    /// BLE bond slot 2.
    BleHost2 = 10,
    /// BLE bond slot 3.
    BleHost3 = 11,
}

impl Key for StorageKey {
    #[expect(
        clippy::as_conversions,
        reason = "enum discriminant to u8 for storage key serialization"
    )]
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        let slot = buffer
            .first_mut()
            .ok_or(SerializationError::BufferTooSmall)?;
        *slot = *self as u8;
        Ok(1)
    }

    fn deserialize_from(buffer: &[u8]) -> Result<(Self, usize), SerializationError> {
        let &b = buffer.first().ok_or(SerializationError::BufferTooSmall)?;
        let key = match b {
            0 => Self::DefaultState,
            1 => Self::LockedLayers,
            2 => Self::RolloverMode,
            3 => Self::Macro0,
            4 => Self::Macro1,
            5 => Self::Macro2,
            6 => Self::Macro3,
            7 => Self::Eeprom,
            8 => Self::BleHost0,
            9 => Self::BleHost1,
            10 => Self::BleHost2,
            11 => Self::BleHost3,
            _ => return Err(SerializationError::InvalidFormat),
        };
        Ok((key, 1))
    }

    fn get_len(_buffer: &[u8]) -> Result<usize, SerializationError> {
        Ok(1)
    }
}

/// Restored keyboard state from flash.
pub struct PersistedState {
    /// Default layer bitmask.
    pub default_state: u32,
    /// Locked layers bitmask.
    pub locked_layers: u32,
    /// Rollover mode.
    pub rollover_mode: RolloverMode,
    /// Dynamic macro slot contents.
    pub macro_slots: [Deque<KeyCode, 64>; 4],
    /// BLE bond slots.
    pub ble_bonds: [Option<BondInfo>; 4],
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            default_state: 1,
            locked_layers: 0,
            rollover_mode: RolloverMode::Kro6,
            macro_slots: [Deque::new(), Deque::new(), Deque::new(), Deque::new()],
            ble_bonds: [None, None, None, None],
        }
    }
}

impl PersistedState {
    /// Returns the persisted BLE bond slots.
    #[must_use]
    pub fn ble_bonds(&self) -> &[Option<BondInfo>; 4] {
        &self.ble_bonds
    }
}

/// Loads persisted keyboard state from flash.
///
/// Missing keys return defaults. Corrupted data is silently ignored.
pub async fn load_state<S: NorFlash, C: CacheImpl<StorageKey>>(
    storage: &mut MapStorage<StorageKey, S, C>,
    buf: &mut [u8],
) -> PersistedState {
    let mut state = PersistedState::default();

    if let Ok(Some(v)) = storage
        .fetch_item::<u32>(buf, &StorageKey::DefaultState)
        .await
    {
        state.default_state = v;
    }

    if let Ok(Some(v)) = storage
        .fetch_item::<u32>(buf, &StorageKey::LockedLayers)
        .await
    {
        state.locked_layers = v;
    }

    if let Ok(Some(v)) = storage
        .fetch_item::<u32>(buf, &StorageKey::RolloverMode)
        .await
    {
        state.rollover_mode = if v == 1 {
            RolloverMode::Nkro
        } else {
            RolloverMode::Kro6
        };
    }

    let macro_keys = [
        StorageKey::Macro0,
        StorageKey::Macro1,
        StorageKey::Macro2,
        StorageKey::Macro3,
    ];
    for (i, key) in macro_keys.iter().enumerate() {
        if let Ok(Some(bytes)) = storage.fetch_item::<heapless::Vec<u8, 128>>(buf, key).await {
            if let Some(slot) = state.macro_slots.get_mut(i) {
                slot.clear();
                for &b in &bytes {
                    if let Some(kc) = KeyCode::from_usage_id(b) {
                        let _ = slot.push_back(kc);
                    }
                }
            }
        }
    }

    let ble_keys = [
        StorageKey::BleHost0,
        StorageKey::BleHost1,
        StorageKey::BleHost2,
        StorageKey::BleHost3,
    ];
    for (i, key) in ble_keys.iter().enumerate() {
        if let Ok(Some(bytes)) = storage.fetch_item::<heapless::Vec<u8, 23>>(buf, key).await {
            if let Some(bond) = BondInfo::from_bytes(&bytes) {
                if let Some(slot) = state.ble_bonds.get_mut(i) {
                    *slot = Some(bond);
                }
            }
        }
    }

    state
}

/// Saves keyboard state to flash.
pub async fn save_state<
    S: NorFlash,
    C: CacheImpl<StorageKey>,
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext: Copy + PartialEq + 'static,
>(
    storage: &mut MapStorage<StorageKey, S, C>,
    buf: &mut [u8],
    proc: &KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
) -> Result<(), sequential_storage::Error<S::Error>> {
    storage
        .store_item(buf, &StorageKey::DefaultState, &proc.default_state())
        .await?;
    storage
        .store_item(buf, &StorageKey::LockedLayers, &proc.locked_layers())
        .await?;

    let mode_u32: u32 = match proc.rollover_mode() {
        RolloverMode::Kro6 => 0,
        RolloverMode::Nkro => 1,
    };
    storage
        .store_item(buf, &StorageKey::RolloverMode, &mode_u32)
        .await?;

    let macro_keys = [
        StorageKey::Macro0,
        StorageKey::Macro1,
        StorageKey::Macro2,
        StorageKey::Macro3,
    ];
    for (slot_idx, key) in macro_keys.iter().enumerate() {
        let idx = u8::try_from(slot_idx).unwrap_or(u8::MAX);
        if let Some(slot) = proc.macro_slot(idx) {
            let mut bytes: heapless::Vec<u8, 128> = heapless::Vec::new();
            for kc in slot {
                let _ = bytes.push(kc.usage_id());
            }
            storage.store_item(buf, key, &bytes).await?;
        }
    }

    Ok(())
}

/// Saves BLE bond state to flash.
pub async fn save_ble_bonds<S: NorFlash, C: CacheImpl<StorageKey>>(
    storage: &mut MapStorage<StorageKey, S, C>,
    buf: &mut [u8],
    bonds: &[Option<BondInfo>; 4],
) -> Result<(), sequential_storage::Error<S::Error>> {
    let ble_keys = [
        StorageKey::BleHost0,
        StorageKey::BleHost1,
        StorageKey::BleHost2,
        StorageKey::BleHost3,
    ];
    for (i, key) in ble_keys.iter().enumerate() {
        if let Some(bond) = bonds.get(i).and_then(|s| s.as_ref()) {
            let raw = bond.to_bytes();
            let bytes: heapless::Vec<u8, 23> = heapless::Vec::from_slice(&raw).unwrap_or_default();
            storage.store_item(buf, key, &bytes).await?;
        }
    }
    Ok(())
}
