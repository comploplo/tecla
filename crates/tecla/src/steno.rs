//! Stenography chord recognition.
//!
//! A [`StenoRecognizer`] accumulates a chord (set of simultaneously held keys)
//! until all keys are released, then yields a [`StenoChord`] containing the
//! raw bitmask of pressed flat key indices and the active protocol.
//!
//! Serialization to GeminiPR or TXBOLT wire format is provided by
//! [`StenoChord::serialize`]. Dictionary lookup stays on the host in Plover.

use crate::event::KeyEvent;
use crate::recognizer::{Recognition, Recognizer};

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

/// A resolved steno chord ready for serialization.
///
/// `keys` is a bitmask where bit `n` corresponds to flat key index `n`
/// (i.e. `row * COLS + col`). The board crate is responsible for mapping
/// physical key positions to the correct bit positions for the steno layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StenoChord {
    /// Bitmask of all key positions that were part of this chord.
    pub keys: u64,
    /// Protocol used for serialization.
    pub protocol: StenoProtocol,
}

impl StenoChord {
    /// Serializes the chord to wire bytes.
    ///
    /// Returns `(bytes, len)` where `bytes[..len]` is the packet.
    #[must_use]
    pub fn serialize(self) -> ([u8; 6], usize) {
        match self.protocol {
            StenoProtocol::GeminiPr => self.to_gemini_pr(),
            StenoProtocol::TxBolt => self.to_txbolt(),
        }
    }

    #[expect(
        clippy::as_conversions,
        reason = "u64 bits masked to 6 bits then cast to u8"
    )]
    fn to_gemini_pr(self) -> ([u8; 6], usize) {
        let mut out = [0u8; 6];
        for byte_idx in 0..6_usize {
            let bits = (self.keys >> (byte_idx * 6)) & 0x3F;
            if let Some(slot) = out.get_mut(byte_idx) {
                *slot = bits as u8;
            }
        }
        if let Some(b) = out.first_mut() {
            *b |= 0x80;
        }
        (out, 6)
    }

    #[expect(
        clippy::as_conversions,
        reason = "u64 bits masked then cast to u8 for wire format"
    )]
    fn to_txbolt(self) -> ([u8; 6], usize) {
        let mut out = [0u8; 6];
        let mut len = 0_usize;
        let banks: [(u8, u8); 4] = [(0b00, 6), (0b01, 6), (0b10, 6), (0b11, 6)];
        for (bank_id, bit_count) in banks {
            let shift = usize::from(bank_id) * 6;
            let bits = (self.keys >> shift) & ((1 << bit_count) - 1);
            if bits != 0 || len > 0 {
                if let Some(slot) = out.get_mut(len) {
                    *slot = (bank_id << 6) | (bits & 0xFF) as u8;
                    len += 1;
                }
            }
        }
        if len == 0 {
            if let Some(b) = out.first_mut() {
                *b = 0;
            }
            len = 1;
        }
        (out, len)
    }
}

// ── StenoRecognizer ───────────────────────────────────────────────────────────

/// Accumulates a steno chord until all keys are released.
///
/// # Lifecycle
///
/// 1. Construct with [`StenoRecognizer::new`].
/// 2. Feed events via [`observe`](Recognizer::observe).
/// 3. When all pressed keys are subsequently released, yields
///    [`Recognition::Matched`] with the chord.
/// 4. [`tick`](Recognizer::tick) and [`next_deadline_ms`](Recognizer::next_deadline_ms)
///    are no-ops — steno chords have no timeout.
pub struct StenoRecognizer {
    protocol: StenoProtocol,
    /// Keys currently physically held.
    held_mask: u64,
    /// All keys that were pressed as part of this chord.
    chord_mask: u64,
    /// True once at least one key has been pressed.
    started: bool,
}

impl StenoRecognizer {
    /// Creates a new steno recognizer with the given protocol.
    #[must_use]
    pub fn new(protocol: StenoProtocol) -> Self {
        Self {
            protocol,
            held_mask: 0,
            chord_mask: 0,
            started: false,
        }
    }
}

impl Recognizer for StenoRecognizer {
    type Output = StenoChord;

    fn observe(&mut self, event: KeyEvent, _now_ms: u64) -> Recognition<StenoChord> {
        let bit = 1u64 << event.pos.flat(64).min(63);

        if event.pressed {
            self.started = true;
            self.held_mask |= bit;
            self.chord_mask |= bit;
            Recognition::Pending
        } else {
            self.held_mask &= !bit;
            if self.started && self.held_mask == 0 {
                Recognition::Matched(StenoChord {
                    keys: self.chord_mask,
                    protocol: self.protocol,
                })
            } else {
                Recognition::Pending
            }
        }
    }

    fn tick(&mut self, _now_ms: u64) -> Recognition<StenoChord> {
        Recognition::Pending
    }

