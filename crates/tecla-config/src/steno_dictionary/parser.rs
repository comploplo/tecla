//! Lapwing/Plover steno dictionary importer.
//!
//! This module reads vendored Lapwing JSON dictionaries, parses their outline
//! chords into steno stroke bitmasks, and compiles their translation strings
//! into a compact native playback bytecode consumed by the firmware.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use nom::branch::alt;
use nom::bytes::complete::{tag, tag_no_case, take_till, take_till1, take_while1};
use nom::character::complete::{char as nom_char, digit1, multispace1};
use nom::combinator::{all_consuming, map, map_res, opt, rest, value};
use nom::error::{Error, ErrorKind};
use nom::multi::{many0, many1};
use nom::sequence::{delimited, preceded, terminated};
use nom::{IResult, Parser};
use serde_json::from_str;

const OP_TEXT: u8 = 0x01;
const OP_HOST_CHORD: u8 = 0x02;
const OP_NO_SPACE: u8 = 0x03;
const OP_SET_CASE: u8 = 0x05;
const OP_SET_SPACE: u8 = 0x06;
const OP_RESET_FORMATTING: u8 = 0x07;
const OP_REPEAT_LAST_TRANSLATION: u8 = 0x08;
const OP_UNDO_LAST_TRANSLATION: u8 = 0x09;
const OP_NOOP: u8 = 0x0A;
const OP_RETRO_CASE: u8 = 0x0B;
const OP_STITCH: u8 = 0x0C;

const CASE_CAP_FIRST_WORD: u8 = 0x01;
const CASE_UPPER_FIRST_WORD: u8 = 0x02;
const CASE_LOWER_FIRST_CHAR: u8 = 0x03;

/// A compiled Lapwing dictionary bundle.
#[derive(Debug, Clone)]
pub struct StenoDictionaryBuild {
    /// Serialized DAFSA ready to embed in firmware.
    pub bytes: Vec<u8>,
    /// Statistics gathered while compiling the dictionaries.
    pub stats: StenoDictionaryStats,
}

/// Compilation statistics for a Lapwing dictionary bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StenoDictionaryStats {
    /// Number of JSON files scanned.
    pub files_scanned: usize,
    /// Number of dictionary entries seen across all files.
    pub entries_seen: usize,
    /// Number of entries successfully compiled.
    pub entries_kept: usize,
    /// Number of entries skipped because the outline could not be parsed.
    pub entries_skipped_outline: usize,
    /// Number of entries skipped because the translation could not be encoded.
    pub entries_skipped_translation: usize,
}

/// Canonical Lapwing JSON file order.
///
/// The more specific dictionaries come first so later files can override them
/// when the same stroke is repeated.
const LAPWING_JSON_FILES: &[&str] = &[
    "lapwing-commands.json",
    "lapwing-numbers.json",
    "lapwing-uk-additions.json",
    "lapwing-semi-modal-movement.json",
    "lapwing-prefixed-proper-nouns.json",
    "lapwing-proper-nouns.json",
    "lapwing-base.json",
];

/// Compiles every supported Lapwing JSON dictionary under `dir`.
pub fn compile_lapwing_dictionary_dir(dir: impl AsRef<Path>) -> io::Result<StenoDictionaryBuild> {
    let files = collect_dictionary_files(dir.as_ref())?;
    compile_lapwing_dictionary_files(files)
}

/// Compiles a specific list of Lapwing JSON files.
pub fn compile_lapwing_dictionary_files(files: Vec<PathBuf>) -> io::Result<StenoDictionaryBuild> {
    let mut stats = StenoDictionaryStats::default();
    let mut merged: BTreeMap<Vec<u64>, Vec<u8>> = BTreeMap::new();

    for path in files {
        stats.files_scanned += 1;
        let raw = fs::read_to_string(&path)?;
        let entries: BTreeMap<String, String> =
            from_str(&raw).map_err(|e| invalid_data(&path, e))?;

        for (outline, translation) in entries {
            stats.entries_seen += 1;

            let Some(strokes) = parse_outline(&outline) else {
                stats.entries_skipped_outline += 1;
                continue;
            };

            let Some(encoded) = compile_translation(&translation) else {
                stats.entries_skipped_translation += 1;
                continue;
            };

            if encoded.is_empty() {
                stats.entries_skipped_translation += 1;
                continue;
            }

            merged.insert(strokes, encoded);
            stats.entries_kept += 1;
        }
    }

    let entries: Vec<(Vec<u64>, Vec<u8>)> = merged.into_iter().collect();
    let bytes = crate::trie::compile_steno_dictionary(&entries);
    Ok(StenoDictionaryBuild { bytes, stats })
}

