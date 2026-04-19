//! QMK `info.json` parser — extracts matrix pins, USB descriptors, and layouts.
//!
//! [`parse`] deserializes a QMK keyboard's `info.json` (JSON5-tolerant) into a
//! [`QmkConfig`] and validates that matrix pin counts match declared dimensions
//! and that layout keys fall within the matrix bounds.
//!
//! Downstream consumers:
//! - [`keymap`](crate::qmk::keymap) uses the config to resolve `#include` paths
//!   and layer dimensions during transpilation.
//! - [`via_json`](crate::qmk::via_json) generates Via-compatible JSON definitions
//!   from the parsed config.
//! - [`proximity`](crate::proximity) builds key-distance tables from the
//!   physical layout coordinates.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Top-level parsed QMK keyboard configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct QmkConfig {
    /// Keyboard identifier (e.g. "planck/rev6").
    pub keyboard_name: Option<String>,
    /// Manufacturer name.
    pub manufacturer: Option<String>,
    /// MCU identifier (e.g. "RP2040", "STM32F303", "atmega32u4").
    pub processor: Option<String>,
    /// Bootloader type (e.g. "rp2040", "stm32-dfu", "caterina", "atmel-dfu").
    pub bootloader: Option<String>,
    /// Diode wiring direction: `"COL2ROW"` or `"ROW2COL"`.
    pub diode_direction: Option<DiodeDirection>,
    /// Development board name (e.g. "promicro", "blackpill_f401"). Informational only.
    pub development_board: Option<String>,
    /// USB descriptor fields.
    #[serde(default)]
    pub usb: UsbConfig,
    /// GPIO pin assignments for the key matrix.
    #[serde(default)]
    pub matrix_pins: MatrixPins,
    /// Matrix dimensions.
    #[serde(default)]
    pub matrix_size: MatrixSize,
    /// Named physical layouts with per-key matrix coordinates.
    #[serde(default)]
    pub layouts: BTreeMap<String, LayoutConfig>,
    /// Rotary encoder configuration.
    #[serde(default)]
    pub encoder: EncoderConfig,
    /// QMK feature flags (bootmagic, mousekey, nkro, etc.).
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

/// Diode wiring direction for the key matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum DiodeDirection {
    /// Column-to-row: columns are outputs, rows are inputs.
    #[serde(rename = "COL2ROW")]
    Col2Row,
    /// Row-to-column: rows are outputs, columns are inputs.
    #[serde(rename = "ROW2COL")]
    Row2Col,
}

/// Rotary encoder configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EncoderConfig {
    /// Per-encoder pin assignments.
    #[serde(default)]
    pub rotary: Vec<RotaryEncoder>,
}

/// A single rotary encoder's pin configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RotaryEncoder {
    /// GPIO pin for encoder phase A.
    pub pin_a: String,
    /// GPIO pin for encoder phase B.
    pub pin_b: String,
    /// Encoder resolution (pulses per detent). Defaults to 4.
    #[serde(default = "default_encoder_resolution")]
    pub resolution: u8,
}

fn default_encoder_resolution() -> u8 {
    4
}

/// USB VID/PID from info.json.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UsbConfig {
    /// Vendor ID (hex string like `0x03A8`).
    #[serde(default, deserialize_with = "deserialize_hex_u16")]
    pub vid: u16,
    /// Product ID (hex string like `0xA4F9`).
    #[serde(default, deserialize_with = "deserialize_hex_u16")]
    pub pid: u16,
}

/// Row and column GPIO pin names.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MatrixPins {
    /// Row pin names, e.g. `B0, B1, B2, B3`. `None` entries represent unused pins.
    #[serde(default)]
    pub rows: Vec<Option<String>>,
    /// Column pin names, e.g. `A0, A1, A2`. `None` entries represent unused pins.
    #[serde(default)]
    pub cols: Vec<Option<String>>,
}

/// Declared matrix dimensions.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MatrixSize {
    /// Number of rows.
    #[serde(default)]
    pub rows: usize,
    /// Number of columns.
    #[serde(default)]
    pub cols: usize,
}

/// A named layout containing key position metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct LayoutConfig {
    /// Per-key entries with matrix coordinates and physical position.
    pub layout: Vec<LayoutKey>,
}

