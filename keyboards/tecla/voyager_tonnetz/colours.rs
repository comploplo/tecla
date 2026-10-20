//! Shared, state-driven Voyager music colours.

use tecla::rgb::{hsv_to_rgb, Hsv, Rgb};

/// The musical significance of one note LED, selected in priority order by its map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteState {
    /// A physically held key, shown at its captured attack pitch.
    Held,
    /// A pad's own released note is still retained by the damper.
    Retained,
    /// An unpressed key would play exactly the MIDI pitch of a held note.
    ExactHeld,
    /// An unpressed key would play a held pitch class in another octave.
    OctaveHeld,
    /// A key corresponds exactly to a generated supporting voice.
    #[allow(
        dead_code,
        reason = "manual Tonnetz profiles generate no supporting voices"
    )]
    GeneratedExact,
    /// A key corresponds to a generated voice in another octave.
    #[allow(
        dead_code,
        reason = "manual Tonnetz profiles generate no supporting voices"
    )]
    GeneratedOctave,
    /// A prospective pitch matches an audible retained note's pitch class.
    Sustained,
    /// A possible harmonic destination, without generating sound.
    Hint,
    /// Piano background: white naturals, dark accidentals, brighter white C references.
    Idle,
}

/// Piano background with stable circle-of-fifths colours for musical activity.
pub fn note(pitch: u8, state: NoteState) -> Rgb {
    let fifths_index = u16::from((pitch % 12 * 7) % 12);
    let hue = 32u8.wrapping_add((256 * fifths_index / 12) as u8);
    let value = match state {
        NoteState::Held => 144,
        NoteState::Retained => 80,
        NoteState::ExactHeld => 112,
        NoteState::OctaveHeld => 96,
        NoteState::GeneratedExact => 88,
        NoteState::GeneratedOctave | NoteState::Sustained => 64,
        NoteState::Hint => 48,
        NoteState::Idle => {
            return match pitch % 12 {
                0 => Rgb::new(40, 40, 40),
                2 | 4 | 5 | 7 | 9 | 11 => Rgb::new(24, 24, 24),
                _ => Rgb::BLACK,
            };
        }
    };
    let saturation = match state {
        NoteState::Held => 120,
        NoteState::ExactHeld => 160,
        NoteState::GeneratedExact => 180,
        _ => 210,
    };
    hsv_to_rgb(Hsv::new(hue, saturation, value))
}

/// A fixed-purpose control retains its hue and brightens while active.
pub fn control(hue: u8, active: bool) -> Rgb {
    hsv_to_rgb(Hsv::new(
        hue,
        if active { 120 } else { 210 },
        if active { 144 } else { 16 },
    ))
}

/// Stop remains recognizably red, including while held.
pub const fn stop(active: bool) -> Rgb {
    Rgb::new(if active { 144 } else { 32 }, 0, 0)
}
