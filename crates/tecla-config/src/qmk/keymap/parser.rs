use std::collections::HashMap;
use tree_sitter::{Node, Parser};

use super::includes::{derive_keyboard_root, resolve_includes, QmkDefines};
use super::{classify, KeymapError, ParsedLayer, ResolvedKeycode};

type Defines = HashMap<String, String>;
type FnMacros = HashMap<String, (Vec<String>, String)>;

pub(crate) struct ParseCtx {
    qmk: QmkDefines,
}

fn resolve_to_u8(qmk: &QmkDefines, text: &str) -> Option<u8> {
    qmk.resolve_value(text)
        .and_then(|v| u8::try_from(v & 0xFF).ok())
}

pub(crate) fn parse_keycode_node(node: Node<'_>, source: &str, ctx: &ParseCtx) -> ResolvedKeycode {
    match node.kind() {
        "identifier" => {
            let text = node_text(node, source);
            if let Some(n) = ctx.qmk.resolve_value(text) {
                return classify(n);
            }
            if let Some(expanded) = ctx.qmk.defines().get(text) {
                return reparse_expanded(expanded, ctx);
            }
            ResolvedKeycode::Unresolved {
                raw: text.to_owned(),
                reason: "unresolved identifier".into(),
            }
        }
        "call_expression" => parse_call(node, source, ctx),
        "parenthesized_expression" => {
            if let Some(inner) = node.named_child(0) {
                return parse_keycode_node(inner, source, ctx);
            }
            ResolvedKeycode::Unresolved {
                raw: node_text(node, source).to_owned(),
                reason: "empty parenthesized expression".into(),
            }
        }
        "binary_expression" => {
            let text = node_text(node, source);
            if let Some(n) = ctx.qmk.resolve_value(text) {
                return classify(n);
            }
            ResolvedKeycode::Unresolved {
                raw: text.to_owned(),
                reason: "unresolved binary expression".into(),
            }
        }
        "number_literal" => {
            let text = node_text(node, source);
            if let Some(n) = ctx.qmk.resolve_value(text) {
                classify(n)
            } else {
                ResolvedKeycode::Unresolved {
                    raw: text.to_owned(),
                    reason: "unparseable number literal".into(),
                }
            }
        }
        "initializer_list" => {
            let mut cursor = node.walk();
            let children: Vec<_> = node
                .named_children(&mut cursor)
                .filter(|c| c.kind() != "comment")
                .collect();
            if let [only] = children.as_slice() {
                return parse_keycode_node(*only, source, ctx);
            }
            ResolvedKeycode::Unresolved {
                raw: node_text(node, source).to_owned(),
                reason: "multi-element initializer list".into(),
            }
        }
        _ => ResolvedKeycode::Unresolved {
            raw: node_text(node, source).to_owned(),
            reason: format!("unexpected node kind: {}", node.kind()),
        },
    }
}

fn reparse_expanded(text: &str, ctx: &ParseCtx) -> ResolvedKeycode {
    let wrapper = format!("int x = {text};");
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .is_err()
    {
        return ResolvedKeycode::Unresolved {
            raw: text.to_owned(),
            reason: "failed to init parser".into(),
        };
    }
    let Some(tree) = parser.parse(&wrapper, None) else {
        return ResolvedKeycode::Unresolved {
            raw: text.to_owned(),
            reason: "failed to parse expanded".into(),
        };
    };
    let root = tree.root_node();
    if let Some(init) = find_child_by_kind(root, "init_declarator") {
        if let Some(value) = init.child_by_field_name("value") {
            return parse_keycode_node(value, &wrapper, ctx);
        }
    }
    ResolvedKeycode::Unresolved {
        raw: text.to_owned(),
        reason: "could not find value in expanded".into(),
    }
}

