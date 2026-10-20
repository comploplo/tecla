use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tree_sitter::Parser;

use super::parser::{collect_all_defines, collect_defines, node_text};

/// Pre-parsed QMK `#define` and `enum` table, built once and shared across keymap parses.
///
/// Constructed from a QMK `quantum/` directory (or full `keyboards/` tree) by
/// tree-sitter-scanning all header files. Pass this to
/// [`parse_keymap_c_with_includes`](super::parse_keymap_c_with_includes) or
/// [`parse_keymap_c_with_shared`](super::parse_keymap_c_with_shared) so that
/// keycodes referencing external defines resolve to numeric values.
#[derive(Debug, Clone, Default)]
pub struct QmkDefines {
    defines: HashMap<String, String>,
    fn_macros: HashMap<String, (Vec<String>, String)>,
}

impl QmkDefines {
    /// Constructs from raw define and fn-macro tables.
    #[must_use]
    pub fn from_raw(
        defines: HashMap<String, String>,
        fn_macros: HashMap<String, (Vec<String>, String)>,
    ) -> Self {
        Self { defines, fn_macros }
    }

    /// Returns a reference to the simple defines table.
    #[must_use]
    pub fn defines(&self) -> &HashMap<String, String> {
        &self.defines
    }

    /// Returns a reference to the function-like macro table.
    #[must_use]
    pub fn fn_macros(&self) -> &HashMap<String, (Vec<String>, String)> {
        &self.fn_macros
    }

    /// Parses all `.h` files under a QMK `quantum/` directory tree and collects
    /// every `#define`, `enum` constant, and function-like macro into a lookup
    /// table. This is the primary way to build a shared defines table for keycode
    /// resolution.
    #[must_use]
    pub fn from_quantum_dir(quantum_dir: &Path) -> Self {
        let mut defines = HashMap::new();
        let mut fn_macros = HashMap::new();
        let mut parser = Parser::new();
        if parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .is_err()
        {
            return Self { defines, fn_macros };
        }
        scan_dir_headers_recursive(
            quantum_dir,
            Path::new(""),
            &mut parser,
            &mut defines,
            &mut fn_macros,
        );
        Self { defines, fn_macros }
    }

    /// Parses all `.h` and `.c` files under a `keyboards/` tree, including
    /// a `quantum/` subdirectory if present. Builds a single shared table
    /// so per-keymap resolution is unnecessary.
    #[must_use]
    pub fn from_keyboards_dir(keyboards_dir: &Path) -> Self {
        let mut defines = HashMap::new();
        let mut fn_macros = HashMap::new();
        let mut parser = Parser::new();
        if parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .is_err()
        {
            return Self { defines, fn_macros };
        }
        scan_dir_all_recursive(keyboards_dir, &mut parser, &mut defines, &mut fn_macros);
        Self { defines, fn_macros }
    }

    /// Follows the `#define` chain until it reaches a numeric literal, evaluating
    /// arithmetic expressions and bitwise operations along the way. Returns `None`
    /// if the name is not defined or the chain does not resolve to a `u16`.
    #[must_use]
    pub fn resolve_value(&self, name: &str) -> Option<u16> {
        let mut visited = HashSet::new();
        self.eval_expr(name.trim(), &mut visited, 0)
    }

    /// Expands a function-like macro and resolves the result to a numeric value.
    #[must_use]
    pub fn expand_and_resolve(&self, name: &str, args: &[&str]) -> Option<u16> {
        let (params, body) = self.fn_macros.get(name)?;
        let mut expanded = body.clone();
        for (param, arg) in params.iter().zip(args) {
            expanded = replace_param(&expanded, param, arg);
        }
        self.resolve_value(&expanded)
    }

