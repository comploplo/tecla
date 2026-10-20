//! Autocorrect engine — trie-based typo correction.
//!
//! [`AutocorrectEngine`] buffers recent keystrokes and checks them against
//! a pre-compiled dictionary trie on every word boundary. When a match is
//! found within an edit-distance threshold, it emits backspaces and the
//! corrected word. Optionally detects rollover transpositions via
//! `check_transposed()`.
//!
//! Attach it to a [`KeyboardProcessor`](crate::keyboard::KeyboardProcessor)
//! via [`with_autocorrect`](crate::keyboard::KeyboardProcessor::with_autocorrect).
//! Toggle at runtime with
//! [`Action::AutocorrectToggle`](crate::action::Action::AutocorrectToggle).
//!
//! Build the trie at compile time with `tecla_config::trie::compile_trie`.
//!
//! See also: [`autocomplete`](crate::autocomplete) for prefix completion.

use core::convert::TryFrom;
use heapless::Deque;

use crate::action::KeyCode;

/// A correction to apply: delete `backspaces` characters, then emit `replacement`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Correction {
    /// Number of backspaces to emit before the replacement.
    pub backspaces: u8,
    /// Replacement keycodes (HID usage IDs) to emit after backspaces.
    pub replacement: [u8; 32],
    /// Number of valid bytes in `replacement`.
    pub replacement_len: u8,
}

impl Correction {
    /// Returns the valid replacement slice.
    #[must_use]
    pub fn replacement_slice(&self) -> &[u8] {
        let len = usize::from(self.replacement_len).min(self.replacement.len());
        self.replacement.get(..len).unwrap_or(&[])
    }
}

/// Trie-based autocorrect engine.
///
/// Monitors recent keystrokes and checks a trie dictionary on word boundaries.
/// The dictionary is a compact trie serialized as `&'static [u8]` where each
/// node encodes: `child_count`, then for each child `keycode, offset_lo, offset_hi`,
/// then `is_word` (0 or 1), then `frequency` (0=common, 255=rare).
pub struct AutocorrectEngine {
    dictionary: &'static [u8],
    buffer: Deque<u8, 64>,
    enabled: bool,
    last_press_time: u64,
    prev_keycode: Option<u8>,
    check_transposition: bool,
    proximity: Option<&'static [[u8; 26]; 26]>,
}

impl AutocorrectEngine {
    /// Creates a new engine with the given trie dictionary.
    #[must_use]
    pub fn new(dictionary: &'static [u8]) -> Self {
        Self {
            dictionary,
            buffer: Deque::new(),
            enabled: true,
            last_press_time: 0,
            prev_keycode: None,
            check_transposition: false,
            proximity: None,
        }
    }

    /// Configures a keyboard-proximity cost table for weighted matching.
    #[must_use]
    pub fn with_proximity(mut self, table: &'static [[u8; 26]; 26]) -> Self {
        self.proximity = Some(table);
        self
    }