fn parse_call(node: Node<'_>, source: &str, ctx: &ParseCtx) -> ResolvedKeycode {
    let func_name = node
        .child_by_field_name("function")
        .map_or("", |f| node_text(f, source));
    let args: Vec<Node<'_>> = node
        .child_by_field_name("arguments")
        .map(|a| {
            let mut cursor = a.walk();
            a.named_children(&mut cursor)
                .filter(|c| c.kind() != "comment")
                .collect()
        })
        .unwrap_or_default();

    let arg_texts: Vec<String> = args
        .iter()
        .map(|a| node_text(*a, source).to_owned())
        .collect();
    let arg_strs: Vec<&str> = arg_texts.iter().map(|s| s.as_str()).collect();

    if let Some(n) = ctx.qmk.expand_and_resolve(func_name, &arg_strs) {
        return classify(n);
    }

    if let Some((params, body)) = ctx.qmk.fn_macros().get(func_name) {
        if arg_texts.len() == params.len() {
            let mut expanded = body.clone();
            for (param, arg) in params.iter().zip(&arg_texts) {
                expanded = expanded.replace(param, arg);
            }
            return reparse_expanded(&expanded, ctx);
        }
    }

    match func_name {
        "LT" if args.len() == 2 => {
            if let (Some(&a0), Some(&a1)) = (arg_strs.first(), arg_strs.get(1)) {
                if let (Some(layer), Some(key)) =
                    (resolve_to_u8(&ctx.qmk, a0), resolve_to_u8(&ctx.qmk, a1))
                {
                    return ResolvedKeycode::LayerTap { layer, key };
                }
            }
        }
        "MT" if args.len() == 2 => {
            if let (Some(&a0), Some(&a1)) = (arg_strs.first(), arg_strs.get(1)) {
                if let (Some(mods), Some(key)) =
                    (resolve_to_u8(&ctx.qmk, a0), resolve_to_u8(&ctx.qmk, a1))
                {
                    return ResolvedKeycode::ModTap { mods, key };
                }
            }
        }
        _ => {}
    }

    let full_text = node_text(node, source);
    if let Some(n) = ctx.qmk.resolve_value(full_text) {
        return classify(n);
    }

    ResolvedKeycode::Unresolved {
        raw: full_text.to_owned(),
        reason: "unresolved call expression".into(),
    }
}

/// Parses a QMK keycode string into a [`ResolvedKeycode`].
///
/// Text-based fallback when no AST node is available.
#[must_use]
pub fn parse_keycode(raw: &str) -> ResolvedKeycode {
    let s = raw.trim();
    if s.is_empty() || s == "KC_NO" {
        return ResolvedKeycode::NoOp;
    }
    if s == "KC_TRNS" {
        return ResolvedKeycode::Transparent;
    }
    ResolvedKeycode::Unresolved {
        raw: s.to_owned(),
        reason: "text-only fallback".into(),
    }
}

/// Parses a simplified QMK keymap.c layer body into keycodes.
///
/// Expects comma-separated keycode tokens extracted from within a
/// `LAYOUT_*(...)` call.
#[must_use]
pub fn parse_layer_body(body: &str) -> Vec<ResolvedKeycode> {
    let mut result = Vec::new();
    let mut depth: i32 = 0;
    let mut current = String::new();

    for ch in body.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_owned();
                if !trimmed.is_empty() {
                    result.push(parse_keycode(&trimmed));
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_owned();
    if !trimmed.is_empty() {
        result.push(parse_keycode(&trimmed));
    }
    result
}

/// Parses a QMK `keymap.c` source string into [`ParsedLayer`]s using tree-sitter.
///
/// Keycodes are resolved using only what is visible in the source itself — no
/// `#include` resolution or external defines. Use [`parse_keymap_c_with_includes`]
/// or [`parse_keymap_c_with_shared`] when the source depends on external headers.
pub fn parse_keymap_c(source: &str) -> Result<Vec<ParsedLayer>, KeymapError> {
    parse_keymap_c_inner(source, &HashMap::new(), &HashMap::new())
}

/// Parses a QMK `keymap.c` file with `#include` resolution from the filesystem.
///
/// Reads `path`, walks its `#include` graph to collect `#define` values and
/// function-like macros, and merges them with the pre-built `shared` defines
/// (typically from [`QmkDefines::from_quantum_dir`]). This is the recommended
/// entry point when transpiling a real QMK keyboard — it resolves layer-index
/// enums, custom keycode defines, and function-like macros like `LT()`.
pub fn parse_keymap_c_with_includes(
    path: &std::path::Path,
    shared: &QmkDefines,
) -> Result<Vec<ParsedLayer>, KeymapError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| KeymapError::ParseFailure(format!("cannot read {}: {e}", path.display())))?;
    let mut external_defines = shared.defines().clone();
    let keyboard_root = derive_keyboard_root(path).unwrap_or_else(|| {
        path.parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    });
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_c::LANGUAGE.into()).is_ok() {
        let mut visited = std::collections::HashSet::new();
        resolve_includes(
            path,
            &keyboard_root,
            &mut parser,
            &mut external_defines,
            &mut visited,
        );
    }
    parse_keymap_c_inner(&source, &external_defines, shared.fn_macros())
}

