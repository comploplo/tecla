//! Autocomplete engine — trie-based prefix completion.
//!
//! [`AutocompleteEngine`] walks a pre-compiled trie as the user types,
//! offering frequency-ranked completions. Attach it to a
//! [`KeyboardProcessor`](crate::keyboard::KeyboardProcessor) via
//! [`with_autocomplete`](crate::keyboard::KeyboardProcessor::with_autocomplete).
//! The user accepts a suggestion with
//! [`Action::AutocompleteAccept`](crate::action::Action::AutocompleteAccept).
//!
//! Build the trie at compile time with `tecla_config::trie::compile_trie`.
//!
//! See also: [`autocorrect`](crate::autocorrect) for typo correction.

use heapless::Vec;

use crate::action::KeyCode;

/// Autocomplete engine backed by a trie dictionary.
///
/// The dictionary is a compact trie serialized as `&'static [u8]` where each
/// node encodes: `child_count`, then for each child `keycode, offset_lo, offset_hi`,
/// then `is_word` (0 or 1), then `frequency` (0=common, 255=rare).
pub struct AutocompleteEngine {
    dictionary: &'static [u8],
    buffer: Vec<u8, 32>,
    enabled: bool,
    suggestion: Vec<u8, 32>,
}

impl AutocompleteEngine {
    /// Creates a new engine with the given trie dictionary.
    #[must_use]
    pub fn new(dictionary: &'static [u8]) -> Self {
        Self {
            dictionary,
            buffer: Vec::new(),
            enabled: true,
            suggestion: Vec::new(),
        }
    }

    /// Returns whether autocomplete is enabled.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables autocomplete.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.suggestion.clear();
        }
    }

    /// Feeds a keycode, updating the current suggestion.
    #[expect(
        clippy::unused_result_ok,
        reason = "heapless push overflow silently dropped in no_std"
    )]
    pub fn process_keycode(&mut self, kc: KeyCode) {
        if !self.enabled {
            return;
        }

        let uid = kc.usage_id();
        if uid == 0 {
            return;
        }

        if is_word_boundary(uid) {
            self.clear();
            return;
        }

        self.buffer.push(uid).ok();
        self.update_suggestion();
    }

    /// Accepts the current suggestion, returning the remaining keycodes.
    #[expect(
        clippy::unused_result_ok,
        reason = "heapless push overflow silently dropped in no_std"
    )]
    pub fn accept(&mut self) -> Option<Vec<u8, 32>> {
        if self.suggestion.is_empty() {
            return None;
        }
        let result = self.suggestion.clone();
        let mut new_buf = self.buffer.clone();
        for &b in &result {
            new_buf.push(b).ok();
        }
        self.buffer = new_buf;
        self.suggestion.clear();
        self.update_suggestion();
        Some(result)
    }

    /// Resets the word buffer and clears the suggestion.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.suggestion.clear();
    }

    /// Returns the current suggestion without accepting it.
    #[must_use]
    pub fn suggestion(&self) -> Option<&[u8]> {
        if self.suggestion.is_empty() {
            None
        } else {
            Some(&self.suggestion)
        }
    }

    fn update_suggestion(&mut self) {
        self.suggestion.clear();

        if self.buffer.is_empty() || self.dictionary.is_empty() {
            return;
        }

        let Some(prefix_pos) = walk_to_prefix(self.dictionary, self.buffer.as_slice()) else {
            return;
        };

        let mut best_suffix: Vec<u8, 32> = Vec::new();
        let mut best_freq: u8 = 255;
        let mut found = false;
        let mut path: Vec<u8, 32> = Vec::new();

        find_best_completion(
            self.dictionary,
            prefix_pos,
            &mut path,
            &mut best_suffix,
            &mut best_freq,
            &mut found,
        );

        if found && !best_suffix.is_empty() {
            self.suggestion = best_suffix;
        }
    }
}

#[expect(
    clippy::as_conversions,
    reason = "u8 child count to usize for iteration"
)]
fn walk_to_prefix(trie: &[u8], prefix: &[u8]) -> Option<usize> {
    let mut pos = 0;
    for &kc in prefix {
        let child_count = *trie.get(pos)? as usize;
        let mut found = false;
        for i in 0..child_count {
            let slot = pos + 1 + i * 5;
            if trie.get(slot).copied() == Some(kc) {
                pos = read_offset_u32(trie, slot)?;
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    Some(pos)
}

#[expect(clippy::as_conversions, reason = "u32 trie offset to usize")]
fn read_offset_u32(trie: &[u8], slot: usize) -> Option<usize> {
    let b0 = u32::from(*trie.get(slot + 1)?);
    let b1 = u32::from(*trie.get(slot + 2)?);
    let b2 = u32::from(*trie.get(slot + 3)?);
    let b3 = u32::from(*trie.get(slot + 4)?);
    Some((b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)) as usize)
}

#[expect(
    clippy::as_conversions,
    reason = "u8 child count to usize for iteration"
)]
fn find_best_completion(
    trie: &[u8],
    pos: usize,
    path: &mut Vec<u8, 32>,
    best: &mut Vec<u8, 32>,
    best_freq: &mut u8,
    found: &mut bool,
) {
    let Some(&child_count) = trie.get(pos) else {
        return;
    };
    let cc = child_count as usize;
    let flag_pos = pos + 1 + cc * 5;
    let is_word = trie.get(flag_pos).copied() == Some(1);
    let freq = trie.get(flag_pos + 1).copied().unwrap_or(255);

    if is_word && !path.is_empty() && (freq < *best_freq || !*found) {
        *best = path.clone();
        *best_freq = freq;
        *found = true;
    }

    for i in 0..cc {
        let slot = pos + 1 + i * 5;
        let Some(&edge_kc) = trie.get(slot) else {
            continue;
        };
        let Some(child_pos) = read_offset_u32(trie, slot) else {
            continue;
        };

        if path.push(edge_kc).is_ok() {
            find_best_completion(trie, child_pos, path, best, best_freq, found);
            path.truncate(path.len() - 1);
        }
    }
}

