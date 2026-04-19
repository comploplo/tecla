//! Devicetree source parser for ZMK shield and board files.
//!
//! Parses the subset of Devicetree syntax used by ZMK keyboard definitions:
//! property assignments, node blocks, angle-bracket cell lists, and the
//! `RC(row,col)` matrix transform macro.
//!
//! All parsing is done with nom. No string matching (`find`, `contains`,
//! `starts_with`, `rfind`, `split`, `trim`) is used for structural parsing.

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_while1};
use nom::character::complete::{char, multispace1, u32 as parse_u32};
use nom::combinator::{map, opt, value};
use nom::multi::{many0, separated_list0};
use nom::sequence::{delimited, preceded, separated_pair};
use nom::{IResult, Parser};

// ── Value types ──────────────────────────────────────────────────────────────

/// A parsed devicetree property value.
#[derive(Debug, Clone, PartialEq)]
pub enum DtsValue {
    /// Integer cell list: `<1 2 3>`
    Cells(Vec<u32>),
    /// String: `"col2row"`
    Str(String),
    /// Phandle reference list: `<&gpio0 22 0>, <&gpio0 24 0>`
    PhandleList(Vec<PhandleRef>),
    /// RC macro list from matrix transforms: `RC(0,0) RC(0,1)`
    RcList(Vec<(u32, u32)>),
    /// Binding list from keymaps: `&kp TAB &mo 1`
    BindingList(Vec<Binding>),
}

/// A phandle reference with arguments: `&label arg0 arg1 ...`
#[derive(Debug, Clone, PartialEq)]
pub struct PhandleRef {
    /// Label name (without `&`).
    pub label: String,
    /// Numeric arguments following the label.
    pub args: Vec<u32>,
}

/// A ZMK keymap binding: `&behavior arg0 arg1 ...`
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// Behavior name (without `&`).
    pub behavior: String,
    /// Arguments (key names or numbers).
    pub args: Vec<String>,
}

/// A parsed devicetree property.
#[derive(Debug, Clone, PartialEq)]
pub struct DtsProperty {
    /// Property name.
    pub name: String,
    /// Property value.
    pub value: DtsValue,
}

/// A rotary encoder parsed from an `alps,ec11` devicetree node.
#[derive(Debug, Clone, PartialEq)]
pub struct DtsEncoder {
    /// Label (e.g. `encoder_left`).
    pub label: Option<String>,
    /// Phase A GPIO reference.
    pub a_pin: PhandleRef,
    /// Phase B GPIO reference.
    pub b_pin: PhandleRef,
    /// Steps per full rotation.
    pub steps: u32,
}

/// A named DTS node with its properties.
#[derive(Debug, Clone)]
pub struct DtsNode {
    /// Label before the colon (e.g. `kscan0` in `kscan0: kscan`).
    pub label: Option<String>,
    /// Node name after the colon.
    pub name: String,
    /// Properties parsed from the body.
    pub properties: Vec<DtsProperty>,
    /// GPIO properties parsed from the body.
    pub gpio_properties: Vec<DtsProperty>,
    /// Raw body text for further parsing.
    pub body: String,
}

/// An override block: `&label { body };`
#[derive(Debug, Clone)]
pub struct DtsOverride {
    /// Label being overridden.
    pub label: String,
    /// Properties in the override.
    pub properties: Vec<DtsProperty>,
    /// GPIO properties in the override.
    pub gpio_properties: Vec<DtsProperty>,
    /// Raw body text.
    pub body: String,
}

/// Top-level parsed DTS document.
#[derive(Debug, Clone)]
pub struct DtsDocument {
    /// Named nodes found in the source.
    pub nodes: Vec<DtsNode>,
    /// Override blocks found in the source.
    pub overrides: Vec<DtsOverride>,
    /// All properties found at any nesting level.
    pub all_properties: Vec<DtsProperty>,
    /// All GPIO properties found at any nesting level.
    pub all_gpio_properties: Vec<DtsProperty>,
}

