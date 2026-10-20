#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::str_to_string, reason = "test code")]

mod common;
use common::{collect, pos, press, processor, release};

use std::vec::Vec;
use tecla::action::{Action, KeyCode, ModifierKey};
use tecla::devices::steno::{CaseMode, OP_RETRO_CASE, OP_SPACE, OP_STITCH, OP_TEXT};
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::KeyPos;
use tecla::pipeline::report::HidReport;
use tecla_config::trie::compile_steno_dictionary;

const ROWS: usize = 1;
const COLS: usize = 2;
const LAYERS: usize = 1;
const KEYS: usize = 2;

fn make_processor(dict: &'static [u8]) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
    processor(&[&[Action::Steno(0), Action::Steno(1)]])
        .with_steno(tecla::devices::steno::StenoEngine::new(dict))
}

fn usage_id_to_char(usage_id: u8, mods: u8) -> Option<String> {
    if usage_id == KeyCode::Space.usage_id() {
        return Some(" ".to_string());
    }
    if usage_id == KeyCode::Backspace.usage_id() {
        return Some("<BS>".to_string());
    }
    if (0x04..=0x1d).contains(&usage_id) {
        let base = if mods & ModifierKey::LSHIFT.bits() != 0 {
            b'A'
        } else {
            b'a'
        };
        let ch = char::from(base + (usage_id - 0x04));
        return Some(ch.to_string());
    }
    None
}

fn report_sequence(reports: &[HidReport]) -> String {
    let mut out = String::new();
    for report in reports {
        if let HidReport::Keyboard(kr) = report {
            if let Some(usage_id) = kr.keycodes.iter().copied().find(|kc| *kc != 0) {
                if let Some(chunk) = usage_id_to_char(usage_id, kr.modifiers) {
                    out.push_str(&chunk);
                }
            }
        }
    }
    out
}

fn leak_dictionary(entries: Vec<(Vec<u64>, Vec<u8>)>) -> &'static [u8] {
    Box::leak(compile_steno_dictionary(&entries).into_boxed_slice())
}

fn push_compact_len(bytes: &mut Vec<u8>, len: usize) {
    if len < 0xFF {
        bytes.push(len as u8);
    } else {
        bytes.push(0xFF);
        bytes.push((len & 0xff) as u8);
        bytes.push(((len >> 8) & 0xff) as u8);
    }
}

fn stitch_payload() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(OP_SPACE);
    bytes.push(OP_STITCH);
    push_compact_len(&mut bytes, 3);
    bytes.extend_from_slice(b"ing");
    bytes.push(OP_TEXT);
    push_compact_len(&mut bytes, 3);
    bytes.extend_from_slice(b"fox");
    bytes
}

fn word_payload(word: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(OP_TEXT);
    push_compact_len(&mut bytes, word.len());
    bytes.extend_from_slice(word.as_bytes());
    bytes
}

fn retro_case_payload() -> Vec<u8> {
    vec![OP_RETRO_CASE, CaseMode::CapFirstWord as u8]
}

fn steno_press_sequence(
    proc: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS>,
    key: KeyPos,
    start_ms: u64,
) -> std::vec::Vec<HidReport> {
    let mut reports = std::vec::Vec::new();
    reports.extend(collect(proc, press(key, start_ms), start_ms));
    reports.extend(collect(proc, release(key, start_ms + 10), start_ms + 10));
    reports
}

#[test]
fn steno_stitch_round_trip_preserves_spacing_state() {
    let dict = leak_dictionary(vec![(vec![1u64 << 0], stitch_payload())]);
    let mut proc = make_processor(dict);

    let reports = steno_press_sequence(&mut proc, pos(0, 0), 0);
    assert_eq!(report_sequence(&reports), "ing fox");
}

#[test]
fn steno_retro_case_round_trip_rewrites_previous_word() {
    let dict = leak_dictionary(vec![
        (vec![1u64 << 0], word_payload("word")),
        (vec![1u64 << 1], retro_case_payload()),
    ]);
    let mut proc = make_processor(dict);

    let mut reports = steno_press_sequence(&mut proc, pos(0, 0), 0);
    reports.extend(steno_press_sequence(&mut proc, pos(0, 1), 20));

    let sequence = report_sequence(&reports);
    assert!(sequence.contains("word"));
    assert!(sequence.contains("<BS><BS><BS><BS>Word"));
}
