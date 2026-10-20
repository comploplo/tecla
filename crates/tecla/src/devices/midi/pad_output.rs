//! Recovery for a pad controller with one fixed channel and note map.

use crate::pipeline::report::MidiEvent;

/// An invalid fixed pad-controller configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadMidiConfigError {
    /// The channel exceeds 15.
    InvalidChannel(u8),
    /// A note exceeds 127.
    InvalidNote(u8),
    /// Two pads use the same note.
    DuplicateNote(u8),
}

/// Gates pad output until every configured note has been released.
///
/// The board supplies a USB session token: bit 0 means configured, and the
/// remaining bits identify the connection epoch. Every token change restarts
/// cleanup. Continue processing physical releases while output is suppressed;
/// discard those emitted events rather than replaying them when ready.
pub struct PadMidiOutput<const N: usize> {
    channel: u8,
    notes: [u8; N],
    session: u32,
    cleanup_cursor: usize,
}

impl<const N: usize> PadMidiOutput<N> {
    /// Validates the fixed channel and unique note map, initially offline.
    pub fn new(channel: u8, notes: [u8; N]) -> Result<Self, PadMidiConfigError> {
        if channel > 15 {
            return Err(PadMidiConfigError::InvalidChannel(channel));
        }
        for (index, &note) in notes.iter().enumerate() {
            if note > 127 {
                return Err(PadMidiConfigError::InvalidNote(note));
            }
            if notes.iter().take(index).any(|&previous| previous == note) {
                return Err(PadMidiConfigError::DuplicateNote(note));
            }
        }
        Ok(Self {
            channel,
            notes,
            session: 0,
            cleanup_cursor: 0,
        })
    }

    /// Observes the latest packed USB session token before processing output.
    pub fn observe_session(&mut self, session: u32) {
        if self.session != session {
            self.session = session;
            self.restart_cleanup();
        }
    }

    /// Restarts the note-off sweep after stop, panic, or failed delivery.
    pub fn restart_cleanup(&mut self) {
        self.cleanup_cursor = 0;
    }

    /// Whether newly generated pad events may be sent in this session.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.session & 1 != 0 && self.cleanup_cursor == N
    }

    /// The next cleanup packet, without advancing until delivery succeeds.
    #[must_use]
    pub fn next_cleanup(&self) -> Option<MidiEvent> {
        if self.session & 1 == 0 {
            return None;
        }
        self.notes
            .get(self.cleanup_cursor)
            .copied()
            .map(|note| MidiEvent::NoteOff {
                channel: self.channel,
                note,
            })
    }

    /// Commits one successful cleanup write if it completed in the same session.
    ///
    /// Call only after a successful write. Capture `started_session` before
    /// awaiting that write and read `current_session` immediately afterward.
    /// Returns whether the cleanup cursor advanced.
    pub fn cleanup_sent(&mut self, started_session: u32, current_session: u32) -> bool {
        self.observe_session(current_session);
        if started_session != current_session || self.session & 1 == 0 || self.cleanup_cursor == N {
            return false;
        }
        self.cleanup_cursor += 1;
        true
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    extern crate std;

    use super::*;

    fn note_off(note: u8) -> MidiEvent {
        MidiEvent::NoteOff { channel: 0, note }
    }

    #[test]
    fn rejects_invalid_channel_note_and_duplicate_note() {
        assert_eq!(
            PadMidiOutput::new(16, [60]).err(),
            Some(PadMidiConfigError::InvalidChannel(16))
        );
        assert_eq!(
            PadMidiOutput::new(0, [128]).err(),
            Some(PadMidiConfigError::InvalidNote(128))
        );
        assert_eq!(
            PadMidiOutput::new(0, [60, 61, 60]).err(),
            Some(PadMidiConfigError::DuplicateNote(60))
        );
        assert!(PadMidiOutput::new(15, [0, 127]).is_ok());
    }

    #[test]
    fn connection_cleans_every_note_in_order_before_becoming_ready() {
        let mut output = PadMidiOutput::new(0, [60, 61]).unwrap();
        assert!(!output.is_ready());
        assert_eq!(output.next_cleanup(), None);

        output.observe_session(1);
        assert!(!output.is_ready());
        assert_eq!(output.next_cleanup(), Some(note_off(60)));
        assert!(output.cleanup_sent(1, 1));
        assert_eq!(output.next_cleanup(), Some(note_off(61)));
        assert!(output.cleanup_sent(1, 1));
        assert!(output.is_ready());
        assert_eq!(output.next_cleanup(), None);
        assert!(!output.cleanup_sent(1, 1));

        output.observe_session(1);
        assert!(output.is_ready());
    }

    #[test]
    fn failed_write_does_not_advance_and_restart_releases_the_entire_map() {
        let mut output = PadMidiOutput::new(0, [60, 61]).unwrap();
        output.observe_session(1);
        assert!(output.cleanup_sent(1, 1));
        assert_eq!(output.next_cleanup(), Some(note_off(61)));
        // A failed write is not committed, so the same note remains pending.
        assert_eq!(output.next_cleanup(), Some(note_off(61)));
        output.observe_session(1);
        assert_eq!(output.next_cleanup(), Some(note_off(61)));
        assert!(output.cleanup_sent(1, 1));
        assert!(output.is_ready());
        output.restart_cleanup();
        assert_eq!(output.next_cleanup(), Some(note_off(60)));
        assert!(!output.is_ready());
    }

    #[test]
    fn completion_across_disconnect_or_reconnect_cannot_advance_cleanup() {
        let mut output = PadMidiOutput::new(0, [60]).unwrap();
        output.observe_session(1);
        assert_eq!(output.next_cleanup(), Some(note_off(60)));
        assert!(!output.cleanup_sent(1, 2));
        assert_eq!(output.next_cleanup(), None);
        assert!(!output.is_ready());

        // Disconnect and reconnect can both happen during one awaited write.
        assert!(!output.cleanup_sent(1, 3));
        assert_eq!(output.next_cleanup(), Some(note_off(60)));
        assert!(!output.is_ready());
        assert!(output.cleanup_sent(3, 3));
        assert!(output.is_ready());
    }

    #[test]
    fn stop_requires_cleanup_even_when_the_usb_session_is_unchanged() {
        let mut output = PadMidiOutput::new(0, [60]).unwrap();
        output.observe_session(1);
        assert!(output.cleanup_sent(1, 1));
        output.restart_cleanup();
        assert!(!output.is_ready());
        assert_eq!(output.next_cleanup(), Some(note_off(60)));
    }

    #[test]
    fn ready_gate_preserves_repeated_attacks_and_does_not_replay_suppressed_ones() {
        let mut output = PadMidiOutput::new(0, [60]).unwrap();
        let attacks = [MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        }; 2];
        let mut sent = std::vec::Vec::new();
        for session in [0, 1, 1] {
            output.observe_session(session);
            for attack in attacks {
                if output.is_ready() {
                    sent.push(attack);
                }
            }
            if output.next_cleanup().is_some() {
                assert!(output.cleanup_sent(session, session));
            }
        }
        assert_eq!(sent.as_slice(), &attacks);

        output.observe_session(2);
        assert!(!output.is_ready());
        output.observe_session(3);
        assert!(!output.is_ready());
        assert!(output.cleanup_sent(3, 3));
        assert_eq!(sent.as_slice(), &attacks);
    }
}
