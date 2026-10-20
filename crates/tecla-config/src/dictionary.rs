//! SCOWL dictionary downloader and compiler.
//!
//! Downloads the SCOWL 3esl word list at build time and compiles it into
//! a trie via [`crate::trie::compile_trie`].
//!
//! Enable with `features = ["dictionary"]` on `tecla-config`.
//!
//! # Usage in a build script
//!
//! ```rust,ignore
//! // build.rs
//! let trie = tecla_config::dictionary::fetch_and_compile();
//! std::fs::write(
//!     std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("dict.trie"),
//!     &trie,
//! ).unwrap();
//! ```
//!
//! Then in your crate:
//!
//! ```rust,ignore
//! static DICTIONARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dict.trie"));
//! ```

use std::io::BufRead;

const SCOWL_3ESL_URL: &str =
    "https://raw.githubusercontent.com/en-wl/wordlist/master/alt12dicts/3esl.txt";

/// Downloads the SCOWL 3esl word list and compiles it into a trie.
///
/// Returns the compiled trie as a byte vector ready to be written to a file
/// and loaded via `include_bytes!`.
#[must_use]
pub fn fetch_and_compile() -> Vec<u8> {
    fetch_and_compile_from(SCOWL_3ESL_URL)
}

/// Downloads a word list from `url` and compiles it into a trie.
///
/// Each line should contain one word. Lines starting with `#` are skipped.
/// Non-alphabetic characters are stripped. Words shorter than 2 characters
/// are excluded.
#[must_use]
#[expect(
    clippy::panic,
    reason = "build-time tool — panicking on download failure is the correct behavior"
)]
pub fn fetch_and_compile_from(url: &str) -> Vec<u8> {
    let response = ureq::get(url)
        .call()
        .unwrap_or_else(|e| panic!("failed to download dictionary from {url}: {e}"));

    let reader = std::io::BufReader::new(response.into_body().into_reader());
    let mut words: Vec<(String, u8)> = Vec::new();

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if let Some(w) = parse_line(&line) {
            if w.len() >= 2 && w.bytes().all(|b| b.is_ascii_lowercase()) {
                words.push((w, 0));
            }
        }
    }

    words.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));
    assign_frequencies(&mut words);

    crate::trie::compile_trie(&words)
}

fn assign_frequencies(words: &mut [(String, u8)]) {
    let len = words.len().max(1);
    for (i, entry) in words.iter_mut().enumerate() {
        #[expect(
            clippy::integer_division_remainder_used,
            reason = "frequency ranking requires division"
        )]
        let freq = (i * 255) / len;
        entry.1 = u8::try_from(freq).unwrap_or(255);
    }
}

fn parse_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let word_part = line.split_whitespace().next()?;
    let cleaned: String = word_part
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned)
}

#[cfg(test)]
#[expect(clippy::as_conversions, reason = "test code")]
#[expect(clippy::indexing_slicing, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn parse_line_basic() {
        assert_eq!(parse_line("hello"), Some(String::from("hello")));
    }

    #[test]
    fn parse_line_strips_nonalpha() {
        assert_eq!(parse_line("can't"), Some(String::from("cant")));
    }

    #[test]
    fn parse_line_skips_comments() {
        assert_eq!(parse_line("# comment"), None);
    }

    #[test]
    fn parse_line_skips_empty() {
        assert_eq!(parse_line(""), None);
        assert_eq!(parse_line("   "), None);
    }

    #[test]
    fn compile_produces_valid_trie() {
        let words = vec![
            (String::from("the"), 0),
            (String::from("them"), 10),
            (String::from("hello"), 5),
        ];
        let trie = crate::trie::compile_trie(&words);
        // Bytes 0..4 are the little-endian offset of the root node; the byte
        // at that offset is the root's child count.
        let root = u32::from_le_bytes([trie[0], trie[1], trie[2], trie[3]]) as usize;
        assert_eq!(trie[root], 2, "root should have 2 children (h, t)");
    }

    #[test]
    fn compiled_trie_lookups_match() {
        fn walk(trie: &[u8], keycodes: &[u8]) -> bool {
            let mut pos = u32::from_le_bytes([trie[0], trie[1], trie[2], trie[3]]) as usize;
            for &kc in keycodes {
                let cc = usize::from(*trie.get(pos).unwrap_or(&0));
                let mut found = false;
                for i in 0..cc {
                    let slot = pos + 1 + i * 5;
                    if trie.get(slot).copied() == Some(kc) {
                        let b0 = u32::from(trie[slot + 1]);
                        let b1 = u32::from(trie[slot + 2]);
                        let b2 = u32::from(trie[slot + 3]);
                        let b3 = u32::from(trie[slot + 4]);
                        pos = (b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)) as usize;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return false;
                }
            }
            let cc = usize::from(*trie.get(pos).unwrap_or(&0));
            let flag_pos = pos + 1 + cc * 5;
            trie.get(flag_pos).copied() == Some(1)
        }

        let words = vec![(String::from("cat"), 0), (String::from("dog"), 50)];
        let trie = crate::trie::compile_trie(&words);

        assert!(walk(&trie, &[0x06, 0x04, 0x17]));
        assert!(walk(&trie, &[0x07, 0x12, 0x0A]));
        assert!(!walk(&trie, &[0x06, 0x04]));
        assert!(!walk(&trie, &[0x1D]));
    }

    #[test]
    fn assign_frequencies_orders_correctly() {
        let mut words = vec![
            (String::from("aaa"), 0),
            (String::from("bbb"), 0),
            (String::from("ccc"), 0),
        ];
        assign_frequencies(&mut words);
        let freqs: Vec<u8> = words.iter().map(|w| w.1).collect();
        assert_eq!(freqs, [0, 85, 170]);
    }
}