/// A single key's position within a layout.
#[derive(Debug, Clone, Deserialize)]
pub struct LayoutKey {
    /// Matrix coordinates as [row, col].
    pub matrix: [usize; 2],
    /// Physical X position (key units).
    #[serde(default)]
    pub x: f64,
    /// Physical Y position (key units).
    #[serde(default)]
    pub y: f64,
    /// Key width in key units (defaults to 1.0).
    #[serde(default = "default_key_width")]
    pub w: f64,
}

fn default_key_width() -> f64 {
    1.0
}

/// Errors from parsing or validating a QMK config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// JSON deserialization failed.
    Json(String),
    /// matrix_pins count does not match matrix_size.
    MatrixMismatch {
        /// Pin row count.
        pins_rows: usize,
        /// Pin column count.
        pins_cols: usize,
        /// Declared row count.
        size_rows: usize,
        /// Declared column count.
        size_cols: usize,
    },
    /// USB configuration is invalid.
    InvalidUsb(String),
    /// A layout key references a matrix position outside the declared size.
    LayoutKeyOutOfBounds {
        /// Layout name.
        layout: String,
        /// Key index within the layout.
        key_index: usize,
        /// Out-of-bounds row.
        row: usize,
        /// Out-of-bounds column.
        col: usize,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(msg) => write!(f, "JSON parse error: {msg}"),
            Self::MatrixMismatch {
                pins_rows,
                pins_cols,
                size_rows,
                size_cols,
            } => {
                write!(
                    f,
                    "matrix_pins ({pins_rows} rows, {pins_cols} cols) does not match matrix_size ({size_rows} rows, {size_cols} cols)"
                )
            }
            Self::InvalidUsb(msg) => write!(f, "invalid USB config: {msg}"),
            Self::LayoutKeyOutOfBounds {
                layout,
                key_index,
                row,
                col,
            } => {
                write!(
                    f,
                    "layout {layout} key {key_index} matrix [{row}, {col}] out of bounds"
                )
            }
        }
    }
}

#[expect(clippy::missing_trait_methods, reason = "default impls are sufficient")]
impl std::error::Error for ConfigError {}

/// MCU family as determined from the `processor` field.
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

impl QmkConfig {
    /// Infer the MCU family from the `processor` field.
    #[must_use]
    pub fn mcu_family(&self) -> McuFamily {
        let Some(ref proc) = self.processor else {
            return McuFamily::Unknown;
        };
        let lower = proc.to_ascii_lowercase();
        if lower.contains("rp2040") {
            McuFamily::Rp2040
        } else if lower.starts_with("stm32") {
            McuFamily::Stm32
        } else if lower.contains("nrf52") {
            McuFamily::Nrf52
        } else if lower.contains("esp32") {
            McuFamily::Esp32
        } else if lower.starts_with("atmega") || lower.starts_with("at90") {
            McuFamily::Avr
        } else {
            McuFamily::Unknown
        }
    }

    /// Returns the effective matrix dimensions, preferring `matrix_size` and
    /// falling back to pin counts.
    #[must_use]
    pub fn matrix_dimensions(&self) -> (usize, usize) {
        let rows = if self.matrix_size.rows > 0 {
            self.matrix_size.rows
        } else {
            self.matrix_pins.rows.len()
        };
        let cols = if self.matrix_size.cols > 0 {
            self.matrix_size.cols
        } else {
            self.matrix_pins.cols.len()
        };
        (rows, cols)
    }

    /// Returns the total number of keys in the matrix (rows × cols).
    #[must_use]
    pub fn key_count(&self) -> usize {
        let (rows, cols) = self.matrix_dimensions();
        rows * cols
    }

    /// Returns true if the keyboard has rotary encoders configured.
    #[must_use]
    pub fn has_encoders(&self) -> bool {
        !self.encoder.rotary.is_empty()
    }

    /// Returns true if a QMK feature flag is enabled.
    #[must_use]
    pub fn has_feature(&self, name: &str) -> bool {
        self.features.get(name).copied().unwrap_or(false)
    }

