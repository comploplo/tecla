//! Stenography chord translation.
//!
//! [`StenoEngine`] performs on-device translation of steno chords into a
//! compact native playback bytecode using a pre-compiled DAFSA dictionary.
//! Chords are accumulated by the processor's inline chord state machine and
//! handed here as [`StenoChord`] values.

use heapless::Vec;

/// Playback opcode: literal text segment.
pub const OP_TEXT: u8 = 0x01;
/// Playback opcode: host key chord with modifier bits and usage ID.
pub const OP_HOST_CHORD: u8 = 0x02;
/// Playback opcode: suppress automatic spacing before the next text segment.
pub const OP_NO_SPACE: u8 = 0x03;
/// Playback opcode: force a space before the next text segment.
pub const OP_SPACE: u8 = 0x04;
/// Playback opcode: set a case transform for the next text segment.
pub const OP_SET_CASE: u8 = 0x05;
/// Playback opcode: enable the formatting/spacing mode requested by Lapwing.
pub const OP_SET_SPACE: u8 = 0x06;
/// Playback opcode: clear spacing/case state.
pub const OP_RESET_FORMATTING: u8 = 0x07;
/// Playback opcode: replay the last translated stroke sequence.
pub const OP_REPEAT_LAST_TRANSLATION: u8 = 0x08;
/// Playback opcode: undo the previous visible translation.
pub const OP_UNDO_LAST_TRANSLATION: u8 = 0x09;
/// Playback opcode: no-op placeholder for host-only commands.
pub const OP_NOOP: u8 = 0x0A;
/// Playback opcode: apply a case transform to the previous visible word.
pub const OP_RETRO_CASE: u8 = 0x0B;
/// Playback opcode: stitch text onto the previous word without disturbing spacing state.
pub const OP_STITCH: u8 = 0x0C;

/// Case transforms understood by the steno playback layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMode {
    /// Capitalize the first character of the next word.
    CapFirstWord = 0x01,
    /// Uppercase the first word.
    UpperFirstWord = 0x02,
    /// Lowercase the first character of the next word.
    LowerFirstChar = 0x03,
}

impl CaseMode {
    /// Decodes a case-mode byte from the steno playback stream.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x01 => Some(Self::CapFirstWord),
            0x02 => Some(Self::UpperFirstWord),
            0x03 => Some(Self::LowerFirstChar),
            _ => None,
        }
    }
}

// ── StenoProtocol ─────────────────────────────────────────────────────────────

/// Wire protocol for transmitting raw steno chords to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StenoProtocol {
    /// GeminiPR: 6-byte packet, high bit of byte 0 set, one bit per steno key.
    GeminiPr,
    /// TXBOLT: variable-length (1–5 bytes), 4 banks of steno keys.
    TxBolt,
}

// ── StenoChord ────────────────────────────────────────────────────────────────

/// A resolved steno chord ready for serialization or translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StenoChord {
    /// Bitmask of all key positions that were part of this chord.
    pub keys: u64,
}

// ── StenoEngine ───────────────────────────────────────────────────────────────

/// Steno translation engine.
///
/// Walks a DAFSA dictionary trie of multi-stroke chords and returns a native
/// playback bytecode stream.
pub struct StenoEngine {
    dictionary: &'static [u8],
    /// Current position in the trie (node offset).
    current_pos: usize,
    /// Buffer of recent strokes in the current multi-stroke sequence.
    strokes: Vec<u64, 8>,
}

impl StenoEngine {
    /// Creates a new engine with the given DAFSA dictionary.
    #[must_use]
    pub fn new(dictionary: &'static [u8]) -> Self {
        let root_offset = if dictionary.len() >= 3 {
            read_u24(dictionary, 0) as usize
        } else {
            0
        };
        Self {
            dictionary,
            current_pos: root_offset,
            strokes: Vec::new(),
        }
    }