/// Parses a QMK `keymap.c` source string using a pre-built [`QmkDefines`] table.
///
/// Like [`parse_keymap_c`] but with access to external `#define` values and
/// function-like macros. Use this when you already have the source in memory
/// and a shared defines table (e.g. from [`QmkDefines::from_quantum_dir`]).
pub fn parse_keymap_c_with_shared(
    source: &str,
    shared: &QmkDefines,
) -> Result<Vec<ParsedLayer>, KeymapError> {
    parse_keymap_c_inner(source, shared.defines(), shared.fn_macros())
}

fn builtin_defines() -> HashMap<String, String> {
    [
        ("_______", "KC_TRNS"),
        ("______", "KC_TRNS"),
        ("____", "KC_TRNS"),
        ("___", "KC_TRNS"),
        ("XXXXXXX", "KC_NO"),
        ("xxxxxxx", "KC_NO"),
        ("xxx", "KC_NO"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

#[expect(
    clippy::iter_over_hash_type,
    reason = "insertion order does not matter, merging defines by key"
)]
fn parse_keymap_c_inner(
    source: &str,
    external_defines: &Defines,
    external_fn_macros: &FnMacros,
) -> Result<Vec<ParsedLayer>, KeymapError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|e| KeymapError::ParseFailure(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| KeymapError::ParseFailure("tree-sitter returned no tree".into()))?;

    let root = tree.root_node();
    let (mut defines, mut fn_macros) = collect_all_defines(root, source);
    for (k, v) in external_defines {
        defines.entry(k.clone()).or_insert_with(|| v.clone());
    }
    for (k, v) in external_fn_macros {
        fn_macros.entry(k.clone()).or_insert_with(|| v.clone());
    }

    let mut builtins = builtin_defines();
    for (k, v) in builtins.drain() {
        defines.entry(k).or_insert(v);
    }

    let ctx = ParseCtx {
        qmk: QmkDefines::from_raw(defines, fn_macros),
    };

    let keymaps_decl = find_keymaps_declaration(root, source)
        .ok_or_else(|| KeymapError::ParseFailure("no keymaps declaration found".into()))?;

    let init_list = find_child_by_kind(keymaps_decl, "initializer_list")
        .ok_or_else(|| KeymapError::ParseFailure("keymaps has no initializer_list".into()))?;

    let mut layers = Vec::new();

    for i in 0..init_list.named_child_count() {
        let Ok(idx) = u32::try_from(i) else { continue };
        let Some(child) = init_list.named_child(idx) else {
            continue;
        };

        if child.kind() == "initializer_pair" {
            let designator = find_child_by_kind(child, "subscript_designator");
            let layer_name = designator
                .and_then(|d| {
                    let inner = d.named_child(0)?;
                    Some(node_text(inner, source).to_owned())
                })
                .unwrap_or_else(|| format!("{}", layers.len()));

            let value = child
                .named_child(
                    u32::try_from(child.named_child_count() - 1).map_err(|e| {
                        KeymapError::ParseFailure(format!("child index overflow: {e}"))
                    })?,
                )
                .ok_or_else(|| KeymapError::ParseFailure("initializer_pair has no value".into()))?;

            let keys = extract_keycodes_from_node(value, source, &ctx);
            layers.push(ParsedLayer {
                index: layers.len(),
                name: layer_name,
                keys,
            });
        } else if child.kind() == "call_expression" || child.kind() == "initializer_list" {
            let keys = extract_keycodes_from_node(child, source, &ctx);
            layers.push(ParsedLayer {
                index: layers.len(),
                name: format!("{}", layers.len()),
                keys,
            });
        }
    }

    if layers.is_empty() {
        return Err(KeymapError::NoLayers);
    }

    Ok(layers)
}

pub(crate) fn collect_defines(root: Node<'_>, source: &str) -> Defines {
    let (defines, _) = collect_all_defines(root, source);
    defines
}