    /// Convert to a firmware-agnostic [`BoardConfig`](crate::board::BoardConfig).
    #[must_use]
    pub fn to_board_config(&self) -> crate::board::BoardConfig {
        use crate::board;
        let (rows, cols) = self.matrix_dimensions();
        board::BoardConfig {
            name: self.keyboard_name.clone(),
            manufacturer: self.manufacturer.clone(),
            processor: self.processor.clone(),
            bootloader: self.bootloader.clone(),
            row_pins: self
                .matrix_pins
                .rows
                .iter()
                .filter_map(|p| p.clone())
                .collect(),
            col_pins: self
                .matrix_pins
                .cols
                .iter()
                .filter_map(|p| p.clone())
                .collect(),
            matrix_size: (rows, cols),
            diode_direction: match self.diode_direction {
                Some(DiodeDirection::Col2Row) => board::DiodeDirection::Col2Row,
                Some(DiodeDirection::Row2Col) => board::DiodeDirection::Row2Col,
                None => board::DiodeDirection::Col2Row,
            },
            usb: Some(board::UsbId {
                vid: self.usb.vid,
                pid: self.usb.pid,
            }),
            layouts: self
                .layouts
                .iter()
                .map(|(name, lc)| (name.clone(), layout_to_board(lc)))
                .collect(),
            encoders: self
                .encoder
                .rotary
                .iter()
                .map(|e| board::EncoderPins {
                    pin_a: e.pin_a.clone(),
                    pin_b: e.pin_b.clone(),
                    resolution: e.resolution,
                })
                .collect(),
            features: self.features.clone(),
            zmk_keymap: None,
        }
    }
}

fn layout_to_board(lc: &LayoutConfig) -> crate::board::Layout {
    crate::board::Layout {
        keys: lc
            .layout
            .iter()
            .map(|k| crate::board::LayoutKey {
                row: k.matrix[0],
                col: k.matrix[1],
                x: k.x,
                y: k.y,
                w: k.w,
            })
            .collect(),
    }
}

/// Parses and validates a QMK `info.json` string.
///
/// Uses a JSON5 parser to tolerate comments, trailing commas, and other
/// JSON5-isms found in QMK configs.
pub fn parse(json: &str) -> Result<QmkConfig, ConfigError> {
    let config: QmkConfig =
        deser_hjson::from_str(json).map_err(|e| ConfigError::Json(e.to_string()))?;
    validate(&config)?;
    Ok(config)
}

/// Validates internal consistency of a parsed config.
pub fn validate(config: &QmkConfig) -> Result<(), ConfigError> {
    let pins_rows = config.matrix_pins.rows.len();
    let pins_cols = config.matrix_pins.cols.len();
    let size_rows = config.matrix_size.rows;
    let size_cols = config.matrix_size.cols;

    if pins_rows > 0
        && pins_cols > 0
        && size_rows > 0
        && size_cols > 0
        && (pins_rows != size_rows || pins_cols != size_cols)
    {
        return Err(ConfigError::MatrixMismatch {
            pins_rows,
            pins_cols,
            size_rows,
            size_cols,
        });
    }

    for (name, layout) in &config.layouts {
        for (i, key) in layout.layout.iter().enumerate() {
            let row = key.matrix.first().copied().unwrap_or(0);
            let col = key.matrix.get(1).copied().unwrap_or(0);
            if size_rows > 0 && size_cols > 0 && (row >= size_rows || col >= size_cols) {
                return Err(ConfigError::LayoutKeyOutOfBounds {
                    layout: name.clone(),
                    key_index: i,
                    row,
                    col,
                });
            }
        }
    }

    Ok(())
}