// ── Primitives ───────────────────────────────────────────────────────────────

fn line_comment(input: &str) -> IResult<&str, ()> {
    (tag("//"), take_until("\n")).map(|_| ()).parse(input)
}

fn block_comment(input: &str) -> IResult<&str, ()> {
    (tag("/*"), take_until("*/"), tag("*/"))
        .map(|_| ())
        .parse(input)
}

fn ws(input: &str) -> IResult<&str, ()> {
    many0(alt((value((), multispace1), line_comment, block_comment)))
        .map(|_| ())
        .parse(input)
}

fn identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-' || c == '#' || c == ',')
        .parse(input)
}

fn quoted_string(input: &str) -> IResult<&str, &str> {
    delimited(char('"'), take_until("\""), char('"')).parse(input)
}

fn rc_macro(input: &str) -> IResult<&str, (u32, u32)> {
    delimited(
        tag("RC("),
        separated_pair(parse_u32, preceded(ws, char(',')), preceded(ws, parse_u32)),
        char(')'),
    )
    .parse(input)
}

fn phandle_ref(input: &str) -> IResult<&str, PhandleRef> {
    let (input, _) = char('&').parse(input)?;
    let (input, label) = identifier(input)?;
    let (input, args) = many0(preceded(multispace1, parse_u32)).parse(input)?;
    Ok((
        input,
        PhandleRef {
            label: label.to_owned(),
            args,
        },
    ))
}

fn binding(input: &str) -> IResult<&str, Binding> {
    let (input, _) = char('&').parse(input)?;
    let (input, behavior) = identifier(input)?;
    let (input, args) = many0(preceded(
        multispace1,
        take_while1(|c: char| c.is_alphanumeric() || c == '_'),
    ))
    .parse(input)?;
    Ok((
        input,
        Binding {
            behavior: behavior.to_owned(),
            args: args.iter().map(|s| (*s).to_owned()).collect(),
        },
    ))
}

// ── Property parsers ─────────────────────────────────────────────────────────

fn cell_list(input: &str) -> IResult<&str, DtsValue> {
    map(
        separated_list0(preceded(ws, opt(char(','))), preceded(ws, parse_u32)),
        DtsValue::Cells,
    )
    .parse(input)
}

/// Parse `name = <integer>;` or `name = "string";`
pub fn property(input: &str) -> IResult<&str, DtsProperty> {
    let (input, _) = ws(input)?;
    let (input, name) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, val) = alt((
        map(quoted_string, |s| DtsValue::Str(s.to_owned())),
        delimited(char('<'), preceded(ws, cell_list), preceded(ws, char('>'))),
    ))
    .parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(';').parse(input)?;
    Ok((
        input,
        DtsProperty {
            name: name.to_owned(),
            value: val,
        },
    ))
}

fn gpio_cell(input: &str) -> IResult<&str, PhandleRef> {
    let (input, _) = ws(input)?;
    let (input, _) = char('<').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, pref) = phandle_ref(input)?;
    let (input, _) = take_until(">").parse(input)?;
    let (input, _) = char('>').parse(input)?;
    Ok((input, pref))
}

/// Parse a multi-line GPIO property: `name = <&ref flags>, <&ref flags>, ... ;`
pub fn gpio_property(input: &str) -> IResult<&str, DtsProperty> {
    let (input, _) = ws(input)?;
    let (input, name) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, refs) = separated_list0(preceded(ws, char(',')), gpio_cell).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(';').parse(input)?;
    Ok((
        input,
        DtsProperty {
            name: name.to_owned(),
            value: DtsValue::PhandleList(refs),
        },
    ))
}

/// Parse a matrix transform `map` property with `RC(row,col)` macros.
pub fn map_property(input: &str) -> IResult<&str, DtsProperty> {
    let (input, _) = ws(input)?;
    let (input, _) = tag("map").parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('<').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, items) = separated_list0(ws, rc_macro).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('>').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(';').parse(input)?;
    Ok((
        input,
        DtsProperty {
            name: "map".to_owned(),
            value: DtsValue::RcList(items),
        },
    ))
}