#[expect(
    clippy::string_slice,
    reason = "stripping C // comments from ASCII source"
)]
pub(crate) fn collect_all_defines(root: Node<'_>, source: &str) -> (Defines, FnMacros) {
    let mut defines: Defines = HashMap::new();
    let mut fn_macros: FnMacros = HashMap::new();
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        match node.kind() {
            "preproc_def" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source));
                let value = node.child_by_field_name("value").map(|n| {
                    let raw = node_text(n, source).trim();
                    raw.find("//").map_or(raw, |i| raw[..i].trim_end())
                });
                if let (Some(n), Some(v)) = (name, value) {
                    if !v.is_empty() {
                        defines.insert(n.to_owned(), v.to_owned());
                    }
                }
            }
            "preproc_function_def" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source));
                let params = node
                    .child_by_field_name("parameters")
                    .map(|n| node_text(n, source));
                let value = node.child_by_field_name("value").map(|n| {
                    let raw = node_text(n, source).trim();
                    raw.find("//").map_or(raw, |i| raw[..i].trim_end())
                });
                if let (Some(n), Some(p), Some(v)) = (name, params, value) {
                    let param_names: Vec<String> = p
                        .trim_start_matches('(')
                        .trim_end_matches(')')
                        .split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !v.is_empty() {
                        fn_macros.insert(n.to_owned(), (param_names, v.to_owned()));
                    }
                }
            }
            "declaration" | "type_definition" | "enum_specifier" => {
                collect_enums_from_node(node, source, &mut defines);
            }
            _ => {}
        }
    }
    (defines, fn_macros)
}

pub(crate) fn collect_enums_from_node(
    node: Node<'_>,
    source: &str,
    defines: &mut std::collections::HashMap<String, String>,
) {
    if node.kind() == "enum_specifier" {
        let mut next_val: i64 = 0;
        if let Some(body) = find_child_by_kind(node, "enumerator_list") {
            let mut cursor = body.walk();
            for child in body.named_children(&mut cursor) {
                if child.kind() != "enumerator" {
                    continue;
                }
                let name = child
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source));
                let value = child
                    .child_by_field_name("value")
                    .map(|n| node_text(n, source).trim());
                if let Some(v) = value {
                    if let Some(n) = parse_int_literal(v) {
                        next_val = n;
                    } else if let Some(existing) = defines.get(v) {
                        if let Some(n) = parse_int_literal(existing) {
                            next_val = n;
                        }
                    }
                }
                if let Some(n) = name {
                    defines.insert(n.to_owned(), next_val.to_string());
                }
                next_val += 1;
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_enums_from_node(child, source, defines);
    }
}

fn parse_int_literal(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        i64::from_str_radix(bin, 2).ok()
    } else {
        s.parse::<i64>().ok()
    }
}

fn find_keymaps_declaration<'a>(root: Node<'a>, source: &str) -> Option<Node<'a>> {
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() != "declaration" {
            continue;
        }
        let decl_text = node_text(node, source);
        if decl_text.contains("keymaps") {
            return Some(node);
        }
    }
    None
}

pub(crate) fn find_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = find_child_by_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

fn extract_keycodes_from_node(
    node: Node<'_>,
    source: &str,
    ctx: &ParseCtx,
) -> Vec<ResolvedKeycode> {
    if node.kind() == "call_expression" {
        let func = node.child_by_field_name("function");
        let func_name = func.map(|f| node_text(f, source));
        if let Some(name) = func_name {
            if name.starts_with("LAYOUT") {
                if let Some(args) = node.child_by_field_name("arguments") {
                    return collect_keycode_args(args, source, ctx);
                }
            }
        }
        return vec![parse_keycode_node(node, source, ctx)];
    }

    if node.kind() == "initializer_list" {
        return collect_keycode_args(node, source, ctx);
    }

    vec![parse_keycode_node(node, source, ctx)]
}

fn collect_keycode_args(args_node: Node<'_>, source: &str, ctx: &ParseCtx) -> Vec<ResolvedKeycode> {
    let mut keys = Vec::new();
    let mut cursor = args_node.walk();
    for child in args_node.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        keys.push(parse_keycode_node(child, source, ctx));
    }
    keys
}