fn collect_dictionary_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut seen = BTreeSet::<String>::new();

    for name in LAPWING_JSON_FILES {
        let path = dir.join(name);
        if path.is_file() {
            seen.insert((*name).to_string());
            files.push(path);
        }
    }

    let entries = fs::read_dir(dir)?;
    let mut extras: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !seen.contains(name))
        })
        .collect();
    extras.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
    files.extend(extras);

    Ok(files)
}

fn invalid_data(path: &Path, err: serde_json::Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("failed to parse {}: {err}", path.display()),
    )
}

/// Parses a Plover/Lapwing outline into a sequence of steno chord bitmasks.
pub fn parse_outline(outline: &str) -> Option<Vec<u64>> {
    let mut strokes = Vec::new();

    for stroke in outline.split('/') {
        if stroke.is_empty() {
            return None;
        }
        strokes.push(parse_stroke(stroke)?);
    }

    if strokes.is_empty() {
        None
    } else {
        Some(strokes)
    }
}

fn parse_stroke(stroke: &str) -> Option<u64> {
    let mut mask = 0u64;
    let mut side = HandSide::Left;

    for ch in stroke.chars() {
        match ch {
            '#' => set_bit(&mut mask, BIT_NUMBER),
            '-' => side = HandSide::Right,
            '*' => set_bit(&mut mask, BIT_STAR),
            'A' => set_bit(&mut mask, BIT_A),
            'O' => set_bit(&mut mask, BIT_O),
            'E' => set_bit(&mut mask, BIT_E),
            'U' => set_bit(&mut mask, BIT_U),
            'F' => {
                set_bit(&mut mask, BIT_F);
                side = HandSide::Right;
            }
            'B' => {
                set_bit(&mut mask, BIT_B);
                side = HandSide::Right;
            }
            'L' => {
                set_bit(&mut mask, BIT_L);
                side = HandSide::Right;
            }
            'G' => {
                set_bit(&mut mask, BIT_G);
                side = HandSide::Right;
            }
            'D' => {
                set_bit(&mut mask, BIT_D);
                side = HandSide::Right;
            }
            'Z' => {
                set_bit(&mut mask, BIT_Z);
                side = HandSide::Right;
            }
            'K' => {
                if side == HandSide::Right {
                    return None;
                }
                set_bit(&mut mask, BIT_K);
            }
            'W' => {
                if side == HandSide::Right {
                    return None;
                }
                set_bit(&mut mask, BIT_W);
            }
            'H' => {
                if side == HandSide::Right {
                    return None;
                }
                set_bit(&mut mask, BIT_H);
            }
            'P' => {
                if side == HandSide::Right {
                    set_bit(&mut mask, BIT_RP);
                } else {
                    set_bit(&mut mask, BIT_LP);
                }
            }
            'R' => {
                if side == HandSide::Right {
                    set_bit(&mut mask, BIT_RR);
                } else {
                    set_bit(&mut mask, BIT_LR);
                }
            }
            'T' => {
                if side == HandSide::Right {
                    set_bit(&mut mask, BIT_RT);
                } else {
                    set_bit(&mut mask, BIT_LT);
                }
            }
            'S' => {
                if side == HandSide::Right {
                    set_bit(&mut mask, BIT_RS);
                } else {
                    set_bit(&mut mask, BIT_LS);
                }
            }
            _ => return None,
        }
    }

    Some(mask)
}

