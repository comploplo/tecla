//! BLE abstractions — reporter trait, bond management, multi-host switching.
//!
//! Wireless keyboards use `BleReporter` (extending `AsyncReporter`) to
//! send HID reports over
//! Bluetooth Low Energy. [`BleHostManager`] tracks up to `MAX_HOSTS` bonded
//! peers, switchable at runtime via
//! [`Action::BleHostSwitch`](crate::action::Action::BleHostSwitch).
//!
//! See also: [`power`](crate::power) for deep sleep and battery monitoring.

/// BLE-specific errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleError {
    /// No active BLE connection.
    NotConnected,
    /// Advertising failed to start.
    AdvertisingFailed,
    /// All bond storage slots are full.
    BondStorageFull,
    /// Connection attempt failed.
    ConnectionFailed,
}

impl core::fmt::Display for BleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotConnected => write!(f, "no active BLE connection"),
            Self::AdvertisingFailed => write!(f, "BLE advertising failed to start"),
            Self::BondStorageFull => write!(f, "all bond storage slots are full"),
            Self::ConnectionFailed => write!(f, "BLE connection attempt failed"),
        }
    }
}

/// Stored bond information for a paired host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BondInfo {
    /// Host slot index (0–3).
    pub host_id: u8,
    /// 6-byte BLE address.
    pub address: [u8; 6],
    /// 16-byte Identity Resolving Key.
    pub irk: [u8; 16],
}

impl BondInfo {
    /// Creates a new bond info entry.
    #[must_use]
    pub const fn new(host_id: u8, address: [u8; 6], irk: [u8; 16]) -> Self {
        Self {
            host_id,
            address,
            irk,
        }
    }

    /// Serializes to 23 bytes: 1 byte host_id + 6 bytes address + 16 bytes irk.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 23] {
        let mut buf = [0u8; 23];
        buf[0] = self.host_id;
        buf[1..7].copy_from_slice(&self.address);
        buf[7..23].copy_from_slice(&self.irk);
        buf
    }

    /// Deserializes from 23 bytes. Returns `None` if the slice length is wrong.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 23 {
            return None;
        }
        let &host_id = bytes.first()?;
        if host_id >= 4 {
            return None;
        }
        let mut address = [0u8; 6];
        address.copy_from_slice(bytes.get(1..7)?);
        let mut irk = [0u8; 16];
        irk.copy_from_slice(bytes.get(7..23)?);
        Some(Self {
            host_id,
            address,
            irk,
        })
    }
}

/// A command sent from the keyboard task to the BLE task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleCommand {
    /// Start BLE advertising.
    Advertise,
    /// Disconnect the active BLE connection.
    Disconnect,
    /// Switch to a bonded host by index.
    SwitchHost(u8),
}

/// Channel for routing [`BleCommand`]s from the keyboard task to the BLE task.
#[cfg(feature = "embassy")]
pub type BleCommandChannel<M, const N: usize> = embassy_sync::channel::Channel<M, BleCommand, N>;

/// Async BLE HID reporter — extends [`AsyncReporter`](crate::hid::AsyncReporter) with BLE lifecycle.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait BleReporter: crate::hid::AsyncReporter {
    /// Establish a BLE connection.
    async fn connect(&mut self) -> Result<(), BleError>;
    /// Disconnect the active BLE connection.
    async fn disconnect(&mut self) -> Result<(), BleError>;
    /// Returns whether a BLE connection is active.
    async fn is_connected(&self) -> bool;
    /// Start BLE advertising.
    async fn advertise(&mut self) -> Result<(), BleError>;
}

/// Manages bonded BLE hosts and active-host selection.
pub struct BleHostManager<const MAX_HOSTS: usize> {
    bonds: [Option<BondInfo>; MAX_HOSTS],
    active_host: u8,
}

impl<const MAX_HOSTS: usize> BleHostManager<MAX_HOSTS> {
    const NONE_BOND: Option<BondInfo> = None;

