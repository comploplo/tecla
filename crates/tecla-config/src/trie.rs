//! Trie compiler — builds compact trie byte arrays for autocorrect/autocomplete.
//!
//! [`compile_trie`] takes a list of `(word, frequency)` pairs and produces a
//! byte array encoding a prefix trie. The firmware's
//! The firmware's `AutocorrectEngine` and `AutocompleteEngine` walk this
//! byte array at runtime with zero allocation.
//!
//! Intended for use in build scripts — this is a `std` function, not `no_std`.

use std::collections::BTreeMap;

struct TrieNode {
    children: BTreeMap<u8, TrieNode>,
    is_word: bool,
    frequency: u8,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: BTreeMap::new(),
            is_word: false,
            frequency: 255,
        }
    }
}

/// Compiles a word list into a compact trie byte array.
///
/// Each entry is `(word, frequency)` where frequency 0 is most common
/// and 255 is rarest. Non-alpha characters cause the word to be skipped.
///
/// The output format encodes each trie node as:
/// - `child_count: u8`
/// - For each child: `keycode: u8, offset: u32 LE (4 bytes)`
/// - `is_word: u8` (0 or 1)
/// - `frequency: u8`
#[must_use]
pub fn compile_trie(words: &[(String, u8)]) -> Vec<u8> {
    let mut root = TrieNode::new();

    for (word, freq) in words {
        let keycodes: Vec<u8> = word.chars().filter_map(char_to_keycode).collect();
        if keycodes.len() != word.len() {
            continue;
        }
        let mut node = &mut root;
        for &kc in &keycodes {
            node = node.children.entry(kc).or_insert_with(TrieNode::new);
        }
        node.is_word = true;
        if *freq < node.frequency {
            node.frequency = *freq;
        }
    }

    let mut buf = Vec::new();
    serialize_node(&root, &mut buf);
    buf
}

fn serialize_node(node: &TrieNode, buf: &mut Vec<u8>) {
    let child_count = node.children.len();
    assert!(child_count <= 255, "too many children");

    let node_start = buf.len();
    buf.push(u8::try_from(child_count).unwrap_or(255));

    let children: Vec<_> = node.children.iter().collect();
    for (&keycode, _) in &children {
        buf.push(keycode);
        buf.extend_from_slice(&0u32.to_le_bytes());
    }

    buf.push(u8::from(node.is_word));
    buf.push(node.frequency);

    for (i, (_, child)) in children.iter().enumerate() {
        let child_offset = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let slot_start = node_start + 1 + i * 5 + 1;
        let offset_bytes = child_offset.to_le_bytes();
        for (j, &b) in offset_bytes.iter().enumerate() {
            if let Some(slot) = buf.get_mut(slot_start + j) {
                *slot = b;
            }
        }
        serialize_node(child, buf);
    }
}

fn char_to_keycode(c: char) -> Option<u8> {
    match c.to_ascii_lowercase() {
        'a' => Some(0x04),
        'b' => Some(0x05),
        'c' => Some(0x06),
        'd' => Some(0x07),
        'e' => Some(0x08),
        'f' => Some(0x09),
        'g' => Some(0x0A),
        'h' => Some(0x0B),
        'i' => Some(0x0C),
        'j' => Some(0x0D),
        'k' => Some(0x0E),
        'l' => Some(0x0F),
        'm' => Some(0x10),
        'n' => Some(0x11),
        'o' => Some(0x12),
        'p' => Some(0x13),
        'q' => Some(0x14),
        'r' => Some(0x15),
        's' => Some(0x16),
        't' => Some(0x17),
        'u' => Some(0x18),
        'v' => Some(0x19),
        'w' => Some(0x1A),
        'x' => Some(0x1B),
        'y' => Some(0x1C),
        'z' => Some(0x1D),
        _ => None,
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::as_conversions, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn empty_trie() {
        let trie = compile_trie(&[]);
        assert_eq!(trie.get(0).copied(), Some(0));
        assert_eq!(trie.get(1).copied(), Some(0));
        assert_eq!(trie.get(2).copied(), Some(255));
        assert_eq!(trie.len(), 3);
    }

    #[test]
    fn single_word() {
        let words = vec![(String::from("hi"), 0)];
        let trie = compile_trie(&words);
        assert_eq!(trie.get(0).copied(), Some(1));
        assert_eq!(trie.get(1).copied(), Some(0x0B));
    }

    #[test]
    fn lookup_word_in_trie() {
        let words = vec![
            (String::from("the"), 0),
            (String::from("them"), 10),
            (String::from("hi"), 5),
        ];
        let trie = compile_trie(&words);

        assert!(walk_trie(&trie, &[0x17, 0x0B, 0x08]).is_some());
        assert!(walk_trie(&trie, &[0x17, 0x0B, 0x08, 0x10]).is_some());
        assert!(walk_trie(&trie, &[0x0B, 0x0C]).is_some());
        assert!(walk_trie(&trie, &[0x17, 0x0B]).is_none());
        assert!(walk_trie(&trie, &[0x1D]).is_none());
    }

    #[test]
    fn frequency_preserved() {
        let words = vec![(String::from("the"), 0), (String::from("them"), 10)];
        let trie = compile_trie(&words);
        let (is_word, freq) = walk_trie(&trie, &[0x17, 0x0B, 0x08]).unwrap();
        assert!(is_word);
        assert_eq!(freq, 0);
        let (is_word, freq) = walk_trie(&trie, &[0x17, 0x0B, 0x08, 0x10]).unwrap();
        assert!(is_word);
        assert_eq!(freq, 10);
    }

    #[test]
    fn non_alpha_chars_skipped() {
        let words = vec![(String::from("can't"), 0)];
        let trie = compile_trie(&words);
        assert_eq!(trie.get(0).copied(), Some(0));
    }

    #[test]
    fn duplicate_word_keeps_lowest_frequency() {
        let words = vec![(String::from("the"), 50), (String::from("the"), 10)];
        let trie = compile_trie(&words);
        let (_, freq) = walk_trie(&trie, &[0x17, 0x0B, 0x08]).unwrap();
        assert_eq!(freq, 10);
    }

    fn walk_trie(trie: &[u8], keycodes: &[u8]) -> Option<(bool, u8)> {
        let mut pos = 0;
        for &kc in keycodes {
            let child_count = usize::from(*trie.get(pos)?);
            let mut found = false;
            for i in 0..child_count {
                let slot = pos + 1 + i * 5;
                if trie.get(slot).copied() == Some(kc) {
                    let b0 = u32::from(*trie.get(slot + 1)?);
                    let b1 = u32::from(*trie.get(slot + 2)?);
                    let b2 = u32::from(*trie.get(slot + 3)?);
                    let b3 = u32::from(*trie.get(slot + 4)?);
                    pos = (b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)) as usize;
                    found = true;
                    break;
                }
            }
            if !found {
                return None;
            }
        }
        let child_count = usize::from(*trie.get(pos)?);
        let flag_pos = pos + 1 + child_count * 5;
        let is_word = *trie.get(flag_pos)? == 1;
        let freq = *trie.get(flag_pos + 1)?;
        if is_word {
            Some((true, freq))
        } else {
            None
        }
    }
}