fn set_bit(mask: &mut u64, bit: u8) {
    *mask |= 1u64 << bit;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TranslationPart {
    Text(String),
    Stitch(String),
    NoSpace,
    HostChord { mods: u8, key: u8 },
    SetCase(u8),
    SetSpace,
    ResetFormatting,
    RepeatLastTranslation,
    UndoLastTranslation,
    Noop,
    RetroCase(u8),
}

/// Compiles a Lapwing/Plover translation string into native playback opcodes.
fn compile_translation(raw: &str) -> Option<Vec<u8>> {
    let trimmed = raw.trim_start();
    let parts = if trimmed.starts_with('=') {
        let (_, parts) = parse_command(trimmed).ok()?;
        parts
    } else {
        let (_, parts) = parse_translation_body(raw).ok()?;
        parts
    };

    let mut out = Vec::new();
    for part in parts {
        encode_part(part, &mut out)?;
    }
    if out.is_empty() {
        out.push(OP_NOOP);
    }
    Some(out)
}

fn parse_command(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    alt((
        value(vec![TranslationPart::UndoLastTranslation], tag("=undo")),
        value(
            vec![TranslationPart::RepeatLastTranslation],
            tag("=repeat_last_translation"),
        ),
    ))
    .parse(input)
}

fn encode_part(part: TranslationPart, out: &mut Vec<u8>) -> Option<()> {
    match part {
        TranslationPart::Text(text) => push_text(out, text.as_str()),
        TranslationPart::Stitch(text) => {
            out.push(OP_STITCH);
            push_text_payload(out, text.as_str())
        }
        TranslationPart::NoSpace => {
            out.push(OP_NO_SPACE);
            Some(())
        }
        TranslationPart::HostChord { mods, key } => {
            out.push(OP_HOST_CHORD);
            out.push(mods);
            out.push(key);
            Some(())
        }
        TranslationPart::SetCase(mode) => {
            out.push(OP_SET_CASE);
            out.push(mode);
            Some(())
        }
        TranslationPart::SetSpace => {
            out.push(OP_SET_SPACE);
            Some(())
        }
        TranslationPart::ResetFormatting => {
            out.push(OP_RESET_FORMATTING);
            Some(())
        }
        TranslationPart::RepeatLastTranslation => {
            out.push(OP_REPEAT_LAST_TRANSLATION);
            Some(())
        }
        TranslationPart::UndoLastTranslation => {
            out.push(OP_UNDO_LAST_TRANSLATION);
            Some(())
        }
        TranslationPart::Noop => {
            out.push(OP_NOOP);
            Some(())
        }
        TranslationPart::RetroCase(mode) => {
            out.push(OP_RETRO_CASE);
            out.push(mode);
            Some(())
        }
    }
}

fn parse_translation_body(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(many0(parse_segment), flatten_segments).parse(input)
}

fn flatten_segments(segments: Vec<Vec<TranslationPart>>) -> Vec<TranslationPart> {
    let mut out = Vec::new();
    for mut segment in segments {
        out.append(&mut segment);
    }
    out
}

fn parse_segment(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    alt((parse_braced_token, parse_outer_glue, parse_plain_text)).parse(input)
}

fn parse_outer_glue(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(
        all_consuming(many1(alt((
            value((), nom_char('^')),
            value((), multispace1),
        )))),
        |_| vec![TranslationPart::NoSpace],
    )
    .parse(input)
}

fn parse_plain_text(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(take_till1(|c| c == '{' || c == '^'), |text: &str| {
        vec![TranslationPart::Text(text.to_string())]
    })
    .parse(input)
}

fn parse_braced_token(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    let (rest, inner) =
        delimited(nom_char('{'), take_till(|c| c == '}'), nom_char('}')).parse(input)?;
    let (_, parts) = all_consuming(parse_token_content).parse(inner)?;
    Ok((rest, parts))
}

fn parse_token_content(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    alt((
        parse_empty_token,
        parse_glue_only,
        parse_insert_text,
        parse_stitch_text,
        parse_plover_command,
        parse_simple_no_space,
        parse_host_chord,
        parse_case,
        parse_retro_case,
        parse_stitch_last_word,
        parse_mode,
        parse_literal_token,
    ))
    .parse(input)
}

fn parse_empty_token(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    if input.is_empty() {
        Ok(("", Vec::new()))
    } else {
        Err(nom::Err::Error(Error::new(input, ErrorKind::Eof)))
    }
}

fn parse_glue_only(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(glue_only, |_| vec![TranslationPart::NoSpace]).parse(input)
}

fn parse_insert_text(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(preceded(tag("&"), rest), |text: &str| {
        vec![TranslationPart::Text(text.to_string())]
    })
    .parse(input)
}

fn parse_stitch_text(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(preceded(tag(":stitch:"), rest), |text: &str| {
        vec![TranslationPart::Stitch(text.to_string())]
    })
    .parse(input)
}

fn parse_plover_command(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(preceded(tag("PLOVER:"), rest), |_| {
        vec![TranslationPart::Noop]
    })
    .parse(input)
}

fn parse_simple_no_space(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    alt((
        value(vec![TranslationPart::NoSpace], tag(">")),
        value(vec![TranslationPart::NoSpace], tag("-|")),
        value(vec![TranslationPart::NoSpace], tag("~|")),
        value(vec![TranslationPart::NoSpace], tag("-")),
    ))
    .parse(input)
}

fn parse_host_chord(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(preceded(tag("#"), parse_host_chord_expr), |(mods, key)| {
        vec![TranslationPart::HostChord { mods, key }]
    })
    .parse(input)
}

fn parse_host_chord_expr(input: &str) -> IResult<&str, (u8, u8)> {
    alt((parse_modifier_call, parse_key_usage)).parse(input)
}

fn parse_modifier_call(input: &str) -> IResult<&str, (u8, u8)> {
    let (input, name) = parse_ident(input)?;
    let Some(bits) = modifier_bits(name) else {
        return Err(nom::Err::Error(Error::new(input, ErrorKind::Tag)));
    };
    let (input, inner) =
        delimited(nom_char('('), parse_host_chord_expr, nom_char(')')).parse(input)?;
    Ok((input, (bits | inner.0, inner.1)))
}

fn parse_key_usage(input: &str) -> IResult<&str, (u8, u8)> {
    let (input, name) = parse_ident(input)?;
    let Some(key) = key_usage_id(name) else {
        return Err(nom::Err::Error(Error::new(input, ErrorKind::Tag)));
    };
    Ok((input, (0, key)))
}

fn parse_ident(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))(input)
}