fn is_word_boundary(uid: u8) -> bool {
    uid == KeyCode::Space.usage_id()
        || uid == KeyCode::Enter.usage_id()
        || uid == KeyCode::Tab.usage_id()
        || uid == KeyCode::Escape.usage_id()
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    extern crate std;

    use super::*;
    fn build_test_trie(words: &[(&str, u8)]) -> std::vec::Vec<u8> {
        tecla_config::trie::compile_trie(
            &words
                .iter()
                .map(|(w, f)| (std::string::String::from(*w), *f))
                .collect::<std::vec::Vec<_>>(),
        )
    }

    fn leaked_trie(words: &[(&str, u8)]) -> &'static [u8] {
        let trie = build_test_trie(words);
        let boxed: std::boxed::Box<[u8]> = trie.into_boxed_slice();
        std::boxed::Box::leak(boxed)
    }

    #[test]
    fn typing_th_suggests_the() {
        let dict = leaked_trie(&[("the", 0), ("their", 10), ("there", 20)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        let s = engine.suggestion();
        assert!(s.is_some());
        assert_eq!(s.unwrap(), &[0x08]);
    }

    #[test]
    fn typing_the_suggests_their() {
        let dict = leaked_trie(&[("the", 0), ("their", 10), ("there", 20)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        engine.process_keycode(KeyCode::E);
        let s = engine.suggestion();
        assert!(s.is_some());
        assert_eq!(s.unwrap(), &[0x0C, 0x15]);
    }

    #[test]
    fn accept_returns_remaining_keycodes() {
        let dict = leaked_trie(&[("the", 0), ("their", 10)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        let result = engine.accept();
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_slice(), &[0x08]);
    }

    #[test]
    fn accept_with_no_suggestion_returns_none() {
        let mut engine = AutocompleteEngine::new(&[]);
        engine.process_keycode(KeyCode::Z);
        assert!(engine.accept().is_none());
    }

    #[test]
    fn toggling_off_clears_suggestion() {
        let dict = leaked_trie(&[("the", 0), ("their", 10)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        assert!(engine.suggestion().is_some());
        engine.set_enabled(false);
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn clear_resets_on_word_boundary() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        engine.clear();
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn space_clears_buffer() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        engine.process_keycode(KeyCode::Space);
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn new_engine_is_enabled() {
        let engine = AutocompleteEngine::new(&[]);
        assert!(engine.enabled());
    }

    #[test]
    fn set_enabled_toggle() {
        let mut engine = AutocompleteEngine::new(&[]);
        engine.set_enabled(false);
        assert!(!engine.enabled());
        engine.set_enabled(true);
        assert!(engine.enabled());
    }

    #[test]
    fn disabled_engine_ignores_input() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.set_enabled(false);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn none_keycode_is_ignored() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::None);
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn no_match_gives_no_suggestion() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::Z);
        engine.process_keycode(KeyCode::Z);
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn empty_dictionary_gives_no_suggestion() {
        let mut engine = AutocompleteEngine::new(&[]);
        engine.process_keycode(KeyCode::T);
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn accept_updates_buffer_for_further_suggestions() {
        let dict = leaked_trie(&[("the", 0), ("their", 10), ("there", 20)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        let _ = engine.accept();
        let s = engine.suggestion();
        assert!(s.is_some());
        assert_eq!(s.unwrap(), &[0x0C, 0x15]);
    }

    #[test]
    fn exact_match_without_longer_word_gives_no_suggestion() {
        let dict = leaked_trie(&[("th", 0)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn frequency_picks_best_completion() {
        let dict = leaked_trie(&[("there", 50), ("their", 5)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::T);
        engine.process_keycode(KeyCode::H);
        engine.process_keycode(KeyCode::E);
        let s = engine.suggestion();
        assert!(s.is_some());
        assert_eq!(s.unwrap(), &[0x0C, 0x15]);
    }

    #[test]
    fn single_word_trie() {
        let dict = leaked_trie(&[("hello", 0)]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::H);
        let s = engine.suggestion();
        assert!(s.is_some());
        assert_eq!(s.unwrap(), &[0x08, 0x0F, 0x0F, 0x12]);
    }

    #[test]
    fn empty_trie_no_crash() {
        let dict = leaked_trie(&[]);
        let mut engine = AutocompleteEngine::new(dict);
        engine.process_keycode(KeyCode::A);
        assert!(engine.suggestion().is_none());
    }
}
