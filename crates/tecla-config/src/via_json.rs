//! Via / Vial JSON definition file generator.
//!
//! [`generate_via_json`] takes a parsed [`QmkConfig`]
//! and a layout name, and produces a Via-compatible JSON string containing the
//! keyboard name, USB VID/PID, matrix dimensions, and key positions. The output
//! can be loaded directly into the [Via](https://www.caniusevia.com/) configurator.

use crate::info_json::QmkConfig;
use serde_json::{json, Value};

/// Errors from Via JSON generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViaJsonError {
    /// The requested layout name was not found in the config.
    MissingLayout(String),
    /// The config has no `keyboard_name` field.
    MissingKeyboardName,
    /// The config has no USB VID/PID.
    MissingUsb,
    /// The named layout has zero keys.
    EmptyLayout(String),
}

impl std::fmt::Display for ViaJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLayout(name) => write!(f, "layout '{name}' not found in config"),
            Self::MissingKeyboardName => write!(f, "keyboard_name is missing from config"),
            Self::MissingUsb => write!(f, "USB VID/PID is missing from config"),
            Self::EmptyLayout(name) => write!(f, "layout '{name}' has no keys"),
        }
    }
}

#[expect(clippy::missing_trait_methods, reason = "default impls are sufficient")]
impl std::error::Error for ViaJsonError {}

