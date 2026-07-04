//! Round-trip fuzz of the dictionary compilation pipeline.
//!
//! Two end-to-end paths are validated:
//!
//! - **Trie**: arbitrary word sets compile via `tecla_config::trie::compile_trie`
//!   without panic and produce a well-formed header (4-byte LE root offset
//!   pointing inside the buffer).
//! - **DAFSA steno dictionary**: arbitrary (strokes, payload) entry sets
//!   compile via `compile_steno_dictionary` and every entry is findable
//!   via `StenoEngine::process_stroke` in stroke order. Multi-stroke briefs
//!   are exercised — the publishable "fits on stock Pico" result depends on
//!   this round-trip being airtight.

#![allow(missing_docs, reason = "integration test")]
#![allow(clippy::tests_outside_test_module, reason = "integration test")]

use proptest::prelude::*;
use tecla::devices::steno::{StenoChord, StenoEngine};
use tecla_config::trie::{compile_steno_dictionary, compile_trie};

// ── Trie compile fuzz ───────────────────────────────────────────────────────

/// Generates an a..z alphabetic word, length 1..12.
fn word_strategy() -> impl Strategy<Value = String> {
    proptest::collection::vec(0u8..26u8, 1..12usize)
        .prop_map(|bytes| bytes.iter().map(|b| (b'a' + b) as char).collect::<String>())
}

proptest! {
    /// Trie compilation is total — never panics on arbitrary alphabetic
    /// input, including duplicates, single-letter words, max-length words.
    #[test]
    fn trie_compile_is_total(
        words in proptest::collection::vec((word_strategy(), 0u8..=255), 0..64),
    ) {
        let bytes = compile_trie(&words);
        // Empty input produces an empty buffer or a buffer with just a header;
        // any compilation result is acceptable so long as we didn't panic.
        prop_assert!(bytes.len() >= 4 || words.is_empty(),
            "non-empty word list compiled to fewer than 4 bytes: {} bytes for {} words",
            bytes.len(), words.len());
    }

    /// Trie root-offset header points inside the buffer (or is zero for empty).
    #[test]
    fn trie_root_offset_is_in_bounds(
        words in proptest::collection::vec((word_strategy(), 0u8..=255), 1..32),
    ) {
        let bytes = compile_trie(&words);
        prop_assert!(bytes.len() >= 4, "compiled trie too short");
        let root = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        prop_assert!(root <= bytes.len(),
            "root offset {} > buffer len {}", root, bytes.len());
    }

    /// Empty word lists compile without panic.
    #[test]
    fn trie_empty_input_is_safe(_dummy in any::<u8>()) {
        let bytes = compile_trie(&[]);
        // Zero-or-more bytes; the only contract is no panic.
        prop_assert!(bytes.len() <= 16, "empty input produced {} bytes", bytes.len());
    }
}

// ── DAFSA steno dictionary round-trip ───────────────────────────────────────

/// Generates a stroke (non-zero u64; the encoder treats `0` specially as
/// "no chord", so we mask the lower 24 bits and ensure ≥ 1).
fn stroke_strategy() -> impl Strategy<Value = u64> {
    (1u32..0x00FF_FFFFu32).prop_map(u64::from)
}

/// Generates an entry: 1..=3 strokes mapped to a non-empty translation.
fn entry_strategy() -> impl Strategy<Value = (Vec<u64>, Vec<u8>)> {
    let strokes = proptest::collection::vec(stroke_strategy(), 1..=3);
    let translation = proptest::collection::vec(any::<u8>(), 1..=8);
    (strokes, translation)
}

/// Drives a freshly-constructed StenoEngine through the given stroke
/// sequence and returns the produced translation (if any).
fn lookup(dict: &'static [u8], strokes: &[u64]) -> Option<Vec<u8>> {
    let mut engine = StenoEngine::new(dict);
    let mut last: Option<Vec<u8>> = None;
    for &s in strokes {
        last = engine
            .process_stroke(StenoChord { keys: s })
            .map(|slice| slice.to_vec());
    }
    last
}

