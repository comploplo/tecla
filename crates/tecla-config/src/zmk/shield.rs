//! ZMK shield directory parser.
//!
//! Reads a shield directory (`.dtsi`, `.overlay`, `.keymap`, `.zmk.yml`) and
//! assembles a [`BoardConfig`](crate::board::BoardConfig) from the parsed data.

use crate::board::{BoardConfig, DiodeDirection, EncoderPins, Layout, LayoutKey};
use crate::zmk::dts::{self, DtsValue};
use serde::Deserialize;
use std::path::Path;

/// Shield type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShieldType {
    /// A traditional keyboard matrix.
    Matrix,
    /// A peripheral device (display, encoder, etc.) with no matrix.
    Peripheral,
    /// A CI/testing stub with no hardware.
    Virtual,
    /// Utility shield for settings or bootloader operations.
    Meta,
}

/// Metadata from a `.zmk.yml` file.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShieldMetadata {
    /// Shield identifier (e.g. "corne").
    #[serde(default)]
    pub id: Option<String>,
    /// Human-readable name.
    #[serde(default)]
    pub name: Option<String>,
    /// Shield type (usually "shield").
    #[serde(default, rename = "type")]
    pub shield_type: Option<String>,
    /// Project URL.
    #[serde(default)]
    pub url: Option<String>,
    /// Required board connectors (e.g. ["pro_micro"]).
    #[serde(default)]
    pub requires: Vec<String>,
    /// Exposed connectors (e.g. ["i2c_oled"]).
    #[serde(default)]
    pub exposes: Vec<String>,
    /// Feature tags (e.g. ["keys", "display", "encoder"]).
    #[serde(default)]
    pub features: Vec<String>,
    /// Split keyboard sibling shields (e.g. ["corne_left", "corne_right"]).
    #[serde(default)]
    pub siblings: Vec<String>,
}

/// Parse a `.zmk.yml` metadata file.
pub fn parse_metadata(yaml: &str) -> Result<ShieldMetadata, String> {
    serde_yml::from_str(yaml).map_err(|e| e.to_string())
}

/// Errors from parsing a ZMK shield directory.
#[derive(Debug)]
pub enum ShieldError {
    /// Failed to read a file.
    Io(std::io::Error),
    /// No `.dtsi` or `.overlay` file found.
    NoDtsSource,
    /// YAML metadata parse error.
    Yaml(String),
}

impl std::fmt::Display for ShieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::NoDtsSource => write!(f, "no .dtsi or .overlay file found in shield directory"),
            Self::Yaml(e) => write!(f, "YAML parse error: {e}"),
        }
    }
}

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_while};
use nom::character::complete::char;
use nom::multi::many0;
use nom::sequence::delimited;
use nom::IResult;
use nom::Parser;
use std::convert::TryFrom;

/// A chunk of DTS source: either a local include directive or verbatim text.
enum DtsChunk<'a> {
    Include(&'a str),
    Text(&'a str),
}

fn local_include(input: &str) -> IResult<&str, DtsChunk<'_>> {
    let (input, _) = take_while(|c: char| c == ' ' || c == '\t').parse(input)?;
    let (input, _) = tag("#include").parse(input)?;
    let (input, _) = take_while(|c: char| c == ' ' || c == '\t').parse(input)?;
    let (input, filename) = delimited(char('"'), take_until("\""), char('"')).parse(input)?;
    let (input, _) = take_until("\n").parse(input)?;
    let (input, _) = char('\n').parse(input)?;
    Ok((input, DtsChunk::Include(filename)))
}

fn text_line(input: &str) -> IResult<&str, DtsChunk<'_>> {
    let (rest, line) = take_until("\n").parse(input)?;
    let (rest, _) = char('\n').parse(rest)?;
    Ok((rest, DtsChunk::Text(line)))
}

fn trailing_text(input: &str) -> IResult<&str, DtsChunk<'_>> {
    if input.is_empty() {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }
    Ok(("", DtsChunk::Text(input)))
}