fn deserialize_hex_u16<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    let s = String::deserialize(deserializer)?;
    let trimmed = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(&s);
    u16::from_str_radix(trimmed, 16).map_err(serde::de::Error::custom)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let json = r#"{
            "keyboard_name": "test/board",
            "manufacturer": "TestCo",
            "usb": { "vid": "0x1234", "pid": "0x5678" },
            "matrix_pins": { "rows": ["B0", "B1"], "cols": ["A0", "A1", "A2"] },
            "matrix_size": { "rows": 2, "cols": 3 },
            "layouts": {
                "LAYOUT": {
                    "layout": [
                        {"matrix": [0, 0], "x": 0, "y": 0},
                        {"matrix": [0, 1], "x": 1, "y": 0},
                        {"matrix": [0, 2], "x": 2, "y": 0},
                        {"matrix": [1, 0], "x": 0, "y": 1},
                        {"matrix": [1, 1], "x": 1, "y": 1},
                        {"matrix": [1, 2], "x": 2, "y": 1}
                    ]
                }
            }
        }"#;

        let config = parse(json).unwrap();
        assert_eq!(config.keyboard_name.as_deref(), Some("test/board"));
        assert_eq!(config.usb.vid, 0x1234);
        assert_eq!(config.usb.pid, 0x5678);
        assert_eq!(config.matrix_size.rows, 2);
        assert_eq!(config.matrix_size.cols, 3);
        assert_eq!(config.matrix_pins.rows.len(), 2);
        assert_eq!(config.matrix_pins.cols.len(), 3);
        assert_eq!(
            config.matrix_pins.rows.first().unwrap().as_deref(),
            Some("B0")
        );
        assert_eq!(config.layouts.len(), 1);
        assert!(config
            .layouts
            .get("LAYOUT")
            .is_some_and(|l| l.layout.len() == 6));
    }

    #[test]
    fn parse_empty_config() {
        let json = "{}";
        let config = parse(json).unwrap();
        assert!(config.keyboard_name.is_none());
        assert_eq!(config.matrix_size.rows, 0);
    }

    #[test]
    fn matrix_mismatch_error() {
        let json = r#"{
            "matrix_pins": { "rows": ["B0"], "cols": ["A0"] },
            "matrix_size": { "rows": 2, "cols": 3 }
        }"#;
        let err = parse(json).unwrap_err();
        assert!(matches!(err, ConfigError::MatrixMismatch { .. }));
        assert!(err.to_string().contains("does not match"));
    }

    #[test]
    fn layout_key_out_of_bounds() {
        let json = r#"{
            "matrix_size": { "rows": 2, "cols": 2 },
            "layouts": {
                "LAYOUT": {
                    "layout": [{"matrix": [5, 0], "x": 0, "y": 0}]
                }
            }
        }"#;
        let err = parse(json).unwrap_err();
        assert!(matches!(err, ConfigError::LayoutKeyOutOfBounds { .. }));
        assert!(err.to_string().contains("out of bounds"));
    }

    #[test]
    fn invalid_json_error() {
        let err = parse("not json").unwrap_err();
        assert!(matches!(err, ConfigError::Json(_)));
        assert!(err.to_string().contains("JSON parse error"));
    }

    #[test]
    fn hex_vid_pid_without_prefix() {
        let json = r#"{
            "usb": { "vid": "1234", "pid": "ABCD" }
        }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.usb.vid, 0x1234);
        assert_eq!(config.usb.pid, 0xABCD);
    }

    #[test]
    fn validate_skips_pin_check_when_pins_empty() {
        let json = r#"{
            "matrix_size": { "rows": 4, "cols": 6 }
        }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.matrix_size.rows, 4);
    }

    #[test]
    fn validate_skips_bounds_check_when_size_zero() {
        let json = r#"{
            "layouts": {
                "LAYOUT": {
                    "layout": [{"matrix": [99, 99], "x": 0, "y": 0}]
                }
            }
        }"#;
        parse(json).unwrap();
    }

    #[test]
    fn config_error_display_invalid_usb() {
        let err = ConfigError::InvalidUsb("bad vid".into());
        assert_eq!(err.to_string(), "invalid USB config: bad vid");
    }

    #[test]
    fn parse_processor_and_bootloader() {
        let json = r#"{
            "processor": "RP2040",
            "bootloader": "rp2040"
        }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.processor.as_deref(), Some("RP2040"));
        assert_eq!(config.bootloader.as_deref(), Some("rp2040"));
        assert_eq!(config.mcu_family(), McuFamily::Rp2040);
    }

    #[test]
    fn parse_diode_direction() {
        let json = r#"{ "diode_direction": "COL2ROW" }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.diode_direction, Some(DiodeDirection::Col2Row));

        let json = r#"{ "diode_direction": "ROW2COL" }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.diode_direction, Some(DiodeDirection::Row2Col));
    }

    #[test]
    fn parse_encoder_config() {
        let json = r#"{
            "encoder": {
                "rotary": [
                    {"pin_a": "B3", "pin_b": "B2"},
                    {"pin_a": "D1", "pin_b": "D0", "resolution": 2}
                ]
            }
        }"#;
        let config = parse(json).unwrap();
        assert!(config.has_encoders());
        assert_eq!(config.encoder.rotary.len(), 2);
        assert_eq!(config.encoder.rotary[0].pin_a, "B3");
        assert_eq!(config.encoder.rotary[0].resolution, 4);
        assert_eq!(config.encoder.rotary[1].resolution, 2);
    }

    #[test]
    fn parse_features() {
        let json = r#"{
            "features": {
                "bootmagic": true,
                "mousekey": true,
                "nkro": false
            }
        }"#;
        let config = parse(json).unwrap();
        assert!(config.has_feature("bootmagic"));
        assert!(config.has_feature("mousekey"));
        assert!(!config.has_feature("nkro"));
        assert!(!config.has_feature("nonexistent"));
    }

    #[test]
    fn parse_development_board() {
        let json = r#"{ "development_board": "promicro" }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.development_board.as_deref(), Some("promicro"));
    }

    #[test]
    fn mcu_family_detection() {
        assert_eq!(
            parse(r#"{"processor": "RP2040"}"#).unwrap().mcu_family(),
            McuFamily::Rp2040
        );
        assert_eq!(
            parse(r#"{"processor": "STM32F303"}"#).unwrap().mcu_family(),
            McuFamily::Stm32
        );
        assert_eq!(
            parse(r#"{"processor": "STM32F072"}"#).unwrap().mcu_family(),
            McuFamily::Stm32
        );
        assert_eq!(
            parse(r#"{"processor": "atmega32u4"}"#)
                .unwrap()
                .mcu_family(),
            McuFamily::Avr
        );
        assert_eq!(
            parse(r#"{"processor": "AT90USB1286"}"#)
                .unwrap()
                .mcu_family(),
            McuFamily::Avr
        );
        assert_eq!(
            parse(r#"{"processor": "nRF52840"}"#).unwrap().mcu_family(),
            McuFamily::Nrf52
        );
        assert_eq!(
            parse(r#"{"processor": "ESP32-S3"}"#).unwrap().mcu_family(),
            McuFamily::Esp32
        );
        assert_eq!(parse(r#"{}"#).unwrap().mcu_family(), McuFamily::Unknown);
    }

    #[test]
    fn matrix_dimensions_prefers_size_over_pins() {
        let json = r#"{
            "matrix_pins": { "rows": ["B0"], "cols": ["A0"] },
            "matrix_size": { "rows": 1, "cols": 1 }
        }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.matrix_dimensions(), (1, 1));
        assert_eq!(config.key_count(), 1);
    }

    #[test]
    fn matrix_dimensions_falls_back_to_pins() {
        let json = r#"{
            "matrix_pins": { "rows": ["B0", "B1"], "cols": ["A0", "A1", "A2"] }
        }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.matrix_dimensions(), (2, 3));
        assert_eq!(config.key_count(), 6);
    }

    #[test]
    fn parse_real_qmk_keyboard() {
        let json = r#"{
            "keyboard_name": "bobpad",
            "manufacturer": "nendowaka",
            "processor": "atmega32u4",
            "bootloader": "caterina",
            "diode_direction": "COL2ROW",
            "usb": { "vid": "0x7C88", "pid": "0x7C99" },
            "matrix_pins": { "rows": ["F0", "F1", "F4", "F5"], "cols": ["D6", "C7", "E6"] },
            "matrix_size": { "rows": 4, "cols": 3 },
            "encoder": { "rotary": [{"pin_a": "B3", "pin_b": "B2"}] },
            "features": { "bootmagic": true, "encoder": true, "nkro": false },
            "layouts": {
                "LAYOUT": {
                    "layout": [
                        {"matrix": [0,0], "x": 0, "y": 0},
                        {"matrix": [0,1], "x": 1, "y": 0}
                    ]
                }
            }
        }"#;
        let config = parse(json).unwrap();
        assert_eq!(config.mcu_family(), McuFamily::Avr);
        assert_eq!(config.matrix_dimensions(), (4, 3));
        assert!(config.has_encoders());
        assert!(config.has_feature("encoder"));
        assert!(!config.has_feature("nkro"));
        assert_eq!(config.diode_direction, Some(DiodeDirection::Col2Row));
    }
}