fn parse_case(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(
        preceded(
            tag(":case:"),
            terminated(parse_case_mode_name, opt(nom_char(':'))),
        ),
        |mode| vec![TranslationPart::SetCase(mode)],
    )
    .parse(input)
}

fn parse_retro_case(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(
        preceded(
            tag(":retro_case:"),
            terminated(parse_case_mode_name, opt(nom_char(':'))),
        ),
        |mode| vec![TranslationPart::RetroCase(mode)],
    )
    .parse(input)
}

fn parse_stitch_last_word(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    preceded(tag(":stitch_last_word:"), parse_stitch_last_word_spec).parse(input)
}

fn parse_mode(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    map(
        preceded(
            tag("MODE:"),
            terminated(parse_mode_name, opt(nom_char(':'))),
        ),
        |mode| match mode {
            0x01 => vec![TranslationPart::SetSpace],
            0x02 => vec![TranslationPart::SetCase(CASE_CAP_FIRST_WORD)],
            0x03 => vec![TranslationPart::ResetFormatting],
            _ => vec![TranslationPart::Noop],
        },
    )
    .parse(input)
}

fn parse_stitch_last_word_spec(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    // The repeated stroke count is encoded by the outline itself; the payload
    // only needs the suffix to stitch onto the previous word.
    let (input, _stroke_count) = map_res(digit1, str::parse::<u8>).parse(input)?;
    let (input, suffix) = preceded(nom_char(':'), rest).parse(input)?;
    let suffix: &str = suffix.trim_end_matches(':');
    if suffix.is_empty() {
        return Err(nom::Err::Error(Error::new(input, ErrorKind::Tag)));
    }
    Ok(("", vec![TranslationPart::Stitch(suffix.to_string())]))
}

