//! Twelve-tone equal-tempered Tonnetz coordinates and major/minor triad moves.
//!
//! The lattice folds octaves and enharmonic spellings into pitch classes.
//! Instruments own their current position, voicing, register and articulation.
//!
//! ```
//! use tecla::music::ChordQuality;
//! use tecla::music::tonnetz::{pitch_class, Transformation, Triad};
//!
//! let c_major = Triad::new(pitch_class(0, 0), ChordQuality::Major).unwrap();
//! let a_minor = c_major.transform(Transformation::Relative);
//! assert_eq!(a_minor.root(), 9);
//! assert_eq!(a_minor.quality(), ChordQuality::Minor);
//! assert!(a_minor.tones().contains(0)); // C remains a common tone.
//! assert_eq!(a_minor.transform(Transformation::Relative), c_major);
//! ```

use super::{ChordQuality, PitchClassSet};

/// Maps lattice coordinates to a pitch class, with C at `(0, 0)`.
///
/// A positive first-axis step adds a perfect fifth (7 semitones); a positive
/// second-axis step adds a major third (4 semitones). Moving `(1, -1)` adds
/// a minor third. Negative coordinates and the full `i32` range are supported.
#[must_use]
pub const fn pitch_class(fifths: i32, major_thirds: i32) -> u8 {
    // Reduce before multiplying so even extreme coordinates cannot overflow.
    let fifths = fifths.rem_euclid(12);
    let major_thirds = major_thirds.rem_euclid(3);
    ((7 * fifths + 4 * major_thirds) % 12) as u8
}

/// An edge move between major and minor triads, retaining two pitch classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transformation {
    /// P: parallel major/minor, such as C major to C minor.
    Parallel,
    /// R: relative major/minor, such as C major to A minor.
    Relative,
    /// L: leading-tone exchange, such as C major to E minor.
    LeadingToneExchange,
}

/// A major or minor triad without an assigned key, octave or voicing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triad {
    root: u8,
    quality: ChordQuality,
}

impl Triad {
    /// Creates a major/minor triad with a root pitch class in `0..12`.
    ///
    /// Rejects other roots and diminished/augmented qualities, which are outside
    /// the domain of these P/L/R transformations.
    #[must_use]
    pub const fn new(root: u8, quality: ChordQuality) -> Option<Self> {
        if root >= 12 {
            return None;
        }
        match quality {
            ChordQuality::Major | ChordQuality::Minor => Some(Self { root, quality }),
            ChordQuality::Diminished | ChordQuality::Augmented => None,
        }
    }

    /// Root pitch class, in `0..12`.
    #[must_use]
    pub const fn root(self) -> u8 {
        self.root
    }

    /// Major or minor quality.
    #[must_use]
    pub const fn quality(self) -> ChordQuality {
        self.quality
    }

    /// The triad's three distinct pitch classes.
    #[must_use]
    pub const fn tones(self) -> PitchClassSet {
        let third = if matches!(self.quality, ChordQuality::Major) {
            4
        } else {
            3
        };
        PitchClassSet::from_bits(
            (1 << self.root) | (1 << ((self.root + third) % 12)) | (1 << ((self.root + 7) % 12)),
        )
    }

    /// Applies P, L or R; applying the same move twice returns the original triad.
    ///
    /// The result describes pitch classes; octave placement and actual voice
    /// motion remain decisions for the instrument using it.
    #[must_use]
    pub const fn transform(self, transformation: Transformation) -> Self {
        let major = matches!(self.quality, ChordQuality::Major);
        let root = match transformation {
            Transformation::Parallel => self.root,
            Transformation::Relative => (self.root + if major { 9 } else { 3 }) % 12,
            Transformation::LeadingToneExchange => (self.root + if major { 4 } else { 8 }) % 12,
        };
        Self {
            root,
            quality: if major {
                ChordQuality::Minor
            } else {
                ChordQuality::Major
            },
        }
    }
}
