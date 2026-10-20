//! MIDI event helpers — convenience constructors for [`MidiEvent`].
//!
//! These functions wrap [`MidiEvent`] variant construction so callers don't
//! need to import the enum variants directly. The keyboard pipeline emits
//! MIDI events via [`HidReport::Midi`](crate::pipeline::report::HidReport::Midi)
//! when a key is mapped to [`Action::Midi`](crate::action::Action::Midi).
//!
//! See also: [`action::MidiAction`](crate::action::MidiAction) for the
//! keymap-level MIDI action type.
//! [`PadMidiOutput`] supplies stop/reconnect cleanup for a fixed pad note map.

use crate::pipeline::report::MidiEvent;

pub mod output;
mod pad_output;
pub use pad_output::{PadMidiConfigError, PadMidiOutput};

/// Creates a MIDI note-on event.
#[must_use]
pub fn midi_note_on(channel: u8, note: u8, velocity: u8) -> MidiEvent {
    MidiEvent::NoteOn {
        channel,
        note,
        velocity,
    }
}

/// Creates a MIDI note-off event.
#[must_use]
pub fn midi_note_off(channel: u8, note: u8) -> MidiEvent {
    MidiEvent::NoteOff { channel, note }
}

/// Creates a MIDI control-change event.
#[must_use]
pub fn midi_cc(channel: u8, controller: u8, value: u8) -> MidiEvent {
    MidiEvent::Cc {
        channel,
        controller,
        value,
    }
}

/// Creates a MIDI polyphonic key pressure (per-note aftertouch) event.
#[must_use]
pub fn midi_poly_pressure(channel: u8, note: u8, pressure: u8) -> MidiEvent {
    MidiEvent::PolyPressure {
        channel,
        note,
        pressure,
    }
}

/// Creates a MIDI channel pressure (aftertouch) event.
#[must_use]
pub fn midi_channel_pressure(channel: u8, pressure: u8) -> MidiEvent {
    MidiEvent::ChannelPressure { channel, pressure }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_on_creates_correct_event() {
        let e = midi_note_on(0, 60, 127);
        assert_eq!(
            e,
            MidiEvent::NoteOn {
                channel: 0,
                note: 60,
                velocity: 127
            }
        );
    }

    #[test]
    fn note_off_creates_correct_event() {
        let e = midi_note_off(1, 48);
        assert_eq!(
            e,
            MidiEvent::NoteOff {
                channel: 1,
                note: 48
            }
        );
    }

    #[test]
    fn cc_creates_correct_event() {
        let e = midi_cc(0, 7, 100);
        assert_eq!(
            e,
            MidiEvent::Cc {
                channel: 0,
                controller: 7,
                value: 100
            }
        );
    }

    #[test]
    fn poly_pressure_creates_correct_event() {
        let e = midi_poly_pressure(0, 60, 100);
        assert_eq!(
            e,
            MidiEvent::PolyPressure {
                channel: 0,
                note: 60,
                pressure: 100
            }
        );
    }

    #[test]
    fn channel_pressure_creates_correct_event() {
        let e = midi_channel_pressure(1, 90);
        assert_eq!(
            e,
            MidiEvent::ChannelPressure {
                channel: 1,
                pressure: 90
            }
        );
    }
}