fn parse_case_mode_name(input: &str) -> IResult<&str, u8> {
    alt((
        value(CASE_CAP_FIRST_WORD, tag_no_case("cap_first_word")),
        value(CASE_UPPER_FIRST_WORD, tag_no_case("upper_first_word")),
        value(CASE_LOWER_FIRST_CHAR, tag_no_case("lower_first_char")),
    ))
    .parse(input)
}

fn parse_mode_name(input: &str) -> IResult<&str, u8> {
    alt((
        value(0x01, tag_no_case("SET_SPACE")),
        value(0x02, tag_no_case("CAPS")),
        value(0x03, tag_no_case("RESET")),
    ))
    .parse(input)
}

fn parse_literal_token(input: &str) -> IResult<&str, Vec<TranslationPart>> {
    let Some((needs_no_space, text)) = normalize_literal_token(input) else {
        return Err(nom::Err::Error(Error::new(input, ErrorKind::Tag)));
    };
    let mut parts = Vec::new();
    if needs_no_space {
        parts.push(TranslationPart::NoSpace);
    }
    if !text.is_empty() {
        parts.push(TranslationPart::Text(text));
    }
    Ok(("", parts))
}

fn normalize_literal_token(token: &str) -> Option<(bool, String)> {
    // Lapwing uses these forms as affix / glue shorthands. We normalize them
    // to a simple "maybe no-space, maybe literal text" pair for the runtime.
    let mut token = token;
    let mut glued = false;
    loop {
        if let Some(rest) = token.strip_prefix("~|") {
            token = rest;
            glued = true;
            continue;
        }
        if let Some(rest) = token.strip_prefix("-|") {
            token = rest;
            glued = true;
            continue;
        }
        if let Some(rest) = token.strip_prefix('>') {
            token = rest;
            glued = true;
            continue;
        }
        if let Some(rest) = token.strip_prefix('^') {
            token = rest;
            glued = true;
            continue;
        }
        break;
    }

    while let Some(rest) = token.strip_suffix('^') {
        token = rest;
        glued = true;
    }

    let mut token = strip_inline_glue(token, &mut glued);

    if token.contains('^') {
        glued = true;
        let stripped_token = token.chars().filter(|&c| c != '^').collect::<String>();
        token = stripped_token.as_str();
        if glued && token.is_empty() {
            return Some((true, String::new()));
        }
        if token.chars().all(is_literal_token_char) {
            return Some((glued, token.to_string()));
        }
        return None;
    }

    if glued && token.is_empty() {
        return Some((true, String::new()));
    }

    if token.chars().all(is_literal_token_char) {
        return Some((glued, token.to_string()));
    }

    None
}

fn strip_inline_glue<'a>(token: &'a str, glued: &mut bool) -> &'a str {
    let mut first_non_glue = 0usize;
    let mut last_non_glue = token.len();

    for (idx, ch) in token.char_indices() {
        if matches!(ch, '^' | ' ' | '\t' | '\n' | '\r') {
            *glued = true;
            if idx == first_non_glue {
                first_non_glue = idx + ch.len_utf8();
            }
            continue;
        }
        break;
    }

    for (idx, ch) in token.char_indices().rev() {
        if matches!(ch, '^' | ' ' | '\t' | '\n' | '\r') {
            *glued = true;
            last_non_glue = idx;
            continue;
        }
        break;
    }

    if first_non_glue >= last_non_glue {
        return "";
    }

    &token[first_non_glue..last_non_glue]
}

fn glue_only(input: &str) -> IResult<&str, ()> {
    if !input.is_empty()
        && input
            .chars()
            .all(|c| matches!(c, '^' | ' ' | '\t' | '\n' | '\r'))
    {
        Ok(("", ()))
    } else {
        Err(nom::Err::Error(Error::new(input, ErrorKind::Tag)))
    }
}

