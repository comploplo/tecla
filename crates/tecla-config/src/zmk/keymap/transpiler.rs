//! ZMK binding-to-tecla transpilation.

use std::fmt::Write as _;

use super::keycodes::{zmk_key_to_tecla, zmk_mod_to_tecla};
use crate::zmk::dts::Binding;

/// Errors from ZMK keymap transpilation.
#[derive(Debug)]
pub enum TranspileError {
    /// Binding references an unrecognised behavior name.
    UnknownBehavior(String),
    /// Key name has no tecla equivalent.
    UnknownKey(String),
    /// Modifier name has no tecla equivalent.
    UnknownModifier(String),
    /// Layer argument is not a valid number.
    InvalidLayerNumber(String),
    /// Behavior requires an argument that was not provided.
    MissingArg {
        /// The behavior that was missing an argument.
        behavior: String,
    },
    /// Keymap contains no layers.
    NoLayers,
    /// Layers have different key counts.
    InconsistentKeyCount {
        /// Layer index.
        layer: usize,
        /// Expected key count.
        expected: usize,
        /// Actual key count.
        got: usize,
    },
    /// Format error writing output.
    Fmt(std::fmt::Error),
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownBehavior(b) => write!(f, "unknown behavior: &{b}"),
            Self::UnknownKey(k) => write!(f, "unknown key: {k}"),
            Self::UnknownModifier(m) => write!(f, "unknown modifier: {m}"),
            Self::InvalidLayerNumber(s) => write!(f, "invalid layer number: {s}"),
            Self::MissingArg { behavior } => write!(f, "&{behavior} missing required argument"),
            Self::NoLayers => write!(f, "no layers found"),
            Self::InconsistentKeyCount {
                layer,
                expected,
                got,
            } => write!(f, "layer {layer} has {got} keys, expected {expected}"),
            Self::Fmt(e) => write!(f, "format error: {e}"),
        }
    }
}

impl From<std::fmt::Error> for TranspileError {
    fn from(e: std::fmt::Error) -> Self {
        Self::Fmt(e)
    }
}

fn require_arg(binding: &Binding, idx: usize) -> Result<&str, TranspileError> {
    binding
        .args
        .get(idx)
        .map(|s| s.as_str())
        .ok_or_else(|| TranspileError::MissingArg {
            behavior: binding.behavior.clone(),
        })
}

fn parse_layer_num(s: &str) -> Result<u8, TranspileError> {
    s.parse()
        .map_err(|_| TranspileError::InvalidLayerNumber(s.to_owned()))
}

fn resolve_key(name: &str) -> Result<&'static str, TranspileError> {
    zmk_key_to_tecla(name).ok_or_else(|| TranspileError::UnknownKey(name.to_owned()))
}

fn resolve_mod(name: &str) -> Result<&'static str, TranspileError> {
    zmk_mod_to_tecla(name).ok_or_else(|| TranspileError::UnknownModifier(name.to_owned()))
}