/// Generates a Via-compatible JSON definition from a parsed QMK config.
///
/// Reads the named layout from `config`, sorts keys into rows by physical
/// position, and emits a pretty-printed JSON string with `name`, `vendorId`,
/// `productId`, `matrix`, and `layouts.keymap` fields. The output can be
/// imported directly into the [Via](https://www.caniusevia.com/) configurator
/// or saved as a `.json` definition file.
///
/// Requires a valid `keyboard_name`, non-zero USB VID/PID, and a non-empty
/// layout with the given name.
pub fn generate_via_json(config: &QmkConfig, layout_name: &str) -> Result<String, ViaJsonError> {
    let name = config
        .keyboard_name
        .as_deref()
        .ok_or(ViaJsonError::MissingKeyboardName)?;

    if config.usb.vid == 0 && config.usb.pid == 0 {
        return Err(ViaJsonError::MissingUsb);
    }

    let layout_config = config
        .layouts
        .get(layout_name)
        .ok_or_else(|| ViaJsonError::MissingLayout(layout_name.to_owned()))?;

    if layout_config.layout.is_empty() {
        return Err(ViaJsonError::EmptyLayout(layout_name.to_owned()));
    }

    let mut keys_with_pos: Vec<(f64, f64, f64)> = layout_config
        .layout
        .iter()
        .map(|k| (k.y, k.x, k.w))
        .collect();
    keys_with_pos.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));

    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut current_y: Option<f64> = None;

    for (y, x, w) in &keys_with_pos {
        #[expect(clippy::float_arithmetic, reason = "comparing layout coordinates")]
        if current_y.is_none_or(|cy| (cy - y).abs() > f64::EPSILON) {
            rows.push(Vec::new());
            current_y = Some(*y);
        }
        let Some(row) = rows.last_mut() else { continue };
        let mut obj = serde_json::Map::new();
        obj.insert("x".to_owned(), json!(x));
        obj.insert("y".to_owned(), json!(y));
        #[expect(clippy::float_arithmetic, reason = "comparing layout width")]
        if (*w - 1.0).abs() > f64::EPSILON {
            obj.insert("w".to_owned(), json!(w));
        }
        row.push(Value::Object(obj));
    }

    let via = json!({
        "name": name,
        "vendorId": format!("0x{:04X}", config.usb.vid),
        "productId": format!("0x{:04X}", config.usb.pid),
        "matrix": {
            "rows": config.matrix_size.rows,
            "cols": config.matrix_size.cols,
        },
        "layouts": {
            "keymap": rows,
        },
    });

    serde_json::to_string_pretty(&via).map_err(|_err| ViaJsonError::MissingKeyboardName)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;
    use crate::info_json;

    const PLANCK_INFO_JSON: &str = r#"{
        "keyboard_name": "planck/rev6",
        "manufacturer": "OLKB",
        "usb": { "vid": "0x03A8", "pid": "0xA4F9" },
        "matrix_pins": {
            "rows": ["B0", "B1", "B2", "B3"],
            "cols": ["A0", "A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "A11"]
        },
        "matrix_size": { "rows": 4, "cols": 12 },
        "layouts": {
            "LAYOUT_planck_grid": {
                "layout": [
                    {"matrix": [0, 0], "x": 0, "y": 0},
                    {"matrix": [0, 1], "x": 1, "y": 0},
                    {"matrix": [0, 2], "x": 2, "y": 0},
                    {"matrix": [0, 3], "x": 3, "y": 0},
                    {"matrix": [0, 4], "x": 4, "y": 0},
                    {"matrix": [0, 5], "x": 5, "y": 0},
                    {"matrix": [0, 6], "x": 6, "y": 0},
                    {"matrix": [0, 7], "x": 7, "y": 0},
                    {"matrix": [0, 8], "x": 8, "y": 0},
                    {"matrix": [0, 9], "x": 9, "y": 0},
                    {"matrix": [0, 10], "x": 10, "y": 0},
                    {"matrix": [0, 11], "x": 11, "y": 0},
                    {"matrix": [1, 0], "x": 0, "y": 1},
                    {"matrix": [1, 1], "x": 1, "y": 1},
                    {"matrix": [1, 2], "x": 2, "y": 1},
                    {"matrix": [1, 3], "x": 3, "y": 1},
                    {"matrix": [1, 4], "x": 4, "y": 1},
                    {"matrix": [1, 5], "x": 5, "y": 1},
                    {"matrix": [1, 6], "x": 6, "y": 1},
                    {"matrix": [1, 7], "x": 7, "y": 1},
                    {"matrix": [1, 8], "x": 8, "y": 1},
                    {"matrix": [1, 9], "x": 9, "y": 1},
                    {"matrix": [1, 10], "x": 10, "y": 1},
                    {"matrix": [1, 11], "x": 11, "y": 1},
                    {"matrix": [2, 0], "x": 0, "y": 2},
                    {"matrix": [2, 1], "x": 1, "y": 2},
                    {"matrix": [2, 2], "x": 2, "y": 2},
                    {"matrix": [2, 3], "x": 3, "y": 2},
                    {"matrix": [2, 4], "x": 4, "y": 2},
                    {"matrix": [2, 5], "x": 5, "y": 2},
                    {"matrix": [2, 6], "x": 6, "y": 2},
                    {"matrix": [2, 7], "x": 7, "y": 2},
                    {"matrix": [2, 8], "x": 8, "y": 2},
                    {"matrix": [2, 9], "x": 9, "y": 2},
                    {"matrix": [2, 10], "x": 10, "y": 2},
                    {"matrix": [2, 11], "x": 11, "y": 2},
                    {"matrix": [3, 0], "x": 0, "y": 3},
                    {"matrix": [3, 1], "x": 1, "y": 3},
                    {"matrix": [3, 2], "x": 2, "y": 3},
                    {"matrix": [3, 3], "x": 3, "y": 3},
                    {"matrix": [3, 4], "x": 4, "y": 3},
                    {"matrix": [3, 5], "x": 5, "y": 3},
                    {"matrix": [3, 6], "x": 6, "y": 3},
                    {"matrix": [3, 7], "x": 7, "y": 3},
                    {"matrix": [3, 8], "x": 8, "y": 3},
                    {"matrix": [3, 9], "x": 9, "y": 3},
                    {"matrix": [3, 10], "x": 10, "y": 3},
                    {"matrix": [3, 11], "x": 11, "y": 3}
                ]
            }
        }
    }"#;

    #[test]
    fn generate_planck_via_json() {
        let config = info_json::parse(PLANCK_INFO_JSON).unwrap();
        let result = generate_via_json(&config, "LAYOUT_planck_grid").unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn missing_layout_error() {
        let config = info_json::parse(PLANCK_INFO_JSON).unwrap();
        let err = generate_via_json(&config, "NONEXISTENT").unwrap_err();
        assert!(matches!(err, ViaJsonError::MissingLayout(_)));
        assert!(err.to_string().contains("NONEXISTENT"));
    }

    #[test]
    fn missing_keyboard_name_error() {
        let config = info_json::parse(
            r#"{
            "usb": { "vid": "0x1234", "pid": "0x5678" },
            "matrix_size": { "rows": 1, "cols": 1 },
            "layouts": { "L": { "layout": [{"matrix": [0,0], "x": 0, "y": 0}] } }
        }"#,
        )
        .unwrap();
        let err = generate_via_json(&config, "L").unwrap_err();
        assert!(matches!(err, ViaJsonError::MissingKeyboardName));
    }

    #[test]
    fn missing_usb_error() {
        let config = info_json::parse(
            r#"{
            "keyboard_name": "test",
            "layouts": { "L": { "layout": [{"matrix": [0,0], "x": 0, "y": 0}] } }
        }"#,
        )
        .unwrap();
        let err = generate_via_json(&config, "L").unwrap_err();
        assert!(matches!(err, ViaJsonError::MissingUsb));
    }

    #[test]
    fn empty_layout_error() {
        let config = info_json::parse(
            r#"{
            "keyboard_name": "test",
            "usb": { "vid": "0x1234", "pid": "0x5678" },
            "layouts": { "L": { "layout": [] } }
        }"#,
        )
        .unwrap();
        let err = generate_via_json(&config, "L").unwrap_err();
        assert!(matches!(err, ViaJsonError::EmptyLayout(_)));
    }

    #[test]
    fn wide_key_includes_width() {
        let config = info_json::parse(
            r#"{
            "keyboard_name": "test",
            "usb": { "vid": "0x1234", "pid": "0x5678" },
            "matrix_size": { "rows": 1, "cols": 2 },
            "layouts": {
                "L": {
                    "layout": [
                        {"matrix": [0, 0], "x": 0, "y": 0, "w": 2},
                        {"matrix": [0, 1], "x": 2, "y": 0}
                    ]
                }
            }
        }"#,
        )
        .unwrap();
        let result = generate_via_json(&config, "L").unwrap();
        assert!(result.contains("\"w\""));
    }
}