/// Parse a keymap `bindings` property.
pub fn bindings_property(input: &str) -> IResult<&str, DtsProperty> {
    let (input, _) = ws(input)?;
    let (input, _) = tag("bindings").parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('=').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('<').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, items) = separated_list0(ws, binding).parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('>').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(';').parse(input)?;
    Ok((
        input,
        DtsProperty {
            name: "bindings".to_owned(),
            value: DtsValue::BindingList(items),
        },
    ))
}

// ── Document parser ──────────────────────────────────────────────────────────

/// A top-level item in a DTS source: a property, a node, an override, or
/// a single character we skip over (preprocessor directives, braces, etc.).
enum DtsItem {
    Prop(DtsProperty),
    GpioProp(DtsProperty),
    MapProp(DtsProperty),
    BindingsProp(DtsProperty),
    LabeledBlock {
        label: String,
        name: String,
        body: String,
    },
    Override {
        label: String,
        body: String,
    },
    Skip,
}

fn any_property(input: &str) -> IResult<&str, DtsItem> {
    alt((
        map(map_property, DtsItem::MapProp),
        map(bindings_property, DtsItem::BindingsProp),
        map(gpio_property, DtsItem::GpioProp),
        map(property, DtsItem::Prop),
    ))
    .parse(input)
}

fn labeled_block(input: &str) -> IResult<&str, DtsItem> {
    let (input, _) = ws(input)?;
    let (input, label) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(':').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, name) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{').parse(input)?;
    let (input, body) = take_until("};").parse(input)?;
    let (input, _) = tag("};").parse(input)?;
    Ok((
        input,
        DtsItem::LabeledBlock {
            label: label.to_owned(),
            name: name.to_owned(),
            body: body.to_owned(),
        },
    ))
}