/// Converts a single ZMK binding to a tecla `keymap!` token string.
pub fn binding_to_tecla(binding: &Binding) -> Result<String, TranspileError> {
    match binding.behavior.as_str() {
        "kp" => {
            let key_name = require_arg(binding, 0)?;
            if let Some(tecla) = zmk_key_to_tecla(key_name) {
                Ok(tecla.to_owned())
            } else if key_name.starts_with("0x") || key_name.starts_with("0X") {
                Ok(format!("HID({key_name})"))
            } else {
                Err(TranspileError::UnknownKey(key_name.to_owned()))
            }
        }
        "trans" => Ok("TRNS".to_owned()),
        "none" => Ok("NoOp".to_owned()),
        "mo" => {
            let layer = parse_layer_num(require_arg(binding, 0)?)?;
            Ok(format!("MO({layer})"))
        }
        "lt" => {
            let layer = parse_layer_num(require_arg(binding, 0)?)?;
            let key = resolve_key(require_arg(binding, 1)?)?;
            Ok(format!("LT!({layer}, {key})"))
        }
        "mt" => {
            let mod_name = resolve_mod(require_arg(binding, 0)?)?;
            let key = resolve_key(require_arg(binding, 1)?)?;
            Ok(format!("MT!({mod_name}, {key})"))
        }
        "tog" => {
            let layer = parse_layer_num(require_arg(binding, 0)?)?;
            Ok(format!("TG({layer})"))
        }
        "to" => {
            let layer = parse_layer_num(require_arg(binding, 0)?)?;
            Ok(format!("TO({layer})"))
        }
        "sk" => {
            let mod_name = resolve_mod(require_arg(binding, 0)?)?;
            Ok(format!("OSM({mod_name})"))
        }
        "sl" => {
            let layer = parse_layer_num(require_arg(binding, 0)?)?;
            Ok(format!("OSL({layer})"))
        }
        "bt" => {
            let cmd = require_arg(binding, 0)?;
            match cmd {
                "BT_CLR" | "BT_CLR_CMD" => Ok("BT_CLR".to_owned()),
                "BT_NXT" | "BT_NXT_CMD" => Ok("BT_NXT".to_owned()),
                "BT_PRV" | "BT_PRV_CMD" => Ok("BT_PRV".to_owned()),
                "BT_SEL" | "BT_SEL_CMD" => {
                    let idx = binding.args.get(1).map(|s| s.as_str()).unwrap_or("0");
                    Ok(format!("BT_SEL({idx})"))
                }
                "BT_CLR_ALL" | "BT_CLR_ALL_CMD" => Ok("BT_CLR_ALL".to_owned()),
                "BT_DISC" | "BT_DISC_CMD" => {
                    let idx = binding.args.get(1).map(|s| s.as_str()).unwrap_or("0");
                    Ok(format!("BT_DISC({idx})"))
                }
                other => Ok(format!("BT_UNKNOWN({other})")),
            }
        }
        "sys_reset" | "reset" => Ok("SYS_RESET".to_owned()),
        "bootloader" => Ok("BOOTLOADER".to_owned()),
        "out" => {
            let cmd = require_arg(binding, 0)?;
            Ok(format!("OUT({cmd})"))
        }
        "rgb_ug" => {
            let cmd = require_arg(binding, 0)?;
            Ok(format!("RGB({cmd})"))
        }
        "ext_power" => {
            let cmd = require_arg(binding, 0)?;
            Ok(format!("EXT_POWER({cmd})"))
        }
        "hm" | "ht" => {
            let hold_arg = require_arg(binding, 0)?;
            let key = resolve_key(require_arg(binding, 1)?)?;
            if let Some(mod_name) = zmk_mod_to_tecla(hold_arg) {
                Ok(format!("MT!({mod_name}, {key})"))
            } else if let Some(key_name) = zmk_key_to_tecla(hold_arg) {
                Ok(format!("MT!({key_name}, {key})"))
            } else {
                Err(TranspileError::UnknownModifier(hold_arg.to_owned()))
            }
        }
        "caps_word" => Ok("CAPS_WORD".to_owned()),
        "gresc" => Ok("GRESC".to_owned()),
        "inc_dec_kp" => {
            let cw = require_arg(binding, 0)?;
            let ccw = require_arg(binding, 1)?;
            let cw_tecla = zmk_key_to_tecla(cw).unwrap_or(cw);
            let ccw_tecla = zmk_key_to_tecla(ccw).unwrap_or(ccw);
            Ok(format!("ENCODER({cw_tecla}, {ccw_tecla})"))
        }
        other => Err(TranspileError::UnknownBehavior(other.to_owned())),
    }
}