    /// Creates a new host manager with no bonds and host 0 active.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bonds: [Self::NONE_BOND; MAX_HOSTS],
            active_host: 0,
        }
    }

    /// Returns the currently active host index.
    #[must_use]
    pub const fn active_host(&self) -> u8 {
        self.active_host
    }

    /// Switches to the given host index (wraps around).
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "wrapping host index within MAX_HOSTS"
    )]
    pub fn switch_host(&mut self, host_id: u8) {
        self.active_host = host_id % MAX_HOSTS as u8;
    }

    /// Advances to the next host (wraps around).
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "wrapping host index within MAX_HOSTS"
    )]
    pub fn next_host(&mut self) {
        self.active_host = (self.active_host + 1) % MAX_HOSTS as u8;
    }

    /// Returns the bond info for the given host, if present.
    #[must_use]
    pub fn bond(&self, host_id: u8) -> Option<&BondInfo> {
        self.bonds
            .get(usize::from(host_id))
            .and_then(|b| b.as_ref())
    }

    /// Returns the bond info for the active host, if present.
    #[must_use]
    pub fn active_bond(&self) -> Option<&BondInfo> {
        self.bond(self.active_host)
    }

    /// Stores bond info, overwriting any previous bond in that slot.
    pub fn store_bond(&mut self, info: BondInfo) -> Result<(), BleError> {
        let idx = usize::from(info.host_id);
        let slot = self.bonds.get_mut(idx).ok_or(BleError::BondStorageFull)?;
        *slot = Some(info);
        Ok(())
    }

    /// Removes the bond at the given host index. Returns `true` if a bond was removed.
    pub fn remove_bond(&mut self, host_id: u8) -> bool {
        if let Some(slot) = self.bonds.get_mut(usize::from(host_id)) {
            let had = slot.is_some();
            *slot = None;
            had
        } else {
            false
        }
    }

    /// Returns a reference to all bond slots.
    #[must_use]
    pub fn bonds(&self) -> &[Option<BondInfo>; MAX_HOSTS] {
        &self.bonds
    }

    /// Restores bonds from persisted state (e.g., loaded from flash at boot).
    pub fn restore_bonds(&mut self, bonds: &[Option<BondInfo>; MAX_HOSTS]) {
        self.bonds = *bonds;
    }

    /// Returns the number of stored bonds.
    #[must_use]
    pub fn bond_count(&self) -> usize {
        self.bonds.iter().filter(|b| b.is_some()).count()
    }
}