    #[expect(
        clippy::string_slice,
        reason = "inputs are QMK C identifiers and expressions, guaranteed ASCII"
    )]
    fn eval_expr(&self, s: &str, visited: &mut HashSet<String>, depth: u16) -> Option<u16> {
        if depth > 20 {
            return None;
        }
        let s = strip_outer_parens(s.trim());
        if s.is_empty() {
            return None;
        }

        if let Some(pos) = find_top_level_op(s, '|') {
            let left = self.eval_expr(&s[..pos], visited, depth + 1)?;
            let right = self.eval_expr(&s[pos + 1..], visited, depth + 1)?;
            return Some(left | right);
        }
        if let Some(pos) = find_top_level_op(s, '+') {
            let left = self.eval_expr(&s[..pos], visited, depth + 1)?;
            let right = self.eval_expr(&s[pos + 1..], visited, depth + 1)?;
            return Some(left.wrapping_add(right));
        }
        if let Some(pos) = find_top_level_op(s, '&') {
            let left = self.eval_expr(&s[..pos], visited, depth + 1)?;
            let right = self.eval_expr(&s[pos + 1..], visited, depth + 1)?;
            return Some(left & right);
        }
        if let Some(pos) = find_top_level_shift(s) {
            let left = self.eval_expr(&s[..pos], visited, depth + 1)?;
            let right = self.eval_expr(&s[pos + 2..], visited, depth + 1)?;
            return Some(left << right);
        }

        if let Some(n) = parse_numeric(s) {
            return Some(n);
        }

        if !visited.insert(s.to_owned()) {
            return None;
        }
        if let Some(v) = self.defines.get(s) {
            let result = self.eval_expr(v, visited, depth + 1);
            visited.remove(s);
            return result;
        }
        visited.remove(s);

        if let Some((name, args)) = parse_call_text(s) {
            if let Some((params, body)) = self.fn_macros.get(name) {
                if args.len() == params.len() {
                    let mut expanded = body.clone();
                    for (param, arg) in params.iter().zip(&args) {
                        expanded = replace_param(&expanded, param, arg);
                    }
                    return self.eval_expr(&expanded, visited, depth + 1);
                }
            }
        }

        None
    }

    /// Merges another table into this one. Existing keys are preserved.
    #[expect(
        clippy::iter_over_hash_type,
        reason = "insertion order does not matter, merging defines by key"
    )]
    pub fn merge(&mut self, other: &Self) {
        for (k, v) in &other.defines {
            self.defines.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &other.fn_macros {
            self.fn_macros.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
}

#[expect(
    clippy::iter_over_hash_type,
    reason = "insertion order does not matter, merging defines by key"
)]
pub(crate) fn resolve_includes(
    file_path: &Path,
    keyboard_root: &Path,
    parser: &mut Parser,
    defines: &mut HashMap<String, String>,
    visited: &mut HashSet<PathBuf>,
) {
    if !visited.insert(file_path.to_path_buf()) {
        return;
    }

    let Ok(raw) = std::fs::read_to_string(file_path) else {
        return;
    };
    let content = strip_line_comments(&raw);
    let Some(tree) = parser.parse(&content, None) else {
        return;
    };
    let root = tree.root_node();

    let file_defines = collect_defines(root, &content);
    for (k, v) in file_defines {
        defines.entry(k).or_insert(v);
    }

    let file_dir = file_path.parent().unwrap_or(Path::new("."));
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() != "preproc_include" {
            continue;
        }
        let Some(path_node) = node.child_by_field_name("path") else {
            continue;
        };
        match path_node.kind() {
            "string_literal" => {
                let raw = node_text(path_node, &content);
                let inc_path = raw.trim_matches('"');
                if inc_path.is_empty() {
                    continue;
                }
                let resolved = file_dir.join(inc_path);
                if resolved.is_file() {
                    resolve_includes(&resolved, keyboard_root, parser, defines, visited);
                } else {
                    let from_kb = keyboard_root.join(inc_path);
                    if from_kb.is_file() {
                        resolve_includes(&from_kb, keyboard_root, parser, defines, visited);
                    }
                }
            }
            "identifier" => {
                let text = node_text(path_node, &content);
                if text == "QMK_KEYBOARD_H" {
                    for header in resolve_qmk_keyboard_h(file_path) {
                        resolve_includes(&header, keyboard_root, parser, defines, visited);
                    }
                }
            }
            "system_lib_string" => {}
            _ => {}
        }
    }
}

fn resolve_qmk_keyboard_h(keymap_path: &Path) -> Vec<PathBuf> {
    let mut headers = Vec::new();
    let keyboard_root = derive_keyboard_root(keymap_path);
    let Some(kb_root) = keyboard_root else {
        return headers;
    };

    let mut dir = kb_root.clone();
    let keyboards_dir = find_keyboards_ancestor(&kb_root);

    loop {
        if let Some(name) = dir.file_name() {
            let header = dir.join(format!("{}.h", name.to_string_lossy()));
            if header.is_file() {
                headers.push(header);
            }
        }
        if Some(&dir) == keyboards_dir.as_ref()
            || !dir.starts_with(keyboards_dir.as_deref().unwrap_or(Path::new("")))
        {
            break;
        }
        let Some(parent) = dir.parent() else { break };
        if parent == dir {
            break;
        }
        dir = parent.to_path_buf();
    }

    headers
}

