//! Trie compiler — builds compact trie byte arrays for autocorrect/autocomplete.
//!
//! [`compile_trie`] takes a list of `(word, frequency)` pairs and produces a
//! byte array encoding a prefix trie. The firmware's `AutocorrectEngine` and
//! `AutocompleteEngine` walk this byte array at runtime with zero allocation.
//!
//! This version implements DAFSA (Deterministic Acyclic Finite State Automaton)
//! by merging identical subtrees during serialization, significantly reducing
//! flash usage for large dictionaries.
//!
//! Intended for use in build scripts — this is a `std` function, not `no_std`.

use std::collections::{BTreeMap, HashMap};

#[derive(Clone, PartialEq, Eq, Hash)]
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

/// Compiles a word list into a compact DAFSA byte array.
///
/// Each entry is `(word, frequency)` where frequency 0 is most common
/// and 255 is rarest. Non-alpha characters cause the word to be skipped.
///
/// The output format encodes the root offset as the first 4 bytes (LE),
/// then each node:
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

    let mut buf = vec![0u8; 4]; // Placeholder for root offset
    let mut cache = HashMap::new();
    let root_offset = serialize_node(&root, &mut buf, &mut cache);

    let root_bytes = root_offset.to_le_bytes();
    buf[0..4].copy_from_slice(&root_bytes);
    buf
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

/// Serializes a node, merging identical subtrees (DAFSA).
///
/// Returns the offset of the serialized node.
fn serialize_node(node: &TrieNode, buf: &mut Vec<u8>, cache: &mut HashMap<TrieNode, u32>) -> u32 {
    // If an identical subtree was already serialized, return its offset.
    if let Some(&offset) = cache.get(node) {
        return offset;
    }

    // First, serialize all children recursively to get their offsets.
    let mut children_offsets = Vec::new();
    for (&kc, child) in &node.children {
        let offset = serialize_node(child, buf, cache);
        children_offsets.push((kc, offset));
    }

    // Now serialize the current node.
    let node_offset = u32::try_from(buf.len()).unwrap_or(u32::MAX);

    let child_count = node.children.len();
    assert!(child_count <= 255, "too many children");

    buf.push(u8::try_from(child_count).unwrap_or(255));
    for (kc, offset) in children_offsets {
        buf.push(kc);
        buf.extend_from_slice(&offset.to_le_bytes());
    }
    buf.push(u8::from(node.is_word));
    buf.push(node.frequency);

    // Cache this node's offset.
    cache.insert(node.clone(), node_offset);

    node_offset
}

/// A node in a steno dictionary trie.
#[derive(Clone, PartialEq, Eq, Hash)]
struct StenoNode {
    children: BTreeMap<u32, StenoNode>,
    translation: Vec<u8>, // HID usage IDs
}

impl StenoNode {
    fn new() -> Self {
        Self {
            children: BTreeMap::new(),
            translation: Vec::new(),
        }
    }
}

/// Compiles a steno dictionary into a compact DAFSA byte array.
///
/// Each entry is `(strokes, translation)` where `strokes` is a sequence of
/// `u64` bitmasks and `translation` is a pre-encoded playback bytecode stream.
#[must_use]
pub fn compile_steno_dictionary(entries: &[(Vec<u64>, Vec<u8>)]) -> Vec<u8> {
    let mut root = StenoNode::new();

    for (strokes, translation) in entries {
        if translation.is_empty() {
            continue;
        }

        let mut node = &mut root;
        for &stroke in strokes {
            let stroke = u32::try_from(stroke).unwrap_or(u32::MAX);
            node = node.children.entry(stroke).or_insert_with(StenoNode::new);
        }
        node.translation = translation.clone();
    }

    let mut buf = vec![0u8; 3];
    let mut cache = HashMap::new();
    let root_offset = serialize_steno_node(&root, &mut buf, &mut cache);

    let bytes = root_offset.to_le_bytes();
    buf[0..3].copy_from_slice(&bytes[0..3]);
    buf
}

fn write_u24(buf: &mut Vec<u8>, value: u32) {
    let bytes = value.to_le_bytes();
    buf.extend_from_slice(&bytes[0..3]);
}

fn serialize_steno_node(
    node: &StenoNode,
    buf: &mut Vec<u8>,
    cache: &mut HashMap<StenoNode, u32>,
) -> u32 {
    if let Some(&offset) = cache.get(node) {
        return offset;
    }

    let mut children_offsets = Vec::new();
    for (&stroke, child) in &node.children {
        let offset = serialize_steno_node(child, buf, cache);
        children_offsets.push((stroke, offset));
    }

    let node_offset = u32::try_from(buf.len()).unwrap_or(u32::MAX);

    write_compact_u16(buf, u16::try_from(node.children.len()).unwrap_or(u16::MAX));

    for (stroke, offset) in children_offsets {
        write_u24(buf, stroke);
        write_u24(buf, offset);
    }

    let len = u16::try_from(node.translation.len()).unwrap_or(u16::MAX);
    write_compact_u16(buf, len);
    if len > 0 {
        buf.extend_from_slice(&node.translation);
    }

    cache.insert(node.clone(), node_offset);
    node_offset
}

fn write_compact_u16(buf: &mut Vec<u8>, value: u16) {
    if value < 0xFF {
        buf.push(value as u8);
    } else {
        buf.push(0xFF);
        buf.extend_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_trie() {
        let trie = compile_trie(&[]);
        assert_eq!(trie.len(), 4 + 3);
        let root_offset = u32::from_le_bytes([trie[0], trie[1], trie[2], trie[3]]);
        assert_eq!(root_offset, 4);
        assert_eq!(trie.get(4).copied(), Some(0));
    }

    #[test]
    fn dafsa_merges_identical_suffixes() {
        let words = vec![(String::from("the"), 0), (String::from("she"), 0)];
        let trie = compile_trie(&words);
        // root (4 bytes) + root_node (1+2*5+2=13) + t_node (1+1*5+2=8) + s_node (8) + h_node (8) + e_node (3) = 44
        assert!(trie.len() < 50);
    }

    #[test]
    fn lookup_word_in_trie() {
        let words = vec![
            (String::from("the"), 0),
            (String::from("them"), 10),
            (String::from("hi"), 5),
        ];
        let trie = compile_trie(&words);

        assert_eq!(walk_trie(&trie, &[0x17, 0x0B, 0x08]), Some((true, 0)));
        assert_eq!(
            walk_trie(&trie, &[0x17, 0x0B, 0x08, 0x10]),
            Some((true, 10))
        );
        assert_eq!(walk_trie(&trie, &[0x0B, 0x0C]), Some((true, 5)));
        assert!(walk_trie(&trie, &[0x17, 0x0B]).is_none());
    }

    #[test]
    fn compile_steno_dictionary_keeps_encoded_payload() {
        let trie = compile_steno_dictionary(&[(vec![0x01], vec![0x01, 0x00, 0x02])]);
        assert!(trie.windows(3).any(|w| w == [0x01, 0x00, 0x02]));
    }

    fn walk_trie(trie: &[u8], keycodes: &[u8]) -> Option<(bool, u8)> {
        let root_offset = u32::from_le_bytes([trie[0], trie[1], trie[2], trie[3]]) as usize;
        let mut pos = root_offset;
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
