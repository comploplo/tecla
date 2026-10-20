//! Diatonic pitch arithmetic and a bounded triad/seventh vocabulary.

/// The unaltered seven-note scale used by a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The major scale: 0, 2, 4, 5, 7, 9, 11 semitones.
    Major,
    /// Unaltered minor; no raised leading tone is implied.
    NaturalMinor,
}

impl Mode {
    const fn intervals(self) -> [u8; 7] {
        match self {
            Self::Major => [0, 2, 4, 5, 7, 9, 11],
            Self::NaturalMinor => [0, 2, 3, 5, 7, 8, 10],
        }
    }
}

/// A scale degree, independent of octave and enharmonic spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Degree {
    /// Tonic.
    I,
    /// Supertonic.
    Ii,
    /// Mediant.
    Iii,
    /// Subdominant.
    Iv,
    /// Dominant.
    V,
    /// Submediant.
    Vi,
    /// Leading tone in major, subtonic in natural minor.
    Vii,
}

impl Degree {
    /// Every degree in ascending scale order.
    pub const ALL: [Self; 7] = [
        Self::I,
        Self::Ii,
        Self::Iii,
        Self::Iv,
        Self::V,
        Self::Vi,
        Self::Vii,
    ];
}

/// Twelve pitch classes represented by the low twelve bits of an integer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PitchClassSet(u16);

impl PitchClassSet {
    /// Constructs a set, discarding bits above the twelve pitch classes.
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits & 0x0fff)
    }

    /// Returns the twelve-bit representation, C in bit zero.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Accepts either a pitch class or a MIDI note.
    pub const fn contains(self, note: u8) -> bool {
        self.0 & (1 << (note % 12)) != 0
    }

    /// Adds a pitch class or the class of a MIDI note.
    pub fn insert(&mut self, note: u8) {
        self.0 |= 1 << (note % 12);
    }

    /// Removes a pitch class or the class of a MIDI note.
    pub fn remove(&mut self, note: u8) {
        self.0 &= !(1 << (note % 12));
    }

    /// Counts distinct pitch classes.
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Whether the set has no pitch classes.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A validated tonic pitch class and diatonic mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    tonic: u8,
    mode: Mode,
}

impl Key {
    /// C major, with C represented by pitch class zero.
    pub const C_MAJOR: Self = Self {
        tonic: 0,
        mode: Mode::Major,
    };

    /// Validates a tonic pitch class in 0..12.
    pub const fn new(tonic: u8, mode: Mode) -> Option<Self> {
        if tonic < 12 {
            Some(Self { tonic, mode })
        } else {
            None
        }
    }

    /// Returns the tonic pitch class.
    pub const fn tonic(self) -> u8 {
        self.tonic
    }

    /// Returns the unaltered scale mode.
    pub const fn mode(self) -> Mode {
        self.mode
    }

    /// Returns a degree's pitch class.
    pub const fn pitch(self, degree: Degree) -> u8 {
        (self.tonic + self.mode.intervals()[degree as usize]) % 12
    }

    /// Finds the scale degree of a valid MIDI note, rejecting chromatic notes.
    pub fn degree(self, note: u8) -> Option<Degree> {
        if note > 127 {
            return None;
        }
        Degree::ALL
            .into_iter()
            .find(|&degree| self.pitch(degree) == note % 12)
    }

    /// Whether a valid MIDI note belongs to this scale.
    pub fn contains(self, note: u8) -> bool {
        self.degree(note).is_some()
    }

    /// Returns the scale's seven pitch classes.
    pub fn pitches(self) -> PitchClassSet {
        let mut pitches = PitchClassSet::default();
        for degree in Degree::ALL {
            pitches.insert(self.pitch(degree));
        }
        pitches
    }

    /// Moves by scale steps, preserving direction across C and rejecting notes
    /// outside the key or the MIDI range. A step of -1 means the previous degree.
    pub fn diatonic_step(self, note: u8, steps: i8) -> Option<u8> {
        let degree = self.degree(note)?;
        let relative = i16::from(note) - i16::from(self.tonic);
        let octave = relative.div_euclid(12);
        let position = octave * 7 + degree as i16 + i16::from(steps);
        let next = i16::from(self.tonic)
            + position.div_euclid(7) * 12
            + i16::from(self.mode.intervals()[position.rem_euclid(7) as usize]);
        if (0..=127).contains(&next) {
            Some(next as u8)
        } else {
            None
        }
    }

    /// Natural minor has a subtonic, so it has no semitone leading tone.
    pub const fn leading_tone(self) -> Option<u8> {
        match self.mode {
            Mode::Major => Some(self.pitch(Degree::Vii)),
            Mode::NaturalMinor => None,
        }
    }

    /// Stacks diatonic thirds on a degree to form a triad or seventh chord.
    pub fn chord(self, degree: Degree, kind: ChordKind) -> Chord {
        let index = degree as usize;
        let root = self.pitch(degree);
        let third = self.pitch(Degree::ALL[(index + 2) % 7]);
        let fifth = self.pitch(Degree::ALL[(index + 4) % 7]);
        let seventh = match kind {
            ChordKind::Triad => None,
            ChordKind::Seventh => Some(self.pitch(Degree::ALL[(index + 6) % 7])),
        };
        let mut tones = PitchClassSet::default();
        for pitch in [root, third, fifth] {
            tones.insert(pitch);
        }
        if let Some(pitch) = seventh {
            tones.insert(pitch);
        }
        Chord {
            degree,
            kind,
            root,
            third,
            fifth,
            seventh,
            tones,
        }
    }
}