/// Transpiles parsed ZMK layers into a complete tecla `keymap!` macro invocation.
pub fn transpile(layers: &[Vec<Binding>], cols: Option<usize>) -> Result<String, TranspileError> {
    if layers.is_empty() {
        return Err(TranspileError::NoLayers);
    }

    let expected = layers[0].len();
    for (i, layer) in layers.iter().enumerate().skip(1) {
        if layer.len() != expected {
            return Err(TranspileError::InconsistentKeyCount {
                layer: i,
                expected,
                got: layer.len(),
            });
        }
    }

    let c = cols.unwrap_or(0);

    let mut out = String::new();
    if let Some(rows) = expected.checked_div(c) {
        writeln!(
            out,
            "// {layer_count} layer(s), {expected} keys ({rows} rows x {c} cols)",
            layer_count = layers.len(),
        )?;
    }
    out.push_str("keymap! {\n");

    for (layer_idx, layer) in layers.iter().enumerate() {
        if c > 0 {
            out.push_str("    #[rustfmt::skip]\n");
        }
        write!(out, "    layer!(Layer{layer_idx},")?;

        #[expect(
            clippy::integer_division_remainder_used,
            reason = "grid layout arithmetic"
        )]
        for (i, binding) in layer.iter().enumerate() {
            let token = binding_to_tecla(binding)?;
            if c > 0 && i % c == 0 {
                out.push_str("\n        ");
            } else {
                out.push(' ');
            }
            out.push_str(&token);
            out.push(',');
        }

        out.push_str("\n    ),\n");
    }

    out.push_str("}\n");
    Ok(out)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    fn b(behavior: &str, args: &[&str]) -> Binding {
        Binding {
            behavior: behavior.to_owned(),
            args: args.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn basic_key() {
        assert_eq!(binding_to_tecla(&b("kp", &["TAB"])).unwrap(), "TAB");
        assert_eq!(binding_to_tecla(&b("kp", &["A"])).unwrap(), "A");
        assert_eq!(binding_to_tecla(&b("kp", &["N1"])).unwrap(), "N1");
        assert_eq!(binding_to_tecla(&b("kp", &["SPACE"])).unwrap(), "SPC");
        assert_eq!(binding_to_tecla(&b("kp", &["BSPC"])).unwrap(), "BSPC");
    }

    #[test]
    fn transparent_and_none() {
        assert_eq!(binding_to_tecla(&b("trans", &[])).unwrap(), "TRNS");
        assert_eq!(binding_to_tecla(&b("none", &[])).unwrap(), "NoOp");
    }

    #[test]
    fn layer_ops() {
        assert_eq!(binding_to_tecla(&b("mo", &["1"])).unwrap(), "MO(1)");
        assert_eq!(binding_to_tecla(&b("tog", &["2"])).unwrap(), "TG(2)");
        assert_eq!(binding_to_tecla(&b("to", &["0"])).unwrap(), "TO(0)");
    }

    #[test]
    fn layer_tap() {
        assert_eq!(
            binding_to_tecla(&b("lt", &["1", "SPACE"])).unwrap(),
            "LT!(1, SPC)"
        );
    }

    #[test]
    fn mod_tap() {
        assert_eq!(
            binding_to_tecla(&b("mt", &["LSHFT", "A"])).unwrap(),
            "MT!(LSFT, A)"
        );
    }

    #[test]
    fn sticky_keys() {
        assert_eq!(binding_to_tecla(&b("sk", &["LSHFT"])).unwrap(), "OSM(LSFT)");
        assert_eq!(binding_to_tecla(&b("sl", &["1"])).unwrap(), "OSL(1)");
    }

    #[test]
    fn bluetooth() {
        assert_eq!(
            binding_to_tecla(&b("bt", &["BT_SEL", "0"])).unwrap(),
            "BT_SEL(0)"
        );
        assert_eq!(binding_to_tecla(&b("bt", &["BT_CLR"])).unwrap(), "BT_CLR");
    }

    #[test]
    fn unknown_behavior_errors() {
        let result = binding_to_tecla(&b("foobar", &[]));
        assert!(result.is_err());
    }

    #[test]
    fn unknown_key_errors() {
        let result = binding_to_tecla(&b("kp", &["NONEXISTENT_KEY_XYZ"]));
        assert!(result.is_err());
    }

    #[test]
    fn transpile_two_layers() {
        let layers = vec![
            vec![b("kp", &["A"]), b("kp", &["B"]), b("kp", &["C"])],
            vec![b("trans", &[]), b("kp", &["N1"]), b("none", &[])],
        ];
        let output = transpile(&layers, None).unwrap();
        assert!(output.contains("keymap!"));
        assert!(output.contains("layer!(Layer0,"));
        assert!(output.contains("layer!(Layer1,"));
        assert!(output.contains("A,"));
        assert!(output.contains("TRNS,"));
    }

    #[test]
    fn inconsistent_key_count_errors() {
        let layers = vec![
            vec![b("kp", &["A"]), b("kp", &["B"])],
            vec![b("kp", &["A"])],
        ];
        let result = transpile(&layers, None);
        assert!(result.is_err());
    }
}