    fn next_deadline_ms(&self) -> Option<u64> {
        None
    }

    fn reset(&mut self) {
        self.held_mask = 0;
        self.chord_mask = 0;
        self.started = false;
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;
    use crate::event::{KeyEvent, KeyPos};

    fn press(pos: KeyPos, ms: u64) -> KeyEvent {
        KeyEvent::press(pos, ms)
    }

    fn release(pos: KeyPos, ms: u64) -> KeyEvent {
        KeyEvent::release(pos, ms)
    }

    const A: KeyPos = KeyPos::new(0, 0);
    const B: KeyPos = KeyPos::new(0, 1);

    // ── StenoRecognizer ───────────────────────────────────────────────────────

    #[test]
    fn pending_while_keys_held() {
        let mut rec = StenoRecognizer::new(StenoProtocol::GeminiPr);
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 5), 5);
        let result = rec.observe(release(A, 10), 10);
        assert!(result.is_pending());
    }

    #[test]
    fn fires_when_all_released() {
        let mut rec = StenoRecognizer::new(StenoProtocol::GeminiPr);
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 5), 5);
        rec.observe(release(A, 10), 10);
        let result = rec.observe(release(B, 15), 15);
        assert!(result.is_matched());
    }

    #[test]
    fn chord_contains_all_pressed_keys() {
        let mut rec = StenoRecognizer::new(StenoProtocol::GeminiPr);
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 5), 5);
        rec.observe(release(A, 10), 10);
        let result = rec.observe(release(B, 15), 15);
        let chord = result.matched().unwrap();
        let a_bit = 1u64 << A.flat(64);
        let b_bit = 1u64 << B.flat(64);
        assert!(chord.keys & a_bit != 0);
        assert!(chord.keys & b_bit != 0);
    }

    #[test]
    fn single_key_chord_fires_on_release() {
        let mut rec = StenoRecognizer::new(StenoProtocol::TxBolt);
        rec.observe(press(A, 0), 0);
        let result = rec.observe(release(A, 10), 10);
        assert!(result.is_matched());
    }

    #[test]
    fn release_before_any_press_is_pending() {
        let mut rec = StenoRecognizer::new(StenoProtocol::GeminiPr);
        let result = rec.observe(release(A, 0), 0);
        assert!(result.is_pending());
    }

    #[test]
    fn tick_always_pending() {
        let mut rec = StenoRecognizer::new(StenoProtocol::GeminiPr);
        assert!(rec.tick(1000).is_pending());
    }

    #[test]
    fn next_deadline_always_none() {
        let mut rec = StenoRecognizer::new(StenoProtocol::GeminiPr);
        rec.observe(press(A, 0), 0);
        assert!(rec.next_deadline_ms().is_none());
    }

    #[test]
    fn reset_clears_state() {
        let mut rec = StenoRecognizer::new(StenoProtocol::GeminiPr);
        rec.observe(press(A, 0), 0);
        rec.reset();
        let result = rec.observe(release(A, 10), 10);
        assert!(result.is_pending());
    }

    #[test]
    fn protocol_stored_in_chord() {
        let mut rec = StenoRecognizer::new(StenoProtocol::TxBolt);
        rec.observe(press(A, 0), 0);
        let result = rec.observe(release(A, 10), 10);
        let chord = result.matched().unwrap();
        assert_eq!(chord.protocol, StenoProtocol::TxBolt);
    }

    // ── StenoChord serialization ──────────────────────────────────────────────

    #[test]
    fn gemini_pr_high_bit_set_on_byte_0() {
        let chord = StenoChord {
            keys: 0,
            protocol: StenoProtocol::GeminiPr,
        };
        let (bytes, len) = chord.serialize();
        assert_eq!(len, 6);
        assert!(bytes[0] & 0x80 != 0);
    }

    #[test]
    fn gemini_pr_encodes_six_bytes() {
        let chord = StenoChord {
            keys: 0xFF,
            protocol: StenoProtocol::GeminiPr,
        };
        let (_, len) = chord.serialize();
        assert_eq!(len, 6);
    }

    #[test]
    fn txbolt_empty_chord_produces_one_byte() {
        let chord = StenoChord {
            keys: 0,
            protocol: StenoProtocol::TxBolt,
        };
        let (_, len) = chord.serialize();
        assert_eq!(len, 1);
    }

    #[test]
    fn txbolt_nonempty_chord_produces_bytes() {
        let chord = StenoChord {
            keys: 0b111,
            protocol: StenoProtocol::TxBolt,
        };
        let (_, len) = chord.serialize();
        assert!(len >= 1);
    }

    #[test]
    fn gemini_pr_key_bits_appear_in_output() {
        let chord = StenoChord {
            keys: 1,
            protocol: StenoProtocol::GeminiPr,
        };
        let (bytes, _) = chord.serialize();
        assert!(bytes[0] & 0x01 != 0);
    }
}