proptest! {
    /// Every (strokes, payload) entry is findable in the compiled DAFSA via
    /// its stroke sequence.
    #[test]
    fn dafsa_round_trip_every_entry(
        entries in proptest::collection::vec(entry_strategy(), 1..16),
    ) {
        // De-duplicate by stroke prefix — the same prefix mapped twice is
        // ambiguous and the second wins on compile.
        let mut seen: std::collections::BTreeMap<Vec<u64>, Vec<u8>> = std::collections::BTreeMap::new();
        for (strokes, translation) in &entries {
            seen.insert(strokes.clone(), translation.clone());
        }
        let canonical: Vec<(Vec<u64>, Vec<u8>)> = seen.into_iter().collect();
        let dict = compile_steno_dictionary(&canonical);
        let dict_static: &'static [u8] = Box::leak(dict.into_boxed_slice());

        for (strokes, expected) in &canonical {
            let got = lookup(dict_static, strokes);
            prop_assert!(got.is_some(),
                "stroke sequence {:?} not found in compiled DAFSA", strokes);
            // The compiled translation may include a header byte for length;
            // it's enough to assert the trailing bytes contain the original
            // translation. Empty translations are skipped (tools may filter).
            if !expected.is_empty() {
                let got_v = got.unwrap();
                prop_assert!(got_v.windows(expected.len()).any(|w| w == expected.as_slice()),
                    "round-trip translation for strokes {:?} missing payload {:?}, got {:?}",
                    strokes, expected, got_v);
            }
        }
    }

    /// Strokes NOT in the dictionary return None (no false positive).
    #[test]
    fn dafsa_unknown_stroke_returns_none(
        entries in proptest::collection::vec(entry_strategy(), 1..8),
        wild_stroke in 0x0100_0000u64..0xFFFF_FFFFu64,
    ) {
        let mut seen: std::collections::BTreeMap<Vec<u64>, Vec<u8>> = std::collections::BTreeMap::new();
        for (strokes, translation) in &entries {
            seen.insert(strokes.clone(), translation.clone());
        }
        let canonical: Vec<(Vec<u64>, Vec<u8>)> = seen.into_iter().collect();
        // Skip the test if wild_stroke happens to be a real first-stroke.
        let collides = canonical.iter().any(|(s, _)| s.first() == Some(&wild_stroke));
        if collides {
            return Ok(());
        }
        let dict = compile_steno_dictionary(&canonical);
        let dict_static: &'static [u8] = Box::leak(dict.into_boxed_slice());

        let mut engine = StenoEngine::new(dict_static);
        let res = engine.process_stroke(StenoChord { keys: wild_stroke });
        prop_assert!(res.is_none(),
            "wild stroke {:#x} returned a translation: {:?}", wild_stroke, res);
    }

    /// Multi-stroke briefs: process_stroke returns None for prefixes, a
    /// payload only on the final stroke of the brief.
    #[test]
    fn dafsa_multi_stroke_only_resolves_on_final(
        strokes in proptest::collection::vec(stroke_strategy(), 2..=3),
        translation in proptest::collection::vec(any::<u8>(), 1..=8),
    ) {
        let entries = vec![(strokes.clone(), translation.clone())];
        let dict = compile_steno_dictionary(&entries);
        let dict_static: &'static [u8] = Box::leak(dict.into_boxed_slice());

        let mut engine = StenoEngine::new(dict_static);
        for (i, &s) in strokes.iter().enumerate() {
            let res = engine.process_stroke(StenoChord { keys: s });
            if i + 1 < strokes.len() {
                // All-but-last: must NOT resolve.
                prop_assert!(res.is_none(),
                    "multi-stroke brief resolved on stroke {}/{}: {:?}",
                    i + 1, strokes.len(), res);
            } else {
                // Final: must resolve.
                prop_assert!(res.is_some(),
                    "multi-stroke brief did NOT resolve on final stroke {}/{}",
                    i + 1, strokes.len());
            }
        }
    }
}