fn dts_chunks(input: &str) -> IResult<&str, Vec<DtsChunk<'_>>> {
    many0(alt((local_include, text_line, trailing_text))).parse(input)
}

fn resolve_local_includes(source: &str, dir: &Path) -> String {
    let chunks = match dts_chunks(source) {
        Ok((_, chunks)) => chunks,
        Err(_) => return source.to_owned(),
    };
    let mut result = String::with_capacity(source.len());
    for chunk in chunks {
        match chunk {
            DtsChunk::Include(filename) => {
                let include_path = dir.join(filename);
                if let Ok(content) = std::fs::read_to_string(&include_path) {
                    let resolved = resolve_local_includes(&content, dir);
                    result.push_str(&resolved);
                    result.push('\n');
                }
            }
            DtsChunk::Text(text) => {
                result.push_str(text);
                result.push('\n');
            }
        }
    }
    result
}

fn collect_shield_source(paths: &[&Path]) -> Result<String, ShieldError> {
    let mut raw_source = String::new();
    for path in paths {
        if path.exists() {
            raw_source.push_str(&std::fs::read_to_string(path).map_err(ShieldError::Io)?);
            raw_source.push('\n');
        }
    }
    Ok(raw_source)
}

fn shield_metadata(
    yml_path: &Path,
    shield_name: &str,
) -> Result<(Option<ShieldMetadata>, ShieldType), ShieldError> {
    if !yml_path.exists() {
        return Ok((None, ShieldType::Matrix));
    }

    let yml = std::fs::read_to_string(yml_path).map_err(ShieldError::Io)?;
    let meta = parse_metadata(&yml).map_err(ShieldError::Yaml)?;
    let shield_type = if meta.features.contains(&"display".to_owned())
        && !meta.features.contains(&"keys".to_owned())
    {
        ShieldType::Peripheral
    } else if shield_name.contains("posix") || shield_name.contains("tester") {
        ShieldType::Virtual
    } else {
        ShieldType::Matrix
    };
    Ok((Some(meta), shield_type))
}

fn pin_refs_to_strings(refs: &[dts::PhandleRef]) -> Vec<String> {
    refs.iter()
        .map(|r| format!("&{} {}", r.label, r.args.first().copied().unwrap_or(0)))
        .collect()
}

fn matrix_size_from_value(value: &DtsValue) -> Option<usize> {
    if let DtsValue::Cells(v) = value {
        v.first().and_then(|cols| usize::try_from(*cols).ok())
    } else {
        None
    }
}

fn apply_layout_from_map(
    config: &mut BoardConfig,
    metadata: Option<&ShieldMetadata>,
    shield_name: &str,
    map: &[(u32, u32)],
) {
    let layout_name = metadata
        .and_then(|m| m.id.clone())
        .unwrap_or_else(|| shield_name.to_owned());
    let cols = config.matrix_size.1.max(1);
    let keys = map
        .iter()
        .enumerate()
        .filter_map(|(i, &(row, col))| {
            let row = usize::try_from(row).ok()?;
            let col = usize::try_from(col).ok()?;
            let x = u32::try_from(i % cols).ok().map(f64::from).unwrap_or(0.0);
            let y = u32::try_from(i / cols).ok().map(f64::from).unwrap_or(0.0);
            Some(LayoutKey {
                row,
                col,
                x,
                y,
                w: 1.0,
            })
        })
        .collect();
    config
        .layouts
        .insert(format!("LAYOUT_{layout_name}"), Layout { keys });
}