    /// Returns whether autocorrect is enabled.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables autocorrect.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    /// Resets the keystroke buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.prev_keycode = None;
    }

    /// Feeds a keycode to the engine, returning a correction if one matches.
    pub fn process_keycode(&mut self, kc: KeyCode, now_ms: u64) -> Option<Correction> {
        if !self.enabled {
            return None;
        }

        let uid = kc.usage_id();
        if uid == 0 {
            return None;
        }

        if is_word_boundary(uid) {
            let result = self.check_word(false);
            self.clear();
            return result;
        }

        let elapsed = now_ms.saturating_sub(self.last_press_time);
        self.check_transposition = elapsed < 30 && self.prev_keycode.is_some();

        if self.buffer.is_full() {
            self.buffer.pop_front();
        }
        let _ = self.buffer.push_back(uid);
        self.last_press_time = now_ms;
        self.prev_keycode = Some(uid);

        None
    }

    /// Checks the buffer with the last two characters swapped.
    ///
    /// Call this after `process_keycode` returns `None` on a word boundary
    /// when you want rollover transposition detection.
    #[must_use]
    pub fn check_transposed(&self) -> Option<Correction> {
        if !self.enabled || self.buffer.len() < 2 || !self.check_transposition {
            return None;
        }
        self.check_word(true)
    }

    fn check_word(&self, transposed: bool) -> Option<Correction> {
        if self.buffer.is_empty() || self.dictionary.is_empty() {
            return None;
        }

        if transposed {
            return self.check_transposed_word();
        }

        let word = self.extract_word(false);
        if word.is_empty() || trie_lookup(self.dictionary, &word) || word.len() < 3 {
            return None;
        }

        if let Some(c) = find_correction(self.dictionary, &word, self.proximity) {
            return Some(c);
        }

        self.check_suffixes(&word)
    }

    fn check_transposed_word(&self) -> Option<Correction> {
        let original = self.extract_word(false);
        let swapped = self.extract_word(true);
        if !trie_lookup(self.dictionary, &original) && trie_lookup(self.dictionary, &swapped) {
            return Some(make_correction(&swapped, original.len()));
        }
        None
    }

    fn check_suffixes(&self, word: &[u8]) -> Option<Correction> {
        for start in 1..word.len().saturating_sub(2) {
            let Some(suffix) = word.get(start..) else {
                break;
            };
            if trie_lookup(self.dictionary, suffix) {
                continue;
            }
            if suffix.len() < 3 {
                break;
            }
            if let Some(c) = find_correction(self.dictionary, suffix, self.proximity) {
                return Some(c);
            }
        }
        None
    }

    fn extract_word(&self, transposed: bool) -> heapless::Vec<u8, 64> {
        let mut word = heapless::Vec::new();
        let buf_len = self.buffer.len();
        for i in 0..buf_len {
            let val = self.get_buffer_char(i, transposed);
            let _ = word.push(val);
        }
        word
    }

    fn get_buffer_char(&self, idx: usize, transposed: bool) -> u8 {
        let buf_len = self.buffer.len();
        if transposed && buf_len >= 2 {
            let last = buf_len - 1;
            let second_last = buf_len - 2;
            if idx == last {
                return self.buffer_at(second_last);
            }
            if idx == second_last {
                return self.buffer_at(last);
            }
        }
        self.buffer_at(idx)
    }

    fn buffer_at(&self, idx: usize) -> u8 {
        for (i, &val) in self.buffer.iter().enumerate() {
            if i == idx {
                return val;
            }
        }
        0
    }
}

fn make_correction(word: &[u8], backspace_count: usize) -> Correction {
    let mut replacement = [0u8; 32];
    let rlen = word.len().min(32);
    for (dest, &src) in replacement.iter_mut().zip(word.iter()) {
        *dest = src;
    }
    Correction {
        backspaces: u8::try_from(backspace_count).unwrap_or(255),
        replacement,
        replacement_len: u8::try_from(rlen).unwrap_or(32),
    }
}

fn find_correction(
    dictionary: &[u8],
    word: &[u8],
    proximity: Option<&[[u8; 26]; 26]>,
) -> Option<Correction> {
    let mut searcher = EditSearch::new(dictionary, proximity);
    searcher.search_root(word, 2);
    let (replacement_word, _) = searcher.best?;
    Some(make_correction(&replacement_word, word.len()))
}

