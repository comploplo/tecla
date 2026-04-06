//! QMK `info.json` parser — extracts matrix pins, USB descriptors, and layouts.
//!
//! [`parse`] deserializes a QMK keyboard's `info.json` (JSON5-tolerant) into a
//! [`QmkConfig`] and validates that matrix pin counts match declared dimensions
//! and that layout keys fall within the matrix bounds.
//!
//! Downstream consumers:
//! - [`keymap`](crate::keymap) uses the config to resolve `#include` paths
//!   and layer dimensions during transpilation.
//! - [`via_json`](crate::via_json) generates Via-compatible JSON definitions
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
}