fn apply_matrix_section(
    config: &mut BoardConfig,
    doc: &dts::DtsDocument,
    metadata: Option<&ShieldMetadata>,
    shield_name: &str,
) {
    if let Some(prop) = doc.property("columns") {
        if let Some(cols) = matrix_size_from_value(&prop.value) {
            config.matrix_size.1 = cols;
        }
    }
    if let Some(prop) = doc.property("rows") {
        if let Some(rows) = matrix_size_from_value(&prop.value) {
            config.matrix_size.0 = rows;
        }
    }

    if let Some(prop) = doc.property("diode-direction") {
        if let DtsValue::Str(ref s) = prop.value {
            config.diode_direction = match s.as_str() {
                "col2row" => DiodeDirection::Col2Row,
                "row2col" => DiodeDirection::Row2Col,
                _ => DiodeDirection::Col2Row,
            };
        }
    }

    if let Some(prop) = doc.gpio_property("row-gpios") {
        if let DtsValue::PhandleList(refs) = &prop.value {
            config.row_pins = pin_refs_to_strings(refs);
        }
    }

    if let Some(prop) = doc.gpio_property("col-gpios") {
        if let DtsValue::PhandleList(refs) = &prop.value {
            config.col_pins = pin_refs_to_strings(refs);
        }
    }

    if config.row_pins.is_empty() && config.col_pins.is_empty() {
        if let Some(prop) = doc.gpio_property("input-gpios") {
            if let DtsValue::PhandleList(refs) = &prop.value {
                config.col_pins = pin_refs_to_strings(refs);
                config.matrix_size = (1, refs.len());
            }
        }
    }

    if config.matrix_size == (0, 0) && !config.row_pins.is_empty() && !config.col_pins.is_empty() {
        config.matrix_size = (config.row_pins.len(), config.col_pins.len());
    }

    if let Some(map) = doc.rc_map() {
        let map_rows = map
            .iter()
            .filter_map(|&(r, _)| usize::try_from(r).ok())
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);
        let map_cols = map
            .iter()
            .filter_map(|&(_, c)| usize::try_from(c).ok())
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);
        if map_rows > config.matrix_size.0 {
            config.matrix_size.0 = map_rows;
        }
        if map_cols > config.matrix_size.1 {
            config.matrix_size.1 = map_cols;
        }
        apply_layout_from_map(config, metadata, shield_name, map);
    }
}

fn apply_encoder_section(config: &mut BoardConfig, doc: &dts::DtsDocument) {
    for enc in doc.encoders() {
        let resolution = u8::try_from(enc.steps / 4).unwrap_or(u8::MAX);
        config.encoders.push(EncoderPins {
            pin_a: format!(
                "&{} {}",
                enc.a_pin.label,
                enc.a_pin.args.first().copied().unwrap_or(0)
            ),
            pin_b: format!(
                "&{} {}",
                enc.b_pin.label,
                enc.b_pin.args.first().copied().unwrap_or(0)
            ),
            resolution,
        });
    }
}