fn override_block(input: &str) -> IResult<&str, DtsItem> {
    let (input, _) = ws(input)?;
    let (input, _) = char('&').parse(input)?;
    let (input, label) = identifier(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('{').parse(input)?;
    let (input, body) = take_until("};").parse(input)?;
    let (input, _) = tag("};").parse(input)?;
    Ok((
        input,
        DtsItem::Override {
            label: label.to_owned(),
            body: body.to_owned(),
        },
    ))
}

fn skip_one(input: &str) -> IResult<&str, DtsItem> {
    let (input, _) = nom::bytes::complete::take(1usize).parse(input)?;
    Ok((input, DtsItem::Skip))
}

fn parse_items(input: &str) -> IResult<&str, Vec<DtsItem>> {
    many0(alt((
        preceded(ws, any_property),
        preceded(ws, labeled_block),
        preceded(ws, override_block),
        skip_one,
    )))
    .parse(input)
}

fn extract_props_from_body(body: &str) -> (Vec<DtsProperty>, Vec<DtsProperty>) {
    let mut props = Vec::new();
    let mut gpio_props = Vec::new();
    if let Ok((_, items)) = parse_items(body) {
        for item in items {
            match item {
                DtsItem::Prop(p) | DtsItem::MapProp(p) | DtsItem::BindingsProp(p) => {
                    props.push(p);
                }
                DtsItem::GpioProp(p) => gpio_props.push(p),
                _ => {}
            }
        }
    }
    (props, gpio_props)
}

/// Parse an entire DTS source into a structured document.
pub fn parse_document(source: &str) -> DtsDocument {
    let items = parse_items(source)
        .map(|(_, items)| items)
        .unwrap_or_default();

    let mut doc = DtsDocument {
        nodes: Vec::new(),
        overrides: Vec::new(),
        all_properties: Vec::new(),
        all_gpio_properties: Vec::new(),
    };

    for item in items {
        match item {
            DtsItem::Prop(p) | DtsItem::MapProp(p) | DtsItem::BindingsProp(p) => {
                doc.all_properties.push(p);
            }
            DtsItem::GpioProp(p) => {
                doc.all_gpio_properties.push(p);
            }
            DtsItem::LabeledBlock { label, name, body } => {
                let (props, gpio_props) = extract_props_from_body(&body);
                doc.all_properties.extend(props.iter().cloned());
                doc.all_gpio_properties.extend(gpio_props.iter().cloned());
                doc.nodes.push(DtsNode {
                    label: Some(label),
                    name,
                    properties: props,
                    gpio_properties: gpio_props,
                    body,
                });
            }
            DtsItem::Override { label, body } => {
                let (props, gpio_props) = extract_props_from_body(&body);
                doc.all_properties.extend(props.iter().cloned());
                doc.all_gpio_properties.extend(gpio_props.iter().cloned());
                doc.overrides.push(DtsOverride {
                    label,
                    properties: props,
                    gpio_properties: gpio_props,
                    body,
                });
            }
            DtsItem::Skip => {}
        }
    }

    doc
}

// ── Query helpers on DtsDocument ─────────────────────────────────────────────

impl DtsDocument {
    /// Find the first property with the given name.
    pub fn property(&self, name: &str) -> Option<&DtsProperty> {
        self.all_properties.iter().find(|p| p.name == name)
    }

    /// Find the first GPIO property with the given name.
    pub fn gpio_property(&self, name: &str) -> Option<&DtsProperty> {
        self.all_gpio_properties.iter().find(|p| p.name == name)
    }

    /// Extract the RC map from a matrix transform.
    pub fn rc_map(&self) -> Option<&[(u32, u32)]> {
        self.all_properties.iter().find_map(|p| match &p.value {
            DtsValue::RcList(items) => Some(items.as_slice()),
            _ => None,
        })
    }

    /// Extract all keymap layer bindings.
    pub fn bindings(&self) -> Vec<&[Binding]> {
        let mut all_bindings = Vec::new();
        // Check top-level properties
        all_bindings.extend(self.all_properties.iter().filter_map(|p| match &p.value {
            DtsValue::BindingList(items) => Some(items.as_slice()),
            _ => None,
        }));
        // Check nodes (e.g. layers in keymap)
        for node in &self.nodes {
            all_bindings.extend(node.properties.iter().filter_map(|p| match &p.value {
                DtsValue::BindingList(items) => Some(items.as_slice()),
                _ => None,
            }));
        }
        all_bindings
    }

    /// Extract all encoder nodes, merging override blocks for pin assignments.
    pub fn encoders(&self) -> Vec<DtsEncoder> {
        let mut result = Vec::new();
        for node in &self.nodes {
            let is_encoder = node.properties.iter().any(|p| {
                p.name == "compatible" && p.value == DtsValue::Str("alps,ec11".to_owned())
            });
            if !is_encoder {
                continue;
            }

            let mut all_props: Vec<&DtsProperty> = node.properties.iter().collect();
            let mut all_gpio: Vec<&DtsProperty> = node.gpio_properties.iter().collect();

            if let Some(ref label) = node.label {
                for ovr in &self.overrides {
                    if ovr.label == *label {
                        all_props.extend(ovr.properties.iter());
                        all_gpio.extend(ovr.gpio_properties.iter());
                    }
                }
            }

            let a_pin = all_gpio.iter().find(|p| p.name == "a-gpios");
            let b_pin = all_gpio.iter().find(|p| p.name == "b-gpios");
            let steps = all_props
                .iter()
                .find(|p| p.name == "steps")
                .and_then(|p| match &p.value {
                    DtsValue::Cells(v) => v.first().copied(),
                    _ => None,
                })
                .unwrap_or(80);

            if let (Some(a), Some(b)) = (a_pin, b_pin) {
                let a_ref = match &a.value {
                    DtsValue::PhandleList(v) if !v.is_empty() => v[0].clone(),
                    _ => continue,
                };
                let b_ref = match &b.value {
                    DtsValue::PhandleList(v) if !v.is_empty() => v[0].clone(),
                    _ => continue,
                };
                result.push(DtsEncoder {
                    label: node.label.clone(),
                    a_pin: a_ref,
                    b_pin: b_ref,
                    steps,
                });
            }
        }
        result
    }

    /// Extract the gpio-map pin translation table.
    pub fn gpio_map(&self) -> Vec<(u32, String, u32)> {
        for node in &self.nodes {
            if let Ok((_, entries)) = gpio_map_entries(&node.body) {
                if !entries.is_empty() {
                    return entries;
                }
            }
        }
        Vec::new()
    }

    /// Extract triggers-per-rotation from a sensor node.
    pub fn triggers_per_rotation(&self) -> Option<u32> {
        self.property("triggers-per-rotation")
            .and_then(|p| match &p.value {
                DtsValue::Cells(v) => v.first().copied(),
                _ => None,
            })
    }
}

fn gpio_map_entry(input: &str) -> IResult<&str, (u32, String, u32)> {
    let (input, _) = ws(input)?;
    let (input, _) = char('<').parse(input)?;
    let (input, _) = ws(input)?;
    let (input, pin) = parse_u32.parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _flags) = parse_u32.parse(input)?;
    let (input, _) = ws(input)?;
    let (input, pref) = phandle_ref(input)?;
    let (input, _) = take_until(">").parse(input)?;
    let (input, _) = char('>').parse(input)?;
    Ok((
        input,
        (pin, pref.label, pref.args.first().copied().unwrap_or(0)),
    ))
}