pub(crate) fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::indexing_slicing, reason = "test code")]
#[expect(clippy::panic, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn parse_keycode_text_fallback() {
        assert_eq!(parse_keycode("KC_NO"), ResolvedKeycode::NoOp);
        assert_eq!(parse_keycode("KC_TRNS"), ResolvedKeycode::Transparent);
        assert_eq!(parse_keycode(""), ResolvedKeycode::NoOp);
        assert!(matches!(
            parse_keycode("KC_A"),
            ResolvedKeycode::Unresolved { .. }
        ));
    }

    #[test]
    fn parse_layer_body_splits_correctly() {
        let keys = parse_layer_body("KC_A, KC_B, KC_C");
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn parse_layer_body_empty() {
        let keys = parse_layer_body("");
        assert!(keys.is_empty());
    }

    fn shared_defines() -> QmkDefines {
        let kb = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards");
        let quantum = kb.join("quantum");
        if quantum.exists() {
            QmkDefines::from_quantum_dir(&quantum)
        } else {
            QmkDefines::default()
        }
    }

    #[test]
    fn parse_keymap_c_basic() {
        let source = "
#include QMK_KEYBOARD_H

enum layers { _BASE, _FN };

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [_BASE] = LAYOUT_planck_grid(
        KC_TAB,  KC_Q,    KC_W,    KC_E,
        KC_ESC,  KC_A,    KC_S,    KC_D
    ),
    [_FN] = LAYOUT_planck_grid(
        KC_GRV,  KC_1,    KC_2,    KC_3,
        _______, KC_F1,   KC_F2,   KC_F3
    ),
};
";
        let shared = shared_defines();
        let layers = parse_keymap_c_with_shared(source, &shared).unwrap();
        assert_eq!(layers.len(), 2);
        let [base, fun] = layers.as_slice() else {
            panic!("expected 2 layers")
        };
        assert_eq!(base.name, "_BASE");
        assert_eq!(base.keys.len(), 8);
        assert_eq!(fun.name, "_FN");
        assert!(matches!(
            fun.keys.get(4),
            Some(ResolvedKeycode::Transparent)
        ));
    }

    #[test]
    fn parse_keymap_c_with_comments_and_macros() {
        let shared = shared_defines();
        let source = "
#include QMK_KEYBOARD_H

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(
        KC_A, LT(1, KC_SPC), MO(1),
        MT(MOD_LSFT, KC_B), KC_NO, _______
    ),
};
";
        let layers = parse_keymap_c_with_shared(source, &shared).unwrap();
        assert_eq!(layers.len(), 1);
        let layer = layers.first().unwrap();
        assert_eq!(layer.keys.len(), 6);
        assert!(matches!(
            layer.keys.get(2),
            Some(ResolvedKeycode::Momentary(1))
        ));
        assert!(matches!(layer.keys.get(4), Some(ResolvedKeycode::NoOp)));
        assert!(matches!(
            layer.keys.get(5),
            Some(ResolvedKeycode::Transparent)
        ));
    }

    #[test]
    fn parse_keymap_c_enum_resolution() {
        let shared = shared_defines();
        let source = "
enum layers { _BASE, _FN };
#define LOWER MO(_FN)

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [_BASE] = LAYOUT(
        MO(_FN), LOWER
    ),
};
";
        let layers = parse_keymap_c_with_shared(source, &shared).unwrap();
        assert_eq!(layers.len(), 1);
        let layer = layers.first().unwrap();
        assert_eq!(layer.keys.first(), Some(&ResolvedKeycode::Momentary(1)));
        assert_eq!(layer.keys.get(1), Some(&ResolvedKeycode::Momentary(1)));
    }

    #[test]
    fn parse_keymap_c_no_keymaps() {
        let source = "int main() { return 0; }";
        let err = parse_keymap_c(source).unwrap_err();
        assert!(matches!(err, KeymapError::ParseFailure(_)));
    }

    #[test]
    fn parse_failure_display() {
        let err = KeymapError::ParseFailure("bad input".into());
        assert_eq!(err.to_string(), "failed to parse C source: bad input");
    }

    #[test]
    fn mo_via_numeric_resolution() {
        let shared = shared_defines();
        if shared.resolve_value("KC_A").is_none() {
            return;
        }
        let source = "
const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(MO(1)),
};
";
        let layers = parse_keymap_c_with_shared(source, &shared).unwrap();
        assert_eq!(layers[0].keys[0], ResolvedKeycode::Momentary(1));
    }

    #[test]
    fn s_kc_a_via_fn_macro() {
        let shared = shared_defines();
        if shared.resolve_value("KC_A").is_none() {
            return;
        }
        let source = "
const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(S(KC_A)),
};
";
        let layers = parse_keymap_c_with_shared(source, &shared).unwrap();
        assert_eq!(
            layers[0].keys[0],
            ResolvedKeycode::Modified { mods: 2, key: 4 },
        );
    }

    #[test]
    fn lsft_t_via_fn_macro() {
        let shared = shared_defines();
        if shared.resolve_value("KC_Z").is_none() {
            return;
        }
        let source = "
const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(LSFT_T(KC_Z)),
};
";
        let layers = parse_keymap_c_with_shared(source, &shared).unwrap();
        assert_eq!(
            layers[0].keys[0],
            ResolvedKeycode::ModTap { mods: 2, key: 29 },
        );
    }

    #[test]
    fn unresolved_custom_keycode() {
        let shared = shared_defines();
        let source = "
const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(MY_CUSTOM_KC),
};
";
        let layers = parse_keymap_c_with_shared(source, &shared).unwrap();
        assert!(matches!(
            layers[0].keys[0],
            ResolvedKeycode::Unresolved { .. }
        ));
    }

    #[test]
    fn meow65_mo_fn_resolves() {
        let kb = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards");
        let path = kb.join("keyboards/meow65/keymaps/default/keymap.c");
        assert!(
            path.exists(),
            "{path:?} not found — run `git submodule update --init`"
        );
        let quantum = kb.join("quantum");
        let shared = QmkDefines::from_quantum_dir(&quantum);
        let layers = parse_keymap_c_with_includes(&path, &shared).unwrap();
        for layer in &layers {
            for (i, kc) in layer.keys.iter().enumerate() {
                if let ResolvedKeycode::Unresolved { ref raw, .. } = kc {
                    assert!(
                        !raw.contains("_FN"),
                        "key {i} in layer {} is unresolved: {raw}",
                        layer.name,
                    );
                }
            }
        }
    }

    #[test]
    fn enum_extraction_layer_names() {
        let shared = shared_defines();
        let src = "
enum layer_names {
    _BASE,
    _FN
};
const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [_BASE] = LAYOUT(MO(_FN)),
};
";
        let layers = parse_keymap_c_with_shared(src, &shared).unwrap();
        let kc = layers[0].keys.first().unwrap();
        assert_eq!(*kc, ResolvedKeycode::Momentary(1));
    }

    #[test]
    fn quantum_dir_includes_jp_bsls() {
        let kb = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../keyboards");
        let quantum = kb.join("quantum");
        assert!(
            quantum.exists(),
            "{quantum:?} not found — run `git submodule update --init`"
        );
        let shared = QmkDefines::from_quantum_dir(&quantum);
        assert_eq!(
            shared.defines().get("JP_BSLS").map(|s| s.as_str()),
            Some("KC_INT1")
        );
    }

    #[test]
    fn collect_defines_from_qmk_header() {
        let src = "#define JP_BSLS KC_INT1 // (backslash)\n#define JP_AT ALGR(KC_LBRC) // @\n#define LOWER MO(1)\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        let defines = collect_defines(tree.root_node(), src);
        assert_eq!(defines.get("JP_BSLS").map(|s| s.as_str()), Some("KC_INT1"));
        assert_eq!(
            defines.get("JP_AT").map(|s| s.as_str()),
            Some("ALGR(KC_LBRC)")
        );
        assert_eq!(defines.get("LOWER").map(|s| s.as_str()), Some("MO(1)"));
    }

    #[test]
    fn lt_structural_parse() {
        let shared = shared_defines();
        if shared.resolve_value("KC_SPC").is_none() {
            return;
        }
        let source = "
const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT(LT(1, KC_SPC)),
};
";
        let layers = parse_keymap_c_with_shared(source, &shared).unwrap();
        assert_eq!(
            layers[0].keys[0],
            ResolvedKeycode::LayerTap {
                layer: 1,
                key: 0x2C
            },
        );
    }
}