/// Parse a ZMK shield directory into a [`BoardConfig`].
///
/// Reads the `.dtsi` (or `.overlay` for single-board), `_left.overlay`,
/// `_right.overlay`, `.keymap`, and `.zmk.yml` files from the shield directory.
pub fn parse_shield(dir: &Path) -> Result<BoardConfig, ShieldError> {
    let shield_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let dtsi_path = dir.join(format!("{shield_name}.dtsi"));
    let overlay_path = dir.join(format!("{shield_name}.overlay"));
    let left_path = dir.join(format!("{shield_name}_left.overlay"));
    let _right_path = dir.join(format!("{shield_name}_right.overlay"));
    let keymap_path = dir.join(format!("{shield_name}.keymap"));
    let yml_path = dir.join(format!("{shield_name}.zmk.yml"));

    let raw_source = collect_shield_source(&[&dtsi_path, &overlay_path, &left_path])?;
    let (metadata, shield_type) = shield_metadata(&yml_path, shield_name)?;

    if raw_source.is_empty() {
        return Err(ShieldError::NoDtsSource);
    }

    let merged = resolve_local_includes(&raw_source, dir);
    let doc = dts::parse_document(&merged);

    let mut config = BoardConfig::default();

    if let Some(ref meta) = metadata {
        config.name = meta.name.clone();
        for feat in &meta.features {
            config.features.insert(feat.clone(), true);
        }
    }

    if shield_type == ShieldType::Matrix {
        apply_matrix_section(&mut config, &doc, metadata.as_ref(), shield_name);
    }

    apply_encoder_section(&mut config, &doc);

    if keymap_path.exists() {
        let keymap_source = std::fs::read_to_string(&keymap_path).map_err(ShieldError::Io)?;
        let layers = dts::extract_bindings(&keymap_source);
        if !layers.is_empty() {
            config.zmk_keymap = Some(layers);
        }
    }

    Ok(config)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_shield_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tecla-shield-{name}-{nonce}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_zmk_yml() {
        let yml = r#"
file_format: "1"
id: corne
name: Corne
type: shield
url: https://github.com/foostan/crkbd/
requires: [pro_micro]
exposes: [i2c_oled]
features:
  - keys
  - display
  - underglow
  - studio
siblings:
  - corne_left
  - corne_right
"#;
        let meta = parse_metadata(yml).unwrap();
        assert_eq!(meta.id.as_deref(), Some("corne"));
        assert_eq!(meta.name.as_deref(), Some("Corne"));
        assert_eq!(meta.requires, vec!["pro_micro"]);
        assert_eq!(meta.siblings, vec!["corne_left", "corne_right"]);
        assert!(meta.features.contains(&"keys".to_owned()));
        assert!(meta.features.contains(&"display".to_owned()));
    }

    #[test]
    fn parse_real_corne_shield() {
        let shield_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/zmk_shields/corne");
        assert!(
            shield_dir.exists(),
            "shield directory not found at {shield_dir:?} — run `git submodule update --init`"
        );
        let config = parse_shield(&shield_dir).unwrap();
        assert_eq!(config.name.as_deref(), Some("Corne"));
        assert_eq!(config.matrix_size, (4, 12));
        assert_eq!(config.diode_direction, DiodeDirection::Col2Row);
        assert!(!config.row_pins.is_empty());
        assert!(!config.col_pins.is_empty());
        assert!(!config.layouts.is_empty());
    }

    #[test]
    fn parse_real_sofle_shield() {
        let shield_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/zmk_shields/sofle");
        assert!(
            shield_dir.exists(),
            "shield directory not found at {shield_dir:?} — run `git submodule update --init`"
        );
        let config = parse_shield(&shield_dir).unwrap();
        assert_eq!(config.name.as_deref(), Some("Sofle"));
        assert_eq!(config.matrix_size, (5, 16));
        assert!(config.has_encoders());
        assert!(config.has_feature("encoder"));
    }

    #[test]
    fn parse_real_bat43_shield() {
        let shield_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/zmk_shields/bat43");
        assert!(
            shield_dir.exists(),
            "shield directory not found at {shield_dir:?} — run `git submodule update --init`"
        );
        let config = parse_shield(&shield_dir).unwrap();
        assert_eq!(config.name.as_deref(), Some("BAT43"));
        assert!(!config.has_encoders());
    }

    #[test]
    fn parse_overlay_only_shield_source() {
        let dir = temp_shield_dir("overlay-only");
        let shield_name = dir.file_name().and_then(|n| n.to_str()).unwrap();
        let overlay_path = dir.join(format!("{shield_name}.overlay"));
        std::fs::write(
            &overlay_path,
            r#"
/ {
    rows = <1>;
    columns = <2>;
    row-gpios = <&gpio0 1 0>;
    col-gpios = <&gpio0 2 0>, <&gpio0 3 0>;
    kscan0: kscan {
        map = <RC(0,0) RC(0,1)>;
    };
};
"#,
        )
        .unwrap();
        let config = parse_shield(&dir).unwrap();
        assert_eq!(config.matrix_size, (1, 2));
        assert_eq!(config.row_pins.len(), 1);
        assert_eq!(config.col_pins.len(), 2);
    }
}