fn gpio_map_entries(input: &str) -> IResult<&str, Vec<(u32, String, u32)>> {
    let (input, _) = take_until("gpio-map").parse(input)?;
    let (input, _) = tag("gpio-map").parse(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = opt(char('=')).parse(input)?;
    let (input, _) = ws(input)?;
    separated_list0(preceded(ws, opt(char(','))), gpio_map_entry).parse(input)
}

// ── Legacy extraction API (delegates to DtsDocument) ─────────────────────────

/// Extract a property by name from DTS source.
pub fn extract_property(source: &str, name: &str) -> Option<DtsProperty> {
    parse_document(source).property(name).cloned()
}

/// Extract a GPIO property from DTS source.
pub fn extract_gpio_property(source: &str, name: &str) -> Option<DtsProperty> {
    parse_document(source).gpio_property(name).cloned()
}

/// Extract `RC(row,col)` entries from a matrix transform map.
pub fn extract_map(source: &str) -> Option<Vec<(u32, u32)>> {
    parse_document(source).rc_map().map(|s| s.to_vec())
}

/// Extract all keymap layer bindings from a `.keymap` file.
pub fn extract_bindings(source: &str) -> Vec<Vec<Binding>> {
    parse_document(source)
        .bindings()
        .into_iter()
        .map(|s| s.to_vec())
        .collect()
}

/// Extract all encoders from DTS source.
pub fn extract_encoders(source: &str) -> Vec<DtsEncoder> {
    parse_document(source).encoders()
}

/// Extract the gpio-map pin translation table.
pub fn extract_gpio_map(source: &str) -> Vec<(u32, String, u32)> {
    parse_document(source).gpio_map()
}

/// Extract triggers-per-rotation from a sensor node.
pub fn extract_triggers_per_rotation(source: &str) -> Option<u32> {
    parse_document(source).triggers_per_rotation()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn parse_integer_property() {
        let (_, prop) = property("columns = <12>;").unwrap();
        assert_eq!(prop.name, "columns");
        assert_eq!(prop.value, DtsValue::Cells(vec![12]));
    }

    #[test]
    fn parse_string_property() {
        let (_, prop) = property(r#"diode-direction = "col2row";"#).unwrap();
        assert_eq!(prop.name, "diode-direction");
        assert_eq!(prop.value, DtsValue::Str("col2row".into()));
    }

    #[test]
    fn parse_rc_map() {
        let input = "map = <\nRC(0,0) RC(0,1)\nRC(1,0) RC(1,1)\n>;";
        let (_, prop) = map_property(input).unwrap();
        assert_eq!(
            prop.value,
            DtsValue::RcList(vec![(0, 0), (0, 1), (1, 0), (1, 1)])
        );
    }

    #[test]
    fn parse_gpio_refs() {
        let input = "row-gpios\n\
            = <&pro_micro 4 (GPIO_ACTIVE_HIGH | GPIO_PULL_DOWN)>\n\
            , <&pro_micro 5 (GPIO_ACTIVE_HIGH | GPIO_PULL_DOWN)>\n\
            ;";
        let (_, prop) = gpio_property(input).unwrap();
        match &prop.value {
            DtsValue::PhandleList(refs) => {
                assert_eq!(refs.len(), 2);
                assert_eq!(refs[0].label, "pro_micro");
                assert_eq!(refs[0].args, vec![4]);
                assert_eq!(refs[1].args, vec![5]);
            }
            other => panic!("expected PhandleList, got {other:?}"),
        }
    }

    #[test]
    fn parse_keymap_bindings() {
        let input = "bindings = <\n&kp TAB &kp Q &kp W &mo 1 &kp SPACE\n>;";
        let (_, prop) = bindings_property(input).unwrap();
        match &prop.value {
            DtsValue::BindingList(bindings) => {
                assert_eq!(bindings.len(), 5);
                assert_eq!(bindings[0].behavior, "kp");
                assert_eq!(bindings[0].args, vec!["TAB"]);
                assert_eq!(bindings[3].behavior, "mo");
                assert_eq!(bindings[3].args, vec!["1"]);
            }
            other => panic!("expected BindingList, got {other:?}"),
        }
    }

    #[test]
    fn parse_with_comments() {
        let input = "/* matrix setup */\nrows = <4>; // 4 rows\n";
        let prop = extract_property(input, "rows").unwrap();
        assert_eq!(prop.value, DtsValue::Cells(vec![4]));
    }

    #[test]
    fn document_extracts_corne_fragment() {
        let dtsi = r#"
            default_transform: keymap_transform_0 {
                compatible = "zmk,matrix-transform";
                columns = <12>;
                rows = <4>;
                map = <
RC(0,0) RC(0,1) RC(0,2) RC(0,3) RC(0,4) RC(0,5)  RC(0,6) RC(0,7) RC(0,8) RC(0,9) RC(0,10) RC(0,11)
RC(1,0) RC(1,1) RC(1,2) RC(1,3) RC(1,4) RC(1,5)  RC(1,6) RC(1,7) RC(1,8) RC(1,9) RC(1,10) RC(1,11)
RC(2,0) RC(2,1) RC(2,2) RC(2,3) RC(2,4) RC(2,5)  RC(2,6) RC(2,7) RC(2,8) RC(2,9) RC(2,10) RC(2,11)
                        RC(3,3) RC(3,4) RC(3,5)  RC(3,6) RC(3,7) RC(3,8)
                >;
            };

            kscan0: kscan {
                compatible = "zmk,kscan-gpio-matrix";
                diode-direction = "col2row";
                row-gpios
                    = <&pro_micro 4 (GPIO_ACTIVE_HIGH | GPIO_PULL_DOWN)>
                    , <&pro_micro 5 (GPIO_ACTIVE_HIGH | GPIO_PULL_DOWN)>
                    , <&pro_micro 6 (GPIO_ACTIVE_HIGH | GPIO_PULL_DOWN)>
                    , <&pro_micro 7 (GPIO_ACTIVE_HIGH | GPIO_PULL_DOWN)>
                    ;
            };
        "#;

        let doc = parse_document(dtsi);

        let cols = doc.property("columns").unwrap();
        assert_eq!(cols.value, DtsValue::Cells(vec![12]));

        let rows = doc.property("rows").unwrap();
        assert_eq!(rows.value, DtsValue::Cells(vec![4]));

        let diode = doc.property("diode-direction").unwrap();
        assert_eq!(diode.value, DtsValue::Str("col2row".into()));

        let map = doc.rc_map().unwrap();
        assert_eq!(map.len(), 42);
        assert_eq!(map[0], (0, 0));
        assert_eq!(map[41], (3, 8));

        let row_pins = doc.gpio_property("row-gpios").unwrap();
        match &row_pins.value {
            DtsValue::PhandleList(refs) => {
                assert_eq!(refs.len(), 4);
                assert_eq!(refs[0].args, vec![4]);
                assert_eq!(refs[3].args, vec![7]);
            }
            other => panic!("expected PhandleList, got {other:?}"),
        }
    }

    #[test]
    fn document_extracts_keymap_layers() {
        let keymap = r#"
            keymap {
                compatible = "zmk,keymap";
                default_layer {
                    bindings = <
                        &kp TAB &kp Q &kp W &kp E
                    >;
                };
                lower_layer {
                    bindings = <
                        &kp N1 &kp N2 &kp N3 &trans
                    >;
                };
            };
        "#;

        let doc = parse_document(keymap);
        let layers = doc.bindings();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].len(), 4);
        assert_eq!(layers[0][0].behavior, "kp");
        assert_eq!(layers[1][3].behavior, "trans");
    }

    #[test]
    fn document_extracts_encoders_with_overrides() {
        let dts = r#"
            left_encoder: encoder_left {
                compatible = "alps,ec11";
                steps = <80>;
                status = "disabled";
            };

            &left_encoder {
                a-gpios = <&pro_micro 21 (GPIO_ACTIVE_HIGH | GPIO_PULL_UP)>;
                b-gpios = <&pro_micro 20 (GPIO_ACTIVE_HIGH | GPIO_PULL_UP)>;
                status = "okay";
            };
        "#;

        let doc = parse_document(dts);
        let encoders = doc.encoders();
        assert_eq!(encoders.len(), 1);
        assert_eq!(encoders[0].label.as_deref(), Some("left_encoder"));
        assert_eq!(encoders[0].a_pin.args, vec![21]);
        assert_eq!(encoders[0].b_pin.args, vec![20]);
        assert_eq!(encoders[0].steps, 80);
    }

    #[test]
    fn document_extracts_encoders_inline() {
        let dts = r#"
            left_encoder: encoder_left {
                compatible = "alps,ec11";
                a-gpios = <&pro_micro 21 (GPIO_ACTIVE_HIGH | GPIO_PULL_UP)>;
                b-gpios = <&pro_micro 20 (GPIO_ACTIVE_HIGH | GPIO_PULL_UP)>;
                steps = <80>;
            };

            right_encoder: encoder_right {
                compatible = "alps,ec11";
                a-gpios = <&pro_micro 20 (GPIO_ACTIVE_HIGH | GPIO_PULL_UP)>;
                b-gpios = <&pro_micro 21 (GPIO_ACTIVE_HIGH | GPIO_PULL_UP)>;
                steps = <80>;
            };
        "#;

        let encoders = extract_encoders(dts);
        assert_eq!(encoders.len(), 2);
        assert_eq!(encoders[0].a_pin.args, vec![21]);
        assert_eq!(encoders[1].a_pin.args, vec![20]);
    }

    #[test]
    fn document_extracts_gpio_map() {
        let dts = r#"
            pro_micro: connector {
                compatible = "some,connector";
                gpio-map
                    = <0  0 &gpio0 8 0>
                    , <1  0 &gpio0 6 0>
                    , <4  0 &gpio0 22 0>
                    , <5  0 &gpio0 24 0>
                    ;
            };
        "#;

        let map = extract_gpio_map(dts);
        assert_eq!(map.len(), 4);
        assert_eq!(map[0], (0, "gpio0".into(), 8));
        assert_eq!(map[2], (4, "gpio0".into(), 22));
    }

    #[test]
    fn extract_triggers_per_rotation_value() {
        let dts = r#"
            sensors: sensors {
                compatible = "zmk,keymap-sensors";
                triggers-per-rotation = <20>;
            };
        "#;
        assert_eq!(extract_triggers_per_rotation(dts), Some(20));
    }
}