/// Number of stacked diatonic thirds in a chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordKind {
    /// Root, third and fifth.
    Triad,
    /// Root, third, fifth and seventh.
    Seventh,
}

/// Quality of a chord's underlying triad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordQuality {
    /// Major third and perfect fifth.
    Major,
    /// Minor third and perfect fifth.
    Minor,
    /// Minor third and diminished fifth.
    Diminished,
    /// Major third and augmented fifth.
    Augmented,
}

/// A pitch's structural role in a chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneRole {
    /// The chord root.
    Root,
    /// The diatonic third above the root.
    Third,
    /// The diatonic fifth above the root.
    Fifth,
    /// The diatonic seventh above the root.
    Seventh,
}

/// The basic functional family of a diatonic chord in a major key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonicFunction {
    /// I and its iii/vi substitutes.
    Tonic,
    /// ii and IV, which can approach the dominant.
    Predominant,
    /// V and vii, whose leading tone points toward the tonic.
    Dominant,
}

/// A diatonic tertian chord. Pitches are classes, not a particular voicing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    /// Scale degree on which the chord is built.
    pub degree: Degree,
    /// Triad or seventh chord.
    pub kind: ChordKind,
    /// Root pitch class.
    pub root: u8,
    /// Third pitch class.
    pub third: u8,
    /// Fifth pitch class.
    pub fifth: u8,
    /// Seventh pitch class, when present.
    pub seventh: Option<u8>,
    /// All distinct chord pitch classes.
    pub tones: PitchClassSet,
}

impl Chord {
    /// Returns the major-key functional family, without analyzing a phrase or
    /// treating natural-minor v as a raised-leading-tone dominant.
    pub const fn function(self) -> HarmonicFunction {
        match self.degree {
            Degree::I | Degree::Iii | Degree::Vi => HarmonicFunction::Tonic,
            Degree::Ii | Degree::Iv => HarmonicFunction::Predominant,
            Degree::V | Degree::Vii => HarmonicFunction::Dominant,
        }
    }

    /// Finds the role of a MIDI note or pitch class in this chord.
    pub fn role(self, note: u8) -> Option<ToneRole> {
        let pitch = note % 12;
        if pitch == self.root {
            Some(ToneRole::Root)
        } else if pitch == self.third {
            Some(ToneRole::Third)
        } else if pitch == self.fifth {
            Some(ToneRole::Fifth)
        } else if self.seventh == Some(pitch) {
            Some(ToneRole::Seventh)
        } else {
            None
        }
    }

    /// Returns the quality of the underlying triad.
    pub fn quality(self) -> ChordQuality {
        match (
            (self.third + 12 - self.root) % 12,
            (self.fifth + 12 - self.root) % 12,
        ) {
            (3, 6) => ChordQuality::Diminished,
            (4, 8) => ChordQuality::Augmented,
            (3, _) => ChordQuality::Minor,
            _ => ChordQuality::Major,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_cross_octaves_and_reject_midi_boundaries() {
        let c = Key::C_MAJOR;
        assert_eq!(c.diatonic_step(60, -1), Some(59));
        assert_eq!(c.diatonic_step(59, 1), Some(60));
        assert_eq!(c.diatonic_step(60, -8), Some(47));
        assert_eq!(c.diatonic_step(0, -1), None);
        assert_eq!(c.diatonic_step(127, 1), None);
        assert_eq!(c.diatonic_step(61, 0), None);
        assert_eq!(c.diatonic_step(128, 0), None);
        let b = Key::new(11, Mode::Major).unwrap();
        assert_eq!(b.diatonic_step(60, -1), None);
        assert_eq!(b.diatonic_step(61, -1), Some(59));
        assert_eq!(b.diatonic_step(59, 1), Some(61));
    }

    #[test]
    fn minor_and_chord_roles_are_diatonic() {
        let a = Key::new(9, Mode::NaturalMinor).unwrap();
        assert_eq!(a.pitches(), Key::C_MAJOR.pitches());
        assert_eq!(a.leading_tone(), None);
        let tonic = a.chord(Degree::I, ChordKind::Triad);
        assert_eq!(tonic.quality(), ChordQuality::Minor);
        assert_eq!(tonic.role(60), Some(ToneRole::Third));
        let dominant = Key::C_MAJOR.chord(Degree::V, ChordKind::Seventh);
        assert_eq!(dominant.seventh, Some(5));
        assert_eq!(dominant.tones.len(), 4);
        assert_eq!(dominant.role(65), Some(ToneRole::Seventh));
        assert_eq!(Key::new(12, Mode::Major), None);
    }

    #[test]
    fn pitch_sets_remove_without_affecting_other_notes() {
        let mut set = PitchClassSet::default();
        for note in [60, 64, 67] {
            set.insert(note);
        }
        set.remove(4);
        assert!(set.contains(0) && set.contains(7));
        assert!(!set.contains(4));
        assert_eq!(set.len(), 2);
        assert_eq!(PitchClassSet::from_bits(u16::MAX).len(), 12);
    }
}