fn find_keyboards_ancestor(path: &Path) -> Option<PathBuf> {
    let mut dir = path;
    loop {
        if dir.file_name().is_some_and(|n| n == "keyboards") {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

pub(crate) fn derive_keyboard_root(keymap_path: &Path) -> Option<PathBuf> {
    let mut dir = keymap_path.parent()?;
    loop {
        if dir.file_name().is_some_and(|n| n == "keymaps") {
            return dir.parent().map(|p| p.to_path_buf());
        }
        let parent = dir.parent()?;
        if parent == dir {
            return None;
        }
        dir = parent;
    }
}

fn scan_dir_all_recursive(
    dir: &Path,
    parser: &mut Parser,
    defines: &mut HashMap<String, String>,
    fn_macros: &mut HashMap<String, (Vec<String>, String)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_all_recursive(&path, parser, defines, fn_macros);
        } else if path.extension().is_some_and(|e| e == "h") {
            parse_header_all(&path, parser, defines, fn_macros);
        }
    }
}

fn scan_dir_headers_recursive(
    dir: &Path,
    skip: &Path,
    parser: &mut Parser,
    defines: &mut HashMap<String, String>,
    fn_macros: &mut HashMap<String, (Vec<String>, String)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == skip {
            continue;
        }
        if path.is_dir() {
            scan_dir_headers_recursive(&path, skip, parser, defines, fn_macros);
        } else if path.extension().is_some_and(|e| e == "h") {
            parse_header_all(&path, parser, defines, fn_macros);
        }
    }
}

#[expect(
    clippy::iter_over_hash_type,
    reason = "insertion order does not matter, merging defines by key"
)]
fn parse_header_all(
    path: &Path,
    parser: &mut Parser,
    defines: &mut HashMap<String, String>,
    fn_macros: &mut HashMap<String, (Vec<String>, String)>,
) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let cleaned = strip_line_comments(&content);
    let Some(tree) = parser.parse(&cleaned, None) else {
        return;
    };
    let root = tree.root_node();
    let (file_defines, file_fn_macros) = collect_all_defines(root, &cleaned);
    for (k, v) in file_defines {
        defines.entry(k).or_insert(v);
    }
    for (k, v) in file_fn_macros {
        fn_macros.entry(k).or_insert(v);
    }
}

fn strip_outer_parens(s: &str) -> &str {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return s;
    }
    let Some(inner) = s.get(1..s.len() - 1) else {
        return s;
    };
    let mut depth: i32 = 0;
    for (i, &b) in inner.as_bytes().iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth < 0 && i < inner.len() - 1 {
                    return s;
                }
            }
            _ => {}
        }
    }
    if depth == 0 {
        strip_outer_parens(inner)
    } else {
        s
    }
}

fn find_top_level_op(s: &str, op: char) -> Option<usize> {
    let mut depth: i32 = 0;
    let bytes = s.as_bytes();
    let op_byte = u8::try_from(op).ok()?;
    for i in (0..bytes.len()).rev() {
        let Some(&b) = bytes.get(i) else { continue };
        match b {
            b')' => depth += 1,
            b'(' => depth -= 1,
            b if b == op_byte && depth == 0 => {
                if op == '&' && i > 0 && bytes.get(i - 1) == Some(&b'&') {
                    continue;
                }
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

fn find_top_level_shift(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let bytes = s.as_bytes();
    for i in (1..bytes.len()).rev() {
        let Some(&b) = bytes.get(i) else { continue };
        match b {
            b')' => depth += 1,
            b'(' => depth -= 1,
            b'<' if depth == 0 && bytes.get(i.wrapping_sub(1)) == Some(&b'<') => {
                return Some(i - 1);
            }
            _ => {}
        }
    }
    None
}

fn parse_numeric(s: &str) -> Option<u16> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16).ok()
    } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        u16::from_str_radix(bin, 2).ok()
    } else {
        s.parse::<u16>().ok()
    }
}

#[expect(
    clippy::string_slice,
    reason = "inputs are QMK C identifiers and expressions, guaranteed ASCII"
)]
fn parse_call_text(s: &str) -> Option<(&str, Vec<&str>)> {
    let open = s.find('(')?;
    let name = s[..open].trim();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    if !s.ends_with(')') {
        return None;
    }
    let inner = &s[open + 1..s.len() - 1];
    let mut args = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0;
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        args.push(last);
    }
    Some((name, args))
}