fn modifier_bits(name: &str) -> Option<u8> {
    let normalized = name.trim().to_ascii_lowercase().replace(['_', '-'], "");
    match normalized.as_str() {
        "shift" | "lshift" | "rshift" | "shiftl" | "shiftr" => Some(0x02),
        "control" | "ctrl" | "lctrl" | "rctrl" | "controll" | "controlr" => Some(0x01),
        "alt" | "lalt" | "ralt" => Some(0x04),
        "gui" | "win" | "cmd" | "lgui" | "rgui" | "lwin" | "rwin" | "lcmd" | "rcmd" => Some(0x08),
        _ => None,
    }
}

fn key_usage_id(name: &str) -> Option<u8> {
    let normalized = name.trim().replace(['_', '-'], "").to_ascii_uppercase();

    match normalized.as_str() {
        "A" => Some(0x04),
        "B" => Some(0x05),
        "C" => Some(0x06),
        "D" => Some(0x07),
        "E" => Some(0x08),
        "F" => Some(0x09),
        "G" => Some(0x0A),
        "H" => Some(0x0B),
        "I" => Some(0x0C),
        "J" => Some(0x0D),
        "K" => Some(0x0E),
        "L" => Some(0x0F),
        "M" => Some(0x10),
        "N" => Some(0x11),
        "O" => Some(0x12),
        "P" => Some(0x13),
        "Q" => Some(0x14),
        "R" => Some(0x15),
        "S" => Some(0x16),
        "T" => Some(0x17),
        "U" => Some(0x18),
        "V" => Some(0x19),
        "W" => Some(0x1A),
        "X" => Some(0x1B),
        "Y" => Some(0x1C),
        "Z" => Some(0x1D),
        "1" | "N1" => Some(0x1E),
        "2" | "N2" => Some(0x1F),
        "3" | "N3" => Some(0x20),
        "4" | "N4" => Some(0x21),
        "5" | "N5" => Some(0x22),
        "6" | "N6" => Some(0x23),
        "7" | "N7" => Some(0x24),
        "8" | "N8" => Some(0x25),
        "9" | "N9" => Some(0x26),
        "0" | "N0" => Some(0x27),
        "ENTER" | "RETURN" => Some(0x28),
        "ESC" | "ESCAPE" => Some(0x29),
        "BACKSPACE" | "BKSP" | "BSPC" => Some(0x2A),
        "TAB" => Some(0x2B),
        "SPACE" => Some(0x2C),
        "MINUS" => Some(0x2D),
        "EQUAL" => Some(0x2E),
        "LEFTBRACE" => Some(0x2F),
        "RIGHTBRACE" => Some(0x30),
        "BACKSLASH" => Some(0x31),
        "NONUSHASH" => Some(0x32),
        "SEMICOLON" => Some(0x33),
        "QUOTE" => Some(0x34),
        "GRAVE" => Some(0x35),
        "COMMA" => Some(0x36),
        "DOT" | "PERIOD" => Some(0x37),
        "SLASH" => Some(0x38),
        "CAPSLOCK" => Some(0x39),
        "HOME" => Some(0x4A),
        "PAGEUP" | "PRIOR" => Some(0x4B),
        "DELETE" | "DEL" => Some(0x4C),
        "END" => Some(0x4D),
        "PAGEDOWN" | "NEXT" => Some(0x4E),
        "RIGHT" => Some(0x4F),
        "LEFT" => Some(0x50),
        "DOWN" => Some(0x51),
        "UP" => Some(0x52),
        "F1" => Some(0x3A),
        "F2" => Some(0x3B),
        "F3" => Some(0x3C),
        "F4" => Some(0x3D),
        "F5" => Some(0x3E),
        "F6" => Some(0x3F),
        "F7" => Some(0x40),
        "F8" => Some(0x41),
        "F9" => Some(0x42),
        "F10" => Some(0x43),
        "F11" => Some(0x44),
        "F12" => Some(0x45),
        _ => None,
    }
}