    /// Processes a single stroke (chord), returning the playback bytecode if
    /// found.
    ///
    /// If the stroke continues a multi-stroke sequence but doesn't resolve yet,
    /// returns `None`. If the stroke doesn't match any path, resets.
    pub fn process_stroke(&mut self, chord: StenoChord) -> Option<&[u8]> {
        if self.dictionary.len() < 3 {
            return None;
        }

        let (child_count, child_count_len) = read_compact_u16(self.dictionary, self.current_pos)?;
        let children_start = self.current_pos + child_count_len;
        let chord_u32 = chord.keys as u32;

        let found_pos =
            binary_search_children(self.dictionary, children_start, child_count, chord_u32);

        if let Some(pos) = found_pos {
            self.current_pos = pos;
            self.strokes.push(chord.keys).ok();

            let (child_count, child_count_len) =
                read_compact_u16(self.dictionary, self.current_pos)?;

            let translation_len_pos = self.current_pos + child_count_len + child_count * 6;
            let (translation_len, translation_len_len) =
                read_compact_u16(self.dictionary, translation_len_pos)?;

            if translation_len > 0 {
                let start = translation_len_pos + translation_len_len;
                let end = start.checked_add(translation_len)?;
                let translation = self.dictionary.get(start..end)?;
                self.reset();
                return Some(translation);
            }
            None
        } else {
            self.reset();
            None
        }
    }

    /// Returns the number of strokes accumulated in the current multi-stroke sequence.
    #[must_use]
    pub fn pending_stroke_count(&self) -> usize {
        self.strokes.len()
    }

    /// Resets the engine to the root of the trie.
    pub fn reset(&mut self) {
        if self.dictionary.len() >= 3 {
            self.current_pos = read_u24(self.dictionary, 0) as usize;
        }
        self.strokes.clear();
    }
}

fn read_u24(data: &[u8], pos: usize) -> u32 {
    u32::from(*data.get(pos).unwrap_or(&0))
        | (u32::from(*data.get(pos + 1).unwrap_or(&0)) << 8)
        | (u32::from(*data.get(pos + 2).unwrap_or(&0)) << 16)
}

fn binary_search_children(data: &[u8], start: usize, count: usize, target: u32) -> Option<usize> {
    let mut lo = 0usize;
    let mut hi = count;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let slot = start + mid * 6;
        let stroke = read_u24(data, slot);
        if stroke == target {
            return Some(read_u24(data, slot + 3) as usize);
        } else if stroke < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    None
}

fn read_compact_u16(data: &[u8], pos: usize) -> Option<(usize, usize)> {
    let first = *data.get(pos)?;
    if first < 0xFF {
        Some((usize::from(first), 1))
    } else {
        let value = u16::from_le_bytes([*data.get(pos + 1)?, *data.get(pos + 2)?]);
        Some((usize::from(value), 3))
    }
}

/// Public accessor for compact u16 reading, used by the playback engine.
pub fn read_compact_u16_pub(data: &[u8], pos: usize) -> Option<(usize, usize)> {
    read_compact_u16(data, pos)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn build_test_dict(entries: &[(std::vec::Vec<u64>, &[u8])]) -> std::vec::Vec<u8> {
        tecla_config::trie::compile_steno_dictionary(
            &entries
                .iter()
                .map(|(s, t)| (s.clone(), t.to_vec()))
                .collect::<std::vec::Vec<_>>(),
        )
    }

    #[test]
    fn single_stroke_translation() {
        let dict = build_test_dict(&[(std::vec![0x01], &[0x01, 0x01, 0x00, b'a'])]);
        let mut engine = StenoEngine::new(std::boxed::Box::leak(dict.into_boxed_slice()));

        let chord = StenoChord { keys: 0x01 };
        let result = engine.process_stroke(chord);

        assert_eq!(result.unwrap(), &[0x01, 0x01, 0x00, b'a']);
    }

    #[test]
    fn multi_stroke_translation() {
        let dict = build_test_dict(&[(std::vec![0x01, 0x01], &[0x01, 0x02, 0x00, b'a', b'a'])]);
        let mut engine = StenoEngine::new(std::boxed::Box::leak(dict.into_boxed_slice()));

        let chord = StenoChord { keys: 0x01 };

        // First stroke
        let result1 = engine.process_stroke(chord);
        assert!(result1.is_none());

        // Second stroke
        let result2 = engine.process_stroke(chord);
        assert_eq!(result2.unwrap(), &[0x01, 0x02, 0x00, b'a', b'a']);
    }

    #[test]
    fn mismatched_stroke_resets() {
        let dict = build_test_dict(&[(std::vec![0x01, 0x01], &[0x01, 0x02, 0x00, b'a', b'a'])]);
        let mut engine = StenoEngine::new(std::boxed::Box::leak(dict.into_boxed_slice()));

        let chord1 = StenoChord { keys: 0x01 };
        let chord2 = StenoChord { keys: 0x02 };

        engine.process_stroke(chord1);
        let result = engine.process_stroke(chord2);

        assert!(result.is_none());
        assert_eq!(engine.strokes.len(), 0);
    }
}
