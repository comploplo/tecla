//! Firmware-agnostic board description types.
//!
//! These types are the common language between QMK, ZMK, and native tecla
//! board definitions. Both [`qmk`](crate::qmk) and [`zmk`](crate::zmk)
//! parsers produce a [`BoardConfig`] as their final output.

use std::collections::BTreeMap;

/// Unified board configuration produced by any parser backend.
#[derive(Debug, Clone)]
pub struct BoardConfig {
    /// Board name / identifier.
    pub name: Option<String>,
    /// Manufacturer.
    pub manufacturer: Option<String>,
    /// MCU identifier string (e.g. "RP2040", "STM32F303", "nRF52840").
    pub processor: Option<String>,
    /// Bootloader type (e.g. "rp2040", "stm32-dfu", "uf2").
    pub bootloader: Option<String>,
    /// Matrix row GPIO pin names.
    pub row_pins: Vec<String>,
    /// Matrix column GPIO pin names.
    pub col_pins: Vec<String>,
    /// Matrix dimensions (rows, cols).
    pub matrix_size: (usize, usize),
    /// Diode wiring direction.
    pub diode_direction: DiodeDirection,
    /// USB VID/PID (if applicable).
    pub usb: Option<UsbId>,
    /// Named physical layouts.
    pub layouts: BTreeMap<String, Layout>,
    /// Rotary encoder pin assignments.
    pub encoders: Vec<EncoderPins>,
    /// Feature flags (firmware-specific keys, values are enabled/disabled).
    pub features: BTreeMap<String, bool>,
    /// Parsed ZMK keymap bindings (layers of bindings), if a `.keymap` was found.
    pub zmk_keymap: Option<Vec<Vec<crate::zmk::dts::Binding>>>,
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self {
            name: None,
            manufacturer: None,
            processor: None,
            bootloader: None,
            row_pins: Vec::new(),
            col_pins: Vec::new(),
            matrix_size: (0, 0),
            diode_direction: DiodeDirection::Col2Row,
            usb: None,
            layouts: BTreeMap::new(),
            encoders: Vec::new(),
            features: BTreeMap::new(),
            zmk_keymap: None,
        }
    }
}

impl BoardConfig {
    /// Infer the MCU family from the processor string.
    #[must_use]
    pub fn mcu_family(&self) -> McuFamily {
        let Some(ref proc) = self.processor else {
            return McuFamily::Unknown;
        };
        McuFamily::from_processor(proc)
    }

    /// Total key count (rows × cols).
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.matrix_size.0 * self.matrix_size.1
    }

    /// Returns true if encoders are configured.
    #[must_use]
    pub fn has_encoders(&self) -> bool {
        !self.encoders.is_empty()
    }

    /// Check if a feature flag is enabled.
    #[must_use]
    pub fn has_feature(&self, name: &str) -> bool {
        self.features.get(name).copied().unwrap_or(false)
    }
}

/// Diode wiring direction for the key matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiodeDirection {
    /// Column-to-row: columns are outputs, rows are inputs.
    Col2Row,
    /// Row-to-column: rows are outputs, columns are inputs.
    Row2Col,
}

/// USB vendor and product IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbId {
    /// Vendor ID.
    pub vid: u16,
    /// Product ID.
    pub pid: u16,
}

/// Rotary encoder pin assignment.
#[derive(Debug, Clone)]
pub struct EncoderPins {
    /// GPIO pin for phase A.
    pub pin_a: String,
    /// GPIO pin for phase B.
    pub pin_b: String,
    /// Pulses per detent (default 4).
    pub resolution: u8,
}

/// A physical keyboard layout with per-key positions.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Per-key entries.
    pub keys: Vec<LayoutKey>,
}

/// A single key's position within a layout.
#[derive(Debug, Clone)]
pub struct LayoutKey {
    /// Matrix row.
    pub row: usize,
    /// Matrix column.
    pub col: usize,
    /// Physical X position (key units).
    pub x: f64,
    /// Physical Y position (key units).
    pub y: f64,
    /// Key width (key units, default 1.0).
    pub w: f64,
}

/// MCU family classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McuFamily {
    /// Raspberry Pi RP2040.
    Rp2040,
    /// STM32 family (F0xx, F1xx, F3xx, F4xx, etc.).
    Stm32,
    /// Nordic nRF52840.
    Nrf52,
    /// Espressif ESP32-S3.
    Esp32,
    /// Atmel AVR (ATmega32u4, AT90USB1286, etc.).
    Avr,
    /// Unknown or unsupported MCU.
    Unknown,
}

impl McuFamily {
    /// Classify a processor string into an MCU family.
    #[must_use]
    pub fn from_processor(proc: &str) -> Self {
        let lower = proc.to_ascii_lowercase();
        if lower.contains("rp2040") {
            Self::Rp2040
        } else if lower.starts_with("stm32") {
            Self::Stm32
        } else if lower.contains("nrf52") {
            Self::Nrf52
        } else if lower.contains("esp32") {
            Self::Esp32
        } else if lower.starts_with("atmega") || lower.starts_with("at90") {
            Self::Avr
        } else {
            Self::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcu_family_classification() {
        assert_eq!(McuFamily::from_processor("RP2040"), McuFamily::Rp2040);
        assert_eq!(McuFamily::from_processor("STM32F303"), McuFamily::Stm32);
        assert_eq!(McuFamily::from_processor("atmega32u4"), McuFamily::Avr);
        assert_eq!(McuFamily::from_processor("nRF52840"), McuFamily::Nrf52);
        assert_eq!(McuFamily::from_processor("ESP32-S3"), McuFamily::Esp32);
        assert_eq!(McuFamily::from_processor("weird_mcu"), McuFamily::Unknown);
    }

    #[test]
    fn board_config_defaults() {
        let bc = BoardConfig::default();
        assert_eq!(bc.mcu_family(), McuFamily::Unknown);
        assert_eq!(bc.key_count(), 0);
        assert!(!bc.has_encoders());
        assert!(!bc.has_feature("nkro"));
    }
}