fn push_text(out: &mut Vec<u8>, text: &str) -> Option<()> {
    if text.is_empty() {
        return Some(());
    }

    out.push(OP_TEXT);
    push_text_payload(out, text)
}

fn push_text_payload(out: &mut Vec<u8>, text: &str) -> Option<()> {
    if text.is_empty() {
        return Some(());
    }

    let len = u16::try_from(text.len()).ok()?;
    push_compact_u16(out, len);
    out.extend_from_slice(text.as_bytes());
    Some(())
}

fn push_compact_u16(out: &mut Vec<u8>, value: u16) {
    if value < 0xFF {
        out.push(value as u8);
    } else {
        out.push(0xFF);
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn is_literal_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || (c.is_ascii_punctuation() && !matches!(c, '{' | '}' | '#' | '|'))
}

const BIT_NUMBER: u8 = 0;
const BIT_LS: u8 = 1;
const BIT_LT: u8 = 2;
const BIT_K: u8 = 3;
const BIT_LP: u8 = 4;
const BIT_W: u8 = 5;
const BIT_H: u8 = 6;
const BIT_LR: u8 = 7;
const BIT_A: u8 = 8;
const BIT_O: u8 = 9;
const BIT_STAR: u8 = 10;
const BIT_E: u8 = 11;
const BIT_U: u8 = 12;
const BIT_F: u8 = 13;
const BIT_RR: u8 = 14;
const BIT_RP: u8 = 15;
const BIT_B: u8 = 16;
const BIT_L: u8 = 17;
const BIT_G: u8 = 18;
const BIT_RT: u8 = 19;
const BIT_RS: u8 = 20;
const BIT_D: u8 = 21;
const BIT_Z: u8 = 22;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_outline_handles_multi_stroke_and_hyphen() {
        let strokes = parse_outline("#STPH-FR").expect("outline should parse");
        assert_eq!(strokes.len(), 1);
        let mask = strokes[0];
        assert!(mask & (1u64 << BIT_NUMBER) != 0);
        assert!(mask & (1u64 << BIT_LS) != 0);
        assert!(mask & (1u64 << BIT_LT) != 0);
        assert!(mask & (1u64 << BIT_LP) != 0);
        assert!(mask & (1u64 << BIT_H) != 0);
        assert!(mask & (1u64 << BIT_F) != 0);
        assert!(mask & (1u64 << BIT_RR) != 0);
    }

    #[test]
    fn compile_translation_supports_plain_text_tokens() {
        let encoded = compile_translation("{^}.sh").expect("translation should compile");
        assert!(encoded.starts_with(&[OP_NO_SPACE]));
        assert!(encoded.windows(3).any(|window| window == b".sh"));
        assert!(compile_translation("{;}{:}").is_some());
        assert!(compile_translation("{&0}").is_some());
    }

    #[test]
    fn compile_translation_supports_commands() {
        assert!(compile_translation("{#Home}").is_some());
        assert!(compile_translation("{#Control_L(Shift_l(Left))}").is_some());
        assert!(compile_translation("{MODE:SET_SPACE:}").is_some());
        assert!(compile_translation("{:case:cap_first_word}").is_some());
    }

    #[test]
    fn compile_translation_supports_repeat_and_undo() {
        assert!(compile_translation("=repeat_last_translation").is_some());
        assert!(compile_translation("=undo").is_some());
    }

    #[test]
    fn compile_translation_supports_stitch_and_affixes() {
        let stitch = compile_translation("{:stitch:Z}").expect("stitch should compile");
        assert_eq!(stitch[0], OP_STITCH);
        assert!(stitch.windows(1).any(|window| window == b"Z"));

        let stitched = compile_translation("{:stitch_last_word:1:.}")
            .expect("stitch_last_word should compile");
        assert_eq!(stitched[0], OP_STITCH);
        assert!(stitched.windows(1).any(|window| window == b"."));

        assert!(compile_translation("{^alise}").is_some());
        assert!(compile_translation("{Mc^}{-|}").is_some());
    }
}