impl<const MAX_HOSTS: usize> Default for BleHostManager<MAX_HOSTS> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn ble_command_variants_distinct() {
        assert_ne!(BleCommand::Advertise, BleCommand::Disconnect);
        assert_ne!(BleCommand::SwitchHost(0), BleCommand::SwitchHost(1));
        assert_ne!(BleCommand::Advertise, BleCommand::SwitchHost(0));
    }

    #[test]
    fn ble_error_variants_distinct() {
        assert_ne!(BleError::NotConnected, BleError::AdvertisingFailed);
        assert_ne!(BleError::BondStorageFull, BleError::ConnectionFailed);
    }

    #[test]
    fn bond_info_new() {
        let info = BondInfo::new(0, [1; 6], [2; 16]);
        assert_eq!(info.host_id, 0);
        assert_eq!(info.address, [1; 6]);
        assert_eq!(info.irk, [2; 16]);
    }

    #[test]
    fn host_manager_new() {
        let mgr = BleHostManager::<4>::new();
        assert_eq!(mgr.active_host(), 0);
        assert_eq!(mgr.bond_count(), 0);
    }

    #[test]
    fn host_manager_switch_host() {
        let mut mgr = BleHostManager::<4>::new();
        mgr.switch_host(2);
        assert_eq!(mgr.active_host(), 2);
    }

    #[test]
    fn host_manager_switch_host_wraps() {
        let mut mgr = BleHostManager::<4>::new();
        mgr.switch_host(5);
        assert_eq!(mgr.active_host(), 1);
    }

    #[test]
    fn host_manager_next_host() {
        let mut mgr = BleHostManager::<4>::new();
        mgr.next_host();
        assert_eq!(mgr.active_host(), 1);
    }

    #[test]
    fn host_manager_next_host_wraps() {
        let mut mgr = BleHostManager::<4>::new();
        mgr.switch_host(3);
        mgr.next_host();
        assert_eq!(mgr.active_host(), 0);
    }

    #[test]
    fn host_manager_store_and_retrieve_bond() {
        let mut mgr = BleHostManager::<4>::new();
        let info = BondInfo::new(1, [0xAA; 6], [0xBB; 16]);
        assert!(mgr.store_bond(info).is_ok());
        assert_eq!(mgr.bond_count(), 1);
        assert_eq!(mgr.bond(1), Some(&info));
    }

    #[test]
    fn host_manager_store_bond_out_of_range() {
        let mut mgr = BleHostManager::<2>::new();
        let info = BondInfo::new(5, [0; 6], [0; 16]);
        assert_eq!(mgr.store_bond(info), Err(BleError::BondStorageFull));
    }

    #[test]
    fn host_manager_remove_bond() {
        let mut mgr = BleHostManager::<4>::new();
        let info = BondInfo::new(0, [1; 6], [2; 16]);
        mgr.store_bond(info).unwrap();
        assert!(mgr.remove_bond(0));
        assert_eq!(mgr.bond_count(), 0);
        assert!(mgr.bond(0).is_none());
    }

    #[test]
    fn host_manager_remove_nonexistent_bond() {
        let mut mgr = BleHostManager::<4>::new();
        assert!(!mgr.remove_bond(0));
    }

    #[test]
    fn host_manager_remove_out_of_range() {
        let mut mgr = BleHostManager::<2>::new();
        assert!(!mgr.remove_bond(5));
    }

    #[test]
    fn host_manager_active_bond() {
        let mut mgr = BleHostManager::<4>::new();
        assert!(mgr.active_bond().is_none());
        let info = BondInfo::new(0, [1; 6], [2; 16]);
        mgr.store_bond(info).unwrap();
        assert_eq!(mgr.active_bond(), Some(&info));
    }

    #[test]
    fn host_manager_default_matches_new() {
        let a = BleHostManager::<4>::new();
        let b = BleHostManager::<4>::default();
        assert_eq!(a.active_host(), b.active_host());
        assert_eq!(a.bond_count(), b.bond_count());
    }

    #[test]
    fn ble_error_display_non_empty() {
        use core::fmt::Write;
        struct Buf([u8; 64], usize);
        impl Write for Buf {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let b = s.as_bytes();
                let end = self.1 + b.len();
                let dest = self.0.get_mut(self.1..end).ok_or(core::fmt::Error)?;
                dest.copy_from_slice(b);
                self.1 = end;
                Ok(())
            }

            fn write_char(&mut self, c: char) -> core::fmt::Result {
                self.write_str(c.encode_utf8(&mut [0; 4]))
            }

            fn write_fmt(&mut self, args: core::fmt::Arguments<'_>) -> core::fmt::Result {
                core::fmt::write(self, args)
            }
        }
        let variants = [
            BleError::NotConnected,
            BleError::AdvertisingFailed,
            BleError::BondStorageFull,
            BleError::ConnectionFailed,
        ];
        for v in &variants {
            let mut buf = Buf([0; 64], 0);
            write!(buf, "{v}").unwrap();
            assert!(buf.1 > 0);
        }
    }

    #[test]
    fn bond_info_round_trip() {
        let info = BondInfo::new(2, [0xAA; 6], [0xBB; 16]);
        let bytes = info.to_bytes();
        let restored = BondInfo::from_bytes(&bytes).unwrap();
        assert_eq!(info, restored);
    }

    #[test]
    fn bond_info_from_bytes_wrong_length() {
        assert!(BondInfo::from_bytes(&[0u8; 10]).is_none());
        assert!(BondInfo::from_bytes(&[0u8; 24]).is_none());
        assert!(BondInfo::from_bytes(&[]).is_none());
    }

    #[test]
    fn bond_info_from_bytes_host_id_out_of_range() {
        let mut bytes = [0u8; 23];
        bytes[0] = 4;
        assert!(BondInfo::from_bytes(&bytes).is_none());
        bytes[0] = 255;
        assert!(BondInfo::from_bytes(&bytes).is_none());
    }

    #[test]
    fn host_manager_overwrite_bond() {
        let mut mgr = BleHostManager::<4>::new();
        let info1 = BondInfo::new(0, [1; 6], [2; 16]);
        let info2 = BondInfo::new(0, [3; 6], [4; 16]);
        mgr.store_bond(info1).unwrap();
        mgr.store_bond(info2).unwrap();
        assert_eq!(mgr.bond_count(), 1);
        assert_eq!(mgr.bond(0).unwrap().address, [3; 6]);
    }
}