fn replace_param(body: &str, param: &str, arg: &str) -> String {
    let mut result = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(pos) = rest.find(param) {
        let before_ok = pos == 0
            || rest
                .as_bytes()
                .get(pos - 1)
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
        let after_pos = pos + param.len();
        let after_ok = rest
            .as_bytes()
            .get(after_pos)
            .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
        if before_ok && after_ok {
            result.push_str(rest.get(..pos).unwrap_or_default());
            result.push_str(arg);
            rest = rest.get(after_pos..).unwrap_or_default();
        } else {
            result.push_str(rest.get(..after_pos).unwrap_or_default());
            rest = rest.get(after_pos..).unwrap_or_default();
        }
    }
    result.push_str(rest);
    result
}

#[expect(
    clippy::string_slice,
    reason = "scanning ASCII-only C source for // comments"
)]
#[expect(
    clippy::indexing_slicing,
    reason = "loop index i is always within bytes.len()"
)]
fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let mut in_string = false;
            let bytes = line.as_bytes();
            for i in 0..bytes.len() {
                if in_string {
                    if bytes[i] == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
                        in_string = false;
                    }
                    continue;
                }
                if bytes[i] == b'"' {
                    in_string = true;
                    continue;
                }
                if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    return line[..i].trim_end().to_owned();
                }
            }
            line.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn quantum_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../keyboards/quantum")
    }

    fn simple_defines() -> QmkDefines {
        let mut defines = HashMap::new();
        defines.insert("KC_A".into(), "0x0004".into());
        defines.insert("MY_KEY".into(), "KC_A".into());
        defines.insert("QK_LSFT".into(), "0x0200".into());
        defines.insert("CYCLE_A".into(), "CYCLE_B".into());
        defines.insert("CYCLE_B".into(), "CYCLE_A".into());
        let mut fn_macros = HashMap::new();
        fn_macros.insert(
            "MO".into(),
            (vec!["layer".into()], "QK_MOMENTARY | (layer)".into()),
        );
        QmkDefines { defines, fn_macros }
    }

    #[test]
    fn resolve_value_evaluates_literals_names_and_expressions() {
        let mut qd = simple_defines();
        qd.defines.insert("QK_MOMENTARY".into(), "0x5220".into());
        let cases: &[(&str, Option<u16>)] = &[
            ("0xFF", Some(0x00FF)),
            ("42", Some(42)),
            ("0b0100", Some(4)),
            ("KC_A", Some(0x0004)),
            ("MY_KEY", Some(0x0004)),
            ("QK_LSFT | KC_A", Some(0x0204)),
            ("(QK_LSFT | (KC_A))", Some(0x0204)),
            ("QK_MOMENTARY + 1", Some(0x5221)),
            ("1 << 8", Some(256)),
            ("0xFF & 0x0F", Some(0x0F)),
            ("NONEXISTENT", None),
            ("CYCLE_A", None),
        ];
        for &(expr, want) in cases {
            assert_eq!(qd.resolve_value(expr), want, "{expr}");
        }
    }

    #[test]
    fn expand_and_resolve_fn_macro() {
        let mut qd = simple_defines();
        qd.defines.insert("QK_MOMENTARY".into(), "0x5220".into());
        assert_eq!(qd.expand_and_resolve("MO", &["1"]), Some(0x5221));
    }

    #[test]
    fn quantum_headers_resolve_real_qmk_keycodes() {
        let dir = quantum_dir();
        assert!(
            dir.exists(),
            "{dir:?} not found — run `git submodule update --init`"
        );
        let qd = QmkDefines::from_quantum_dir(&dir);
        assert_eq!(qd.resolve_value("KC_A"), Some(0x0004));
        assert_eq!(qd.resolve_value("QK_MOMENTARY"), Some(0x5220));
        assert_eq!(qd.expand_and_resolve("MO", &["1"]), Some(0x5221));
    }

    #[test]
    fn replace_param_matches_whole_identifiers_only() {
        let cases = [
            ("QK_n_THING | (n)", "n", "42", "QK_n_THING | (42)"),
            ("_layer + layer", "layer", "1", "_layer + 1"),
            ("layer_ + layer", "layer", "1", "layer_ + 1"),
        ];
        for (body, param, arg, want) in cases {
            assert_eq!(replace_param(body, param, arg), want, "{body}");
        }
    }

    #[test]
    fn strip_line_comments_preserves_url_in_string() {
        let input = "#define FOO \"http://example.com\"\n";
        let result = strip_line_comments(input);
        assert!(result.contains("http://example.com"));
    }

    #[test]
    fn eval_expr_depth_limit() {
        let mut qd = simple_defines();
        qd.defines.insert(
            "DEEP".into(),
            "((((((((((((((((((((((DEEP))))))))))))))))))))))".into(),
        );
        assert_eq!(qd.resolve_value("DEEP"), None);
    }
}
