//! Power management — deep sleep, wake reasons, and battery monitoring.
//!
//! On wireless targets, `PowerManager` puts the MCU to sleep and reports
//! why it woke up. [`BatteryLevel`] tracks charge state for BLE battery
//! service reporting.
//!
//! Sleep is triggered by [`Action::Sleep`](crate::action::Action::Sleep).
//!
//! See also: [`ble`](crate::ble) for BLE host management.

/// Reason the MCU woke from deep sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReason {
    /// A key was pressed on the matrix.
    MatrixKey,
    /// A BLE event triggered the wake.
    BleEvent,
    /// USB cable was connected.
    UsbConnect,
    /// A timer expired.
    Timer,
}

/// Battery charge level reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryLevel {
    /// Charge percentage (0–100).
    pub percent: u8,
    /// Raw voltage in millivolts.
    pub millivolts: u16,
}

impl BatteryLevel {
    /// Creates a new battery level reading.
    #[must_use]
    pub const fn new(percent: u8, millivolts: u16) -> Self {
        Self {
            percent,
            millivolts,
        }
    }

    /// Returns `true` if the battery is at or below 10%.
    #[must_use]
    pub const fn is_low(&self) -> bool {
        self.percent <= 10
    }

    /// Returns `true` if the battery is at or below 5%.
    #[must_use]
    pub const fn is_critical(&self) -> bool {
        self.percent <= 5
    }
}

/// Async power management driver.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait PowerManager {
    /// Driver error type.
    type Error;
    /// Enter deep sleep mode.
    async fn enter_deep_sleep(&mut self) -> Result<(), Self::Error>;
    /// Returns the reason the MCU woke from sleep.
    async fn wake_reason(&self) -> WakeReason;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_reason_variants_distinct() {
        assert_ne!(WakeReason::MatrixKey, WakeReason::BleEvent);
        assert_ne!(WakeReason::UsbConnect, WakeReason::Timer);
    }

    #[test]
    fn battery_level_new() {
        let bl = BatteryLevel::new(85, 3700);
        assert_eq!(bl.percent, 85);
        assert_eq!(bl.millivolts, 3700);
    }

    #[test]
    fn battery_level_is_low() {
        assert!(BatteryLevel::new(10, 3200).is_low());
        assert!(BatteryLevel::new(5, 3100).is_low());
        assert!(!BatteryLevel::new(11, 3300).is_low());
    }

    #[test]
    fn battery_level_is_critical() {
        assert!(BatteryLevel::new(5, 3100).is_critical());
        assert!(BatteryLevel::new(0, 3000).is_critical());
        assert!(!BatteryLevel::new(6, 3150).is_critical());
    }

    #[test]
    fn battery_low_implies_not_necessarily_critical() {
        let bl = BatteryLevel::new(10, 3200);
        assert!(bl.is_low());
        assert!(!bl.is_critical());
    }
}