fn trie_lookup(trie: &[u8], word: &[u8]) -> bool {
    let Some(mut pos) = read_root_pos(trie) else {
        return false;
    };
    for &kc in word {
        let Some(child_count) = node_child_count(trie, pos) else {
            return false;
        };
        let mut found = false;
        for i in 0..child_count {
            let Some(slot) = child_slot(pos, i) else {
                continue;
            };
            if trie.get(slot).copied() == Some(kc) {
                let Some(next) = read_offset(trie, slot) else {
                    return false;
                };
                pos = next;
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    is_word_at(trie, pos)
}

fn read_root_pos(trie: &[u8]) -> Option<usize> {
    let bytes = trie.get(0..4)?;
    let root = u32::from_le_bytes(bytes.try_into().ok()?);
    usize::try_from(root).ok()
}

fn read_offset(trie: &[u8], slot: usize) -> Option<usize> {
    let start = slot.checked_add(1)?;
    let end = start.checked_add(4)?;
    let bytes = trie.get(start..end)?;
    let offset = u32::from_le_bytes(bytes.try_into().ok()?);
    usize::try_from(offset).ok()
}

fn node_child_count(trie: &[u8], pos: usize) -> Option<usize> {
    trie.get(pos).copied().map(usize::from)
}

fn node_flag_pos(pos: usize, child_count: usize) -> Option<usize> {
    let children_len = child_count.checked_mul(5)?;
    pos.checked_add(1)?.checked_add(children_len)
}

fn child_slot(pos: usize, i: usize) -> Option<usize> {
    let offset = i.checked_mul(5)?;
    pos.checked_add(1)?.checked_add(offset)
}

fn is_word_at(trie: &[u8], pos: usize) -> bool {
    let Some(child_count) = node_child_count(trie, pos) else {
        return false;
    };
    let Some(flag_pos) = node_flag_pos(pos, child_count) else {
        return false;
    };
    trie.get(flag_pos).copied() == Some(1)
}

fn freq_at(trie: &[u8], pos: usize) -> u8 {
    let Some(child_count) = node_child_count(trie, pos) else {
        return 255;
    };
    let Some(flag_pos) = node_flag_pos(pos, child_count) else {
        return 255;
    };
    flag_pos
        .checked_add(1)
        .and_then(|next| trie.get(next))
        .copied()
        .unwrap_or(255)
}

fn child_at(trie: &[u8], pos: usize, i: usize) -> Option<(u8, usize)> {
    let slot = child_slot(pos, i)?;
    let edge_kc = trie.get(slot).copied()?;
    Some((edge_kc, read_offset(trie, slot)?))
}

const HID_A: u8 = 0x04;

fn letter_index(usage_id: u8) -> Option<usize> {
    if (HID_A..(HID_A + 26)).contains(&usage_id) {
        Some(usize::from(usage_id - HID_A))
    } else {
        None
    }
}

struct EditSearch<'a> {
    trie: &'a [u8],
    proximity: Option<&'a [[u8; 26]; 26]>,
    current: heapless::Vec<u8, 32>,
    proximity_cost: u16,
    best: Option<(heapless::Vec<u8, 32>, u16)>,
}

impl<'a> EditSearch<'a> {
    fn new(trie: &'a [u8], proximity: Option<&'a [[u8; 26]; 26]>) -> Self {
        Self {
            trie,
            proximity,
            current: heapless::Vec::new(),
            proximity_cost: 0,
            best: None,
        }
    }

    fn search_root(&mut self, word: &[u8], max_dist: u8) {
        if let Some(root_pos) = read_root_pos(self.trie) {
            self.search(root_pos, word, max_dist);
        }
    }

    fn search(&mut self, pos: usize, remaining: &[u8], max_dist: u8) {
        let Some(child_count) = node_child_count(self.trie, pos) else {
            return;
        };

        if remaining.is_empty() {
            self.handle_leaf(pos, child_count, max_dist);
            return;
        }

        let Some((&target, rest)) = remaining.split_first() else {
            return;
        };

        for i in 0..child_count {
            let Some((edge_kc, child_pos)) = child_at(self.trie, pos, i) else {
                continue;
            };
            self.try_match(edge_kc, child_pos, target, rest, max_dist);
            if max_dist > 0 {
                self.try_substitute(edge_kc, child_pos, target, rest, max_dist);
                self.try_insert(edge_kc, child_pos, remaining, max_dist);
                self.try_transpose(pos, child_count, edge_kc, target, remaining, max_dist);
            }
        }

        if max_dist > 0 {
            self.proximity_cost += 255;
            self.search(pos, rest, max_dist - 1);
            self.proximity_cost -= 255;
        }
    }

    fn handle_leaf(&mut self, pos: usize, cc: usize, max_dist: u8) {
        if is_word_at(self.trie, pos) {
            self.update_best(freq_at(self.trie, pos));
        }
        if max_dist > 0 {
            self.proximity_cost += 255;
            for i in 0..cc {
                let Some((edge_kc, child_pos)) = child_at(self.trie, pos, i) else {
                    continue;
                };
                if self.current.push(edge_kc).is_ok() {
                    self.search(child_pos, &[], max_dist - 1);
                    self.current.truncate(self.current.len() - 1);
                }
            }
            self.proximity_cost -= 255;
        }
    }

    fn try_match(&mut self, edge_kc: u8, child_pos: usize, target: u8, rest: &[u8], max_dist: u8) {
        if edge_kc == target && self.current.push(edge_kc).is_ok() {
            self.search(child_pos, rest, max_dist);
            self.current.truncate(self.current.len() - 1);
        }
    }

    fn try_substitute(
        &mut self,
        edge_kc: u8,
        child_pos: usize,
        target: u8,
        rest: &[u8],
        max_dist: u8,
    ) {
        if edge_kc != target {
            let pcost = self.substitution_cost(target, edge_kc);
            self.proximity_cost += pcost;
            if self.current.push(edge_kc).is_ok() {
                self.search(child_pos, rest, max_dist - 1);
                self.current.truncate(self.current.len() - 1);
            }
            self.proximity_cost -= pcost;
        }
    }

    fn substitution_cost(&self, a: u8, b: u8) -> u16 {
        if let Some(table) = self.proximity {
            if let (Some(ai), Some(bi)) = (letter_index(a), letter_index(b)) {
                return table
                    .get(ai)
                    .and_then(|row| row.get(bi))
                    .copied()
                    .map(u16::from)
                    .unwrap_or(255);
            }
        }
        255
    }

    fn try_insert(&mut self, edge_kc: u8, child_pos: usize, remaining: &[u8], max_dist: u8) {
        self.proximity_cost += 255;
        if self.current.push(edge_kc).is_ok() {
            self.search(child_pos, remaining, max_dist - 1);
            self.current.truncate(self.current.len() - 1);
        }
        self.proximity_cost -= 255;
    }

    fn try_transpose(
        &mut self,
        pos: usize,
        cc: usize,
        edge_kc: u8,
        target: u8,
        remaining: &[u8],
        max_dist: u8,
    ) {
        let rest = remaining.get(1..).unwrap_or(&[]);
        if rest.first().copied() != Some(edge_kc) {
            return;
        }
        let rest2 = remaining.get(2..).unwrap_or(&[]);
        let pcost = self.substitution_cost(target, edge_kc);
        self.proximity_cost += pcost;
        for j in 0..cc {
            let Some((edge_kc2, child_pos2)) = child_at(self.trie, pos, j) else {
                continue;
            };
            if edge_kc2 == target {
                let base_len = self.current.len();
                if self.current.push(edge_kc).is_ok() && self.current.push(edge_kc2).is_ok() {
                    self.search(child_pos2, rest2, max_dist - 1);
                }
                self.current.truncate(base_len);
            }
        }
        self.proximity_cost -= pcost;
    }

    fn update_best(&mut self, freq: u8) {
        let score = self.proximity_cost.saturating_add(u16::from(freq));
        match &self.best {
            Some((_, best_score)) => {
                if score < *best_score {
                    self.best = Some((self.current.clone(), score));
                }
            }
            None => {
                self.best = Some((self.current.clone(), score));
            }
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
    fn known_word_no_correction() {
        let dict = leaked_trie(&[("the", 0), ("teh", 5)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::T, 0);
        engine.process_keycode(KeyCode::H, 10);
        engine.process_keycode(KeyCode::E, 20);
        assert!(engine.process_keycode(KeyCode::Space, 30).is_none());
    }

    #[test]
    fn unknown_word_gets_corrected() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::T, 0);
        engine.process_keycode(KeyCode::E, 10);
        engine.process_keycode(KeyCode::H, 20);
        let result = engine.process_keycode(KeyCode::Space, 30);
        let c = result.unwrap();
        assert_eq!(c.backspaces, 3);
        assert_eq!(c.replacement_slice(), &[0x17, 0x0B, 0x08]);
    }

    #[test]
    fn empty_dictionary_never_corrects() {
        let mut engine = AutocorrectEngine::new(&[]);
        engine.process_keycode(KeyCode::T, 0);
        engine.process_keycode(KeyCode::E, 10);
        engine.process_keycode(KeyCode::H, 20);
        assert!(engine.process_keycode(KeyCode::Space, 30).is_none());
    }

    #[test]
    fn toggling_off_prevents_corrections() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.set_enabled(false);
        assert!(!engine.enabled());
        engine.process_keycode(KeyCode::T, 0);
        engine.process_keycode(KeyCode::E, 10);
        engine.process_keycode(KeyCode::H, 20);
        assert!(engine.process_keycode(KeyCode::Space, 30).is_none());
    }

    #[test]
    fn clear_resets_state() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::T, 0);
        engine.process_keycode(KeyCode::E, 10);
        engine.clear();
        engine.process_keycode(KeyCode::H, 20);
        assert!(engine.process_keycode(KeyCode::Space, 30).is_none());
    }

    #[test]
    fn new_engine_is_enabled() {
        let engine = AutocorrectEngine::new(&[]);
        assert!(engine.enabled());
    }

    #[test]
    fn set_enabled_toggle() {
        let mut engine = AutocorrectEngine::new(&[]);
        engine.set_enabled(false);
        assert!(!engine.enabled());
        engine.set_enabled(true);
        assert!(engine.enabled());
    }

    #[test]
    fn none_keycode_is_ignored() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        assert!(engine.process_keycode(KeyCode::None, 0).is_none());
    }

    #[test]
    fn word_boundary_enter_triggers_check() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::T, 0);
        engine.process_keycode(KeyCode::E, 10);
        engine.process_keycode(KeyCode::H, 20);
        let result = engine.process_keycode(KeyCode::Enter, 30);
        let c = result.unwrap();
        assert_eq!(c.backspaces, 3);
        assert_eq!(c.replacement_slice(), &[0x17, 0x0B, 0x08]); // t, h, e
    }

    #[test]
    fn check_transposed_with_fast_keys() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::T, 0);
        engine.process_keycode(KeyCode::E, 10);
        engine.process_keycode(KeyCode::H, 19);
        let result = engine.check_transposed();
        let c = result.unwrap();
        assert_eq!(c.backspaces, 3);
        assert_eq!(c.replacement_slice(), &[0x17, 0x0B, 0x08]); // t, h, e
    }

    #[test]
    fn check_transposed_outside_threshold() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::T, 0);
        engine.process_keycode(KeyCode::E, 10);
        engine.process_keycode(KeyCode::H, 100);
        assert!(engine.check_transposed().is_none());
    }

    #[test]
    fn check_transposed_disabled_engine() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.set_enabled(false);
        assert!(engine.check_transposed().is_none());
    }

    #[test]
    fn check_transposed_empty_buffer() {
        let dict = leaked_trie(&[("the", 0)]);
        let engine = AutocorrectEngine::new(dict);
        assert!(engine.check_transposed().is_none());
    }

    #[test]
    fn buffer_wraps_at_capacity() {
        let dict = leaked_trie(&[("the", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        for i in 0..70u64 {
            engine.process_keycode(KeyCode::A, i * 100);
        }
        engine.process_keycode(KeyCode::T, 7000);
        engine.process_keycode(KeyCode::E, 7010);
        engine.process_keycode(KeyCode::H, 7020);
        let result = engine.process_keycode(KeyCode::Space, 7030);
        let c = result.unwrap();
        assert_eq!(c.backspaces, 3);
        assert_eq!(c.replacement_slice(), &[0x17, 0x0B, 0x08]); // t, h, e
    }

    #[test]
    fn trie_lookup_valid_word() {
        let trie = build_test_trie(&[("hello", 0), ("help", 5)]);
        assert!(trie_lookup(&trie, &[0x0B, 0x08, 0x0F, 0x0F, 0x12]));
        assert!(trie_lookup(&trie, &[0x0B, 0x08, 0x0F, 0x13]));
    }

    #[test]
    fn trie_lookup_invalid_word() {
        let trie = build_test_trie(&[("hello", 0)]);
        assert!(!trie_lookup(&trie, &[0x0B, 0x08, 0x0F]));
        assert!(!trie_lookup(&trie, &[0x1D, 0x1D]));
    }

    #[test]
    fn trie_lookup_empty() {
        let trie = build_test_trie(&[]);
        assert!(!trie_lookup(&trie, &[0x04]));
    }

    #[test]
    fn truncated_trie_lookup_returns_false() {
        assert!(!trie_lookup(&[0, 0, 0, 0, 1], &[0x04]));
    }

    #[test]
    fn invalid_offset_trie_lookup_returns_false() {
        let trie = [
            4,
            0,
            0,
            0,
            1,
            KeyCode::A.usage_id(),
            255,
            255,
            255,
            255,
            0,
            255,
        ];
        assert!(!trie_lookup(&trie, &[KeyCode::A.usage_id()]));
    }

    #[test]
    fn correction_prefers_lower_frequency() {
        let dict = leaked_trie(&[("cat", 0), ("bat", 50), ("hat", 100)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::D, 0);
        engine.process_keycode(KeyCode::A, 10);
        engine.process_keycode(KeyCode::T, 20);
        let result = engine.process_keycode(KeyCode::Space, 30);
        let c = result.unwrap();
        assert_eq!(c.replacement_slice(), &[0x06, 0x04, 0x17]);
    }

    #[test]
    fn single_word_trie() {
        let dict = leaked_trie(&[("a", 0)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::A, 0);
        assert!(engine.process_keycode(KeyCode::Space, 10).is_none());
    }

    #[test]
    #[expect(clippy::indexing_slicing, reason = "test code with known bounds")]
    fn proximity_prefers_adjacent_key_correction() {
        let dict = leaked_trie(&[("sat", 50), ("pat", 50)]);

        let mut table = [[255u8; 26]; 26];
        for (i, row) in table.iter_mut().enumerate() {
            row[i] = 0;
        }
        let d_idx = usize::from(KeyCode::D.usage_id() - 0x04);
        let s_idx = usize::from(KeyCode::S.usage_id() - 0x04);
        let p_idx = usize::from(KeyCode::P.usage_id() - 0x04);
        table[d_idx][s_idx] = 20;
        table[s_idx][d_idx] = 20;
        table[d_idx][p_idx] = 200;
        table[p_idx][d_idx] = 200;

        let table_ref: &'static [[u8; 26]; 26] = std::boxed::Box::leak(std::boxed::Box::new(table));
        let mut engine = AutocorrectEngine::new(dict).with_proximity(table_ref);

        engine.process_keycode(KeyCode::D, 0);
        engine.process_keycode(KeyCode::A, 10);
        engine.process_keycode(KeyCode::T, 20);
        let result = engine.process_keycode(KeyCode::Space, 30);
        let c = result.unwrap();
        let expected = &[
            KeyCode::S.usage_id(),
            KeyCode::A.usage_id(),
            KeyCode::T.usage_id(),
        ];
        assert_eq!(c.replacement_slice(), expected);
    }

    #[test]
    fn without_proximity_falls_back_to_frequency() {
        let dict = leaked_trie(&[("sat", 50), ("pat", 10)]);
        let mut engine = AutocorrectEngine::new(dict);
        engine.process_keycode(KeyCode::D, 0);
        engine.process_keycode(KeyCode::A, 10);
        engine.process_keycode(KeyCode::T, 20);
        let result = engine.process_keycode(KeyCode::Space, 30);
        let c = result.unwrap();
        let expected = &[
            KeyCode::P.usage_id(),
            KeyCode::A.usage_id(),
            KeyCode::T.usage_id(),
        ];
        assert_eq!(c.replacement_slice(), expected);
    }
}
