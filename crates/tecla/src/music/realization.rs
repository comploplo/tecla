//! Bounded harmonic realization around an arbitrary literal pitch collection.
//!
//! This planner adds supporting voices; it never changes, removes or echoes an
//! exact played pitch. Phrase direction, prepared holds and voice-specific
//! dominant resolutions are retained between explicit events. It deliberately
//! has no timer, MIDI output, automatic rhythm, or fixed bass/soprano anchors.

use super::harmony::{Chord, Shape};
use super::voicing::parallel_pair;
use super::{ChordKind, ChordQuality, Key, PitchClassSet, Voicing};

/// Maximum simultaneously generated supporting voices.
pub const SUPPORT_VOICES: usize = 6;
/// Chord identities per bounded search, independent of the number of held keys.
pub const CHORD_CANDIDATES: usize = 12 * 25;

const SHAPES: [Shape; 25] = [
    Shape::Major,
    Shape::Minor,
    Shape::Diminished,
    Shape::Augmented,
    Shape::Major6,
    Shape::Minor6,
    Shape::Add9,
    Shape::MinorAdd9,
    Shape::Major7,
    Shape::Minor7,
    Shape::Dominant7,
    Shape::HalfDiminished7,
    Shape::Diminished7,
    Shape::MinorMajor7,
    Shape::Major9,
    Shape::Minor9,
    Shape::Dominant9,
    Shape::DominantFlat13,
    Shape::Sus2,
    Shape::Sus4,
    Shape::DominantSus4,
    Shape::Dominant13,
    Shape::Major13,
    Shape::DominantFlat9,
    Shape::DominantSharp9,
];

/// Requested chord colour, with its characteristic pitch actually represented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Colour {
    /// Follow the literal collection without adding a specified extension.
    #[default]
    Natural,
    /// Add a second or fourth above the harmonic root.
    SecondsFourths,
    /// Add a major sixth above the harmonic root.
    Sixth,
    /// Supply the selected chord's seventh.
    Seventh,
    /// Supply a ninth, thirteenth, or altered dominant extension.
    Extended,
}

/// The chord-member role of the lowest literal note, independently of added bass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Foundation {
    /// Infer the lowest played note's role from the complete collection.
    #[default]
    Natural,
    /// Interpret the lowest played note as a root.
    Root,
    /// Interpret the lowest played note as a major or minor third.
    Third,
    /// Interpret the lowest played note as a fifth.
    Fifth,
    /// Interpret the lowest played note as a seventh.
    Seventh,
}

/// Register movement of the generated upper voices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Motion {
    /// Prefer short voice-leading distances and common tones.
    #[default]
    Nearest,
    /// Move supporting upper voices downward where the register permits.
    Falling,
    /// Move supporting upper voices upward where the register permits.
    Rising,
    /// Move supporting upper voices toward their preceding collective centre.
    Inward,
    /// Move supporting upper voices away from their preceding collective centre.
    Outward,
}

/// A harmonic relationship to the faithful interpretation of the literal notes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Reharm {
    /// Follow the literal collection and any requested foundation role.
    #[default]
    Faithful,
    /// Use the relative major or minor family.
    Relative,
    /// Choose a different root sharing at least two structural pitch classes.
    CommonTone,
    /// Borrow from the parallel mode or its flat-side chord families.
    Borrowed,
    /// Tonicize the local destination with its dominant.
    AppliedDominant,
}

/// Phrase direction, independent of the requested colour or register movement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Direction {
    /// Continue the current musical thought.
    #[default]
    Continue,
    /// Establish a predominant before the current destination.
    Prepare,
    /// Establish dominant pull toward the current destination.
    Lean,
    /// Fulfil the saved destination and its generated-voice tendencies.
    Arrive,
    /// Resolve toward the local deceptive substitute rather than the tonic.
    Deceive,
    /// Keep a compatible structural chord and audition another bounded voicing.
    Explore,
    /// Audition a different compatible interpretation within the chosen relation.
    Substitute,
}

/// Momentary, categorical compositional intentions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Preferences {
    /// Requested characteristic chord colour.
    pub colour: Colour,
    /// Chord role assigned to the lowest literal played note.
    pub foundation: Foundation,
    /// Requested movement of the generated upper voices.
    pub motion: Motion,
    /// Requested harmonic relationship to the faithful literal interpretation.
    pub reharm: Reharm,
    /// Current phrase direction or explicit audition action.
    pub direction: Direction,
    /// Retain prepared descending suspensions until this control is released.
    pub delay: bool,
    /// Retain the generated bass pitch across subsequent harmonic events.
    pub pedal: bool,
}

/// Supporting realization; the caller separately owns all literal played notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Realization {
    /// Stable voice slots; absent voices are genuinely silent.
    pub notes: [Option<u8>; SUPPORT_VOICES],
    /// Structural interpretation; literal nonchord tones remain present externally.
    pub chord: Option<Chord>,
    /// Saved local destination, which need not be the global tonic.
    pub destination: Option<Chord>,
    /// Prepared pitches currently waiting above their descending resolutions.
    pub suspended: [bool; SUPPORT_VOICES],
    /// The harmonic root has no literal or generated representative.
    pub root_implied: bool,
}

impl Realization {
    const SILENT: Self = Self {
        notes: [None; SUPPORT_VOICES],
        chord: None,
        destination: None,
        suspended: [false; SUPPORT_VOICES],
        root_implied: false,
    };

    /// Pitch classes of generated voices only.
    #[must_use]
    pub fn pitch_classes(self) -> PitchClassSet {
        pitches(self.notes.into_iter().flatten())
    }
}

/// Read-only state of the last explicit harmonic planning event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostics {
    /// Scored interpretation, before phrase-direction substitution.
    pub reference: Option<Chord>,
    /// Last actual realization, including delayed sounding voices.
    pub current: Realization,
    /// Exact resolution targets, indexed by stable supporting voice.
    pub resolved: [Option<u8>; SUPPORT_VOICES],
    /// Outstanding dominant resolution targets in their originating voices.
    pub tendencies: [Option<u8>; SUPPORT_VOICES],
    /// Preferences applied to the last nonempty realization; quiet releases do not change them.
    pub applied: Preferences,
    /// Unfulfilled requested outcomes: colour 1, foundation 2, motion 4, reharm 8.
    pub limited: u8,
}

/// Allocation-free phrase memory and supporting-voice planner.
#[derive(Debug, Clone)]
pub struct Realizer {
    key: Key,
    current: Realization,
    reference: Option<Chord>,
    source: PitchClassSet,
    resolved: [Option<u8>; SUPPORT_VOICES],
    suspensions: [Option<u8>; SUPPORT_VOICES],
    tendencies: [Option<u8>; SUPPORT_VOICES],
    recent: [Option<Chord>; 8],
    event: u32,
    applied: Preferences,
    limited: u8,
    pedal: Option<u8>,
}

impl Realizer {
    /// Starts an empty phrase in the supplied tonal home.
    #[must_use]
    pub const fn new(key: Key) -> Self {
        Self {
            key,
            current: Realization::SILENT,
            reference: None,
            source: PitchClassSet::from_bits(0),
            resolved: [None; SUPPORT_VOICES],
            suspensions: [None; SUPPORT_VOICES],
            tendencies: [None; SUPPORT_VOICES],
            recent: [None; 8],
            event: 0,
            applied: Preferences {
                colour: Colour::Natural,
                foundation: Foundation::Natural,
                motion: Motion::Nearest,
                reharm: Reharm::Faithful,
                direction: Direction::Continue,
                delay: false,
                pedal: false,
            },
            limited: 0,
            pedal: None,
        }
    }

    /// Clears phrase memory, prepared tones and all generated notes.
    pub fn clear(&mut self) {
        *self = Self::new(self.key);
    }

    /// Returns the last planned supporting realization without advancing it.
    #[must_use]
    pub const fn current(&self) -> Realization {
        self.current
    }

    /// Copies the last plan's context without advancing the phrase or search.
    #[must_use]
    pub const fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            reference: self.reference,
            current: self.current,
            resolved: self.resolved,
            tendencies: self.tendencies,
            applied: self.applied,
            limited: self.limited,
        }
    }

    /// Realizes one explicit playing event around exact, currently held MIDI notes.
    ///
    /// Invalid MIDI values are ignored. With no valid source notes, support is
    /// released but the phrase destination survives a breath between gestures.
    /// Two searches visit at most 300 chord recipes each; voice searches are bounded.
    pub fn plan(&mut self, held: &[u8], preferences: Preferences) -> Realization {
        if !preferences.pedal {
            self.pedal = None;
        }
        let source = pitches(held.iter().copied().filter(|note| *note <= 127));
        if source.is_empty() {
            self.current.notes = [None; SUPPORT_VOICES];
            self.current.suspended = [false; SUPPORT_VOICES];
            self.resolved = [None; SUPPORT_VOICES];
            self.suspensions = [None; SUPPORT_VOICES];
            self.tendencies = [None; SUPPORT_VOICES];
            self.source = source;
            return self.current;
        }
        self.applied = preferences;
        let old = self.current;
        let source_changed = source != self.source;
        let lowest = held
            .iter()
            .copied()
            .filter(|note| *note <= 127)
            .min()
            .unwrap_or(60);
        let mut reference = self.select_reference(source, lowest);
        // A single passing/neighbour tone may decorate an established collection.
        if source_changed
            && preferences.motion == Motion::Nearest
            && preferences.reharm == Reharm::Faithful
            && preferences.foundation == Foundation::Natural
        {
            if let Some(chord) = self.reference {
                let old_tones = chord.tones();
                let outside = source.bits() & !old_tones.bits();
                let retained = (source.bits() & self.source.bits()).count_ones();
                if outside.count_ones() == 1
                    && retained >= 2
                    && step_between_sets(PitchClassSet::from_bits(outside), old_tones)
                {
                    reference = chord;
                }
            }
        }
        let target = old.destination.unwrap_or(reference);
        let relation = if preferences.reharm == Reharm::AppliedDominant {
            target
        } else {
            reference
        };
        let selected = self.select_chord(source, lowest % 12, relation, preferences);
        let (chord, destination) = match preferences.direction {
            Direction::Continue | Direction::Substitute | Direction::Explore => {
                let chord = if preferences.direction == Direction::Explore {
                    old.chord
                        .filter(|chord| {
                            candidate_limitations(*chord, lowest % 12, relation, preferences) == 0
                        })
                        .unwrap_or(selected)
                } else {
                    selected
                };
                let destination = if preferences.reharm == Reharm::AppliedDominant
                    && reharm_matches(chord, relation, Reharm::AppliedDominant)
                {
                    Some(target)
                } else {
                    self.tonal_destination(chord).or(old.destination)
                };
                (chord, destination)
            }
            Direction::Prepare => (
                self.colour_at_root(
                    Chord {
                        root: (target.root + 2) % 12,
                        shape: if minor(target) {
                            Shape::HalfDiminished7
                        } else {
                            Shape::Minor7
                        },
                    },
                    source,
                    preferences.colour,
                ),
                Some(target),
            ),
            Direction::Lean => (
                Chord {
                    root: (target.root + 7) % 12,
                    shape: dominant_colour(preferences.colour, self.event),
                },
                Some(target),
            ),
            Direction::Arrive => (
                self.colour_at_root(target, source, preferences.colour),
                None,
            ),
            Direction::Deceive => (
                self.colour_at_root(
                    Chord {
                        root: (target.root + if minor(target) { 8 } else { 9 }) % 12,
                        shape: if minor(target) {
                            Shape::Major
                        } else {
                            Shape::Minor
                        },
                    },
                    source,
                    preferences.colour,
                ),
                None,
            ),
        };
        let palette = chord.tones();
        let mut notes = [None; SUPPORT_VOICES];
        let retaining_delay = preferences.delay && preferences.direction != Direction::Arrive;
        let mut suspended = [false; SUPPORT_VOICES];
        if retaining_delay {
            for voice in 1..SUPPORT_VOICES {
                if let Some(pitch) = self.suspensions[voice] {
                    notes[voice] = Some(pitch);
                    suspended[voice] = true;
                }
            }
        }
        let essential = chord.essential();
        let characteristic = characteristic(chord, preferences.colour);
        // A dense literal collection needs fewer doublings, but its requested
        // colour and missing structural pitches take precedence over density.
        let mut budget = match source.len() {
            1..=2 => 4,
            3..=4 => 3,
            _ => 2,
        };
        let forced_resolution = matches!(
            preferences.direction,
            Direction::Arrive | Direction::Deceive
        ) || (!preferences.delay
            && old.suspended.into_iter().any(|held| held));
        if forced_resolution {
            for (voice, note) in notes.iter_mut().enumerate() {
                let target = if old.suspended[voice] {
                    self.resolved[voice]
                } else if matches!(
                    preferences.direction,
                    Direction::Arrive | Direction::Deceive
                ) {
                    self.tendencies[voice]
                } else {
                    None
                };
                if let Some(target) = target.filter(|target| !held.contains(target)) {
                    *note = Some(target);
                }
            }
        }
        if preferences.pedal && self.pedal.is_none() {
            self.pedal = old.notes[0];
        }
        if let Some(pedal) = self.pedal.filter(|pitch| !held.contains(pitch)) {
            notes[0] = Some(pedal);
        }
        if notes[0].is_none() && self.pedal.is_none() {
            // One octave in the bass register gives each inversion one stable
            // location; repeated steering cannot walk the bass into sub-bass.
            let bass_ceiling = lowest.saturating_sub(1).min(52);
            let bass_floor = bass_ceiling.saturating_sub(11).max(28);
            let bass_class = if preferences.foundation == Foundation::Natural {
                chord.root
            } else {
                lowest % 12
            };
            notes[0] = self.choose_note(
                bass_class,
                bass_floor,
                bass_ceiling,
                0,
                held,
                &notes,
                preferences,
            );
        }
        if preferences.pedal && self.pedal.is_none() {
            self.pedal = notes[0];
        }
        let mut represented = source;
        for (voice, note) in notes.into_iter().enumerate() {
            if let Some(note) = note {
                represented.insert(note);
            }
            // A held suspension owns its target as well as its sounding slot:
            // another voice must not pre-empt the still-delayed resolution.
            if suspended[voice] {
                if let Some(target) = self.resolved[voice] {
                    represented.insert(target);
                }
            }
        }
        // A declared chord must contain its essential pitches. Optional upper
        // colours spend only the remaining budget, even for very sparse input.
        let missing = ((essential.bits() | characteristic.bits()) & !represented.bits())
            .count_ones() as usize;
        budget = budget.max(count(&notes) + missing).min(SUPPORT_VOICES);
        for desired in [characteristic, essential] {
            for offset in 0..12 {
                let class = (chord.root + offset) % 12;
                if !desired.contains(class) || represented.contains(class) {
                    continue;
                }
                let Some(voice) = self.available_voice(class, &notes) else {
                    break;
                };
                if count(&notes) >= budget {
                    break;
                }
                let (low, high) = register(voice);
                notes[voice] = self.choose_note(class, low, high, voice, held, &notes, preferences);
                if notes[voice].is_some() {
                    represented.insert(class);
                }
            }
        }
        // Essentials win the budget. Common-tone preservation then keeps its
        // actual voice identity; requested movement instead explores new motion.
        if preferences.motion == Motion::Nearest && preferences.direction != Direction::Explore {
            for voice in 1..SUPPORT_VOICES {
                if count(&notes) >= budget {
                    break;
                }
                if notes[voice].is_none() {
                    if let Some(note) = old.notes[voice].filter(|note| {
                        palette.contains(*note)
                            && !held.contains(note)
                            && !notes.contains(&Some(*note))
                    }) {
                        notes[voice] = Some(note);
                    }
                }
            }
        }
        let mut fill = [chord.third(), chord.fifth(), chord.root];
        for _ in 0..3 {
            if count(&notes) >= budget {
                break;
            }
            let Some(voice) = (1..SUPPORT_VOICES).find(|voice| notes[*voice].is_none()) else {
                break;
            };
            notes[voice] = self.fill_note(fill, voice, held, &notes, preferences);
            fill.rotate_left(1);
        }
        let mut resolved = notes;
        for voice in 1..SUPPORT_VOICES {
            if suspended[voice] {
                resolved[voice] = self.resolved[voice];
            }
        }
        if retaining_delay {
            for voice in 1..SUPPORT_VOICES {
                if suspended[voice] {
                    continue;
                }
                let (Some(before), Some(target)) = (old.notes[voice], notes[voice]) else {
                    continue;
                };
                let prepared = old
                    .chord
                    .is_some_and(|old_chord| old_chord.tones().contains(before))
                    || old.suspended[voice];
                if prepared
                    && before > target
                    && before - target <= 2
                    && !palette.contains(before)
                    && !held.contains(&before)
                    && !notes
                        .iter()
                        .enumerate()
                        .any(|(index, note)| index != voice && *note == Some(before))
                {
                    notes[voice] = Some(before);
                    suspended[voice] = true;
                }
            }
        }
        self.suspensions = [None; SUPPORT_VOICES];
        for voice in 1..SUPPORT_VOICES {
            if suspended[voice] {
                self.suspensions[voice] = notes[voice];
                if notes[voice].is_some_and(|note| held.contains(&note)) {
                    notes[voice] = None;
                }
            }
        }
        let sounding =
            PitchClassSet::from_bits(source.bits() | pitches(notes.into_iter().flatten()).bits());
        let next = Realization {
            notes,
            chord: Some(chord),
            destination,
            suspended,
            root_implied: !sounding.contains(chord.root),
        };
        self.limited = candidate_limitations(chord, lowest % 12, relation, preferences);
        if !characteristic.is_empty() && sounding.bits() & characteristic.bits() == 0 {
            self.limited |= 1;
        }
        if preferences.reharm == Reharm::Borrowed
            && sounding.bits() & chord.tones().bits() & !mode_pitches(reference).bits() == 0
        {
            self.limited |= 8;
        }
        if preferences.direction == Direction::Substitute && old.chord == next.chord {
            self.limited |= 8;
        }
        if !motion_fulfilled(old, next, preferences.motion) {
            self.limited |= 4;
        }
        self.tendencies = dominant_tendencies(next);
        self.resolved = resolved;
        self.current = next;
        self.reference = Some(reference);
        self.source = source;
        self.recent.rotate_right(1);
        self.recent[0] = Some(chord);
        self.event = self.event.wrapping_add(1);
        next
    }

    fn tonal_destination(&self, chord: Chord) -> Option<Chord> {
        natural_destination(chord).map(|mut destination| {
            if let Some(degree) = self.key.degree(destination.root) {
                if self.key.chord(degree, ChordKind::Triad).quality() == ChordQuality::Minor {
                    destination.shape = Shape::Minor;
                }
            }
            destination
        })
    }

    fn colour_at_root(&self, target: Chord, source: PitchClassSet, colour: Colour) -> Chord {
        if colour == Colour::Natural {
            return target;
        }
        SHAPES
            .into_iter()
            .map(|shape| Chord {
                root: target.root,
                shape,
            })
            .filter(|chord| {
                minor(*chord) == minor(target)
                    && (target.shape != Shape::HalfDiminished7 || chord.fifth() == target.fifth())
                    && !characteristic(*chord, colour).is_empty()
            })
            .min_by_key(|chord| self.literal_score(*chord, source, target.root))
            .unwrap_or(target)
    }

    fn select_reference(&self, source: PitchClassSet, lowest: u8) -> Chord {
        let mut best = Chord {
            root: lowest % 12,
            shape: Shape::Major,
        };
        let mut best_score = i32::MAX;
        for root in 0..12 {
            for shape in SHAPES {
                let chord = Chord { root, shape };
                let mut score = self.literal_score(chord, source, lowest % 12);
                if self.reference == Some(chord) {
                    score -= 22;
                }
                if score < best_score {
                    best_score = score;
                    best = chord;
                }
            }
        }
        best
    }

    fn literal_score(&self, chord: Chord, source: PitchClassSet, lowest: u8) -> i32 {
        let tones = chord.tones();
        let uncovered = (source.bits() & !tones.bits()).count_ones() as i32;
        let missing = (chord.essential().bits() & !source.bits()).count_ones() as i32;
        let chromatic = (tones.bits() & !self.key.pitches().bits()).count_ones() as i32;
        uncovered * 150 + missing * 28 + chromatic * 12 + complexity(chord.shape) * 16
            - i32::from(chord.root == lowest) * 18
    }

    fn select_chord(
        &self,
        source: PitchClassSet,
        lowest: u8,
        reference: Chord,
        preferences: Preferences,
    ) -> Chord {
        if preferences.colour == Colour::Natural
            && preferences.foundation == Foundation::Natural
            && preferences.reharm == Reharm::Faithful
            && preferences.direction != Direction::Substitute
        {
            return reference;
        }
        let mut best = reference;
        let mut best_score = i32::MAX;
        for root in 0..12 {
            for shape in SHAPES {
                let chord = Chord { root, shape };
                let limited = candidate_limitations(chord, lowest, reference, preferences);
                // A requested role/recipe is an admissibility condition. Only an
                // incompatible combination falls back, with a truthful limitation.
                let mut score = i32::from(limited & 2 != 0) * 300_000
                    + i32::from(limited & 1 != 0) * 200_000
                    + i32::from(limited & 8 != 0) * 100_000
                    + self.literal_score(chord, source, lowest);
                if preferences.foundation == Foundation::Seventh && root != (lowest + 2) % 12 {
                    score += 90;
                }
                if preferences.foundation == Foundation::Natural
                    && preferences.reharm == Reharm::Faithful
                {
                    if root != reference.root {
                        score += 180;
                    }
                    if minor(chord) != minor(reference) {
                        score += 45;
                    }
                }
                if preferences.direction == Direction::Substitute {
                    if self.current.chord == Some(chord) {
                        score += 50_000;
                    }
                    score += self
                        .recent
                        .iter()
                        .filter(|recent| **recent == Some(chord))
                        .count() as i32
                        * 18;
                } else if self.current.chord == Some(chord) {
                    score -= 14;
                }
                if score < best_score {
                    best = chord;
                    best_score = score;
                }
            }
        }
        best
    }

    fn available_voice(&self, class: u8, notes: &[Option<u8>; SUPPORT_VOICES]) -> Option<usize> {
        (1..SUPPORT_VOICES)
            .filter(|voice| notes[*voice].is_none())
            .min_by_key(|voice| {
                self.current.notes[*voice].map_or(12, |old| {
                    let interval = (old % 12).abs_diff(class);
                    interval.min(12 - interval)
                })
            })
    }

    fn fill_note(
        &self,
        classes: [u8; 3],
        voice: usize,
        held: &[u8],
        used: &[Option<u8>; SUPPORT_VOICES],
        preferences: Preferences,
    ) -> Option<u8> {
        let mut best = None;
        let mut best_score = i32::MAX;
        let old = self.current.notes[voice];
        let rotation = if preferences.direction == Direction::Explore {
            self.event as usize % 3
        } else {
            0
        };
        for index in 0..3 {
            let class = classes[(index + rotation) % 3];
            let (low, high) = register(voice);
            if let Some(note) = self.choose_note(class, low, high, voice, held, used, preferences) {
                let mut score = i32::from(note.abs_diff(old.unwrap_or(voice_centre(voice)))) * 3;
                if let Some(old) = old {
                    if note == old {
                        score -= 14;
                    }
                    if !motion_accepts(preferences.motion, old, note, self.upper_centre()) {
                        score += 10_000;
                    }
                    if preferences.direction == Direction::Explore && note == old {
                        score += 300;
                    }
                } else {
                    score += index as i32 * 100;
                }
                if score < best_score {
                    best_score = score;
                    best = Some(note);
                }
            }
        }
        best
    }

    fn upper_centre(&self) -> u8 {
        let (sum, count) = self.current.notes[1..]
            .iter()
            .flatten()
            .fold((0_u16, 0_u16), |(sum, count), note| {
                (sum + u16::from(*note), count + 1)
            });
        sum.checked_div(count).map_or(65, |centre| centre as u8)
    }

    #[allow(clippy::too_many_arguments)]
    fn choose_note(
        &self,
        class: u8,
        low: u8,
        high: u8,
        voice: usize,
        held: &[u8],
        used: &[Option<u8>; SUPPORT_VOICES],
        preferences: Preferences,
    ) -> Option<u8> {
        let old = self.current.notes[voice];
        let low = if voice > 0 && old.is_none() {
            low.max(
                held.iter()
                    .copied()
                    .filter(|note| *note <= 127)
                    .min()
                    .unwrap_or(44)
                    .saturating_add(1)
                    .min(72),
            )
        } else {
            low
        };
        let open = pitches(held.iter().copied().filter(|note| *note <= 127)).len() >= 3;
        let centre = if open && voice == 2 && old.is_none() {
            used[1].map_or(voice_centre(voice), |note| note.saturating_add(14).min(87))
        } else if open || voice == 0 {
            voice_centre(voice)
        } else {
            52 + voice as u8 * 5
        };
        let mut best = None;
        let mut best_score = i32::MAX;
        for note in low..=high {
            if note % 12 != class % 12
                || held.contains(&note)
                || used.contains(&Some(note))
                || (preferences.delay
                    && self
                        .suspensions
                        .iter()
                        .zip(self.resolved)
                        .any(|(suspension, target)| {
                            suspension.is_some()
                                && target.is_some_and(|target| target % 12 == note % 12)
                        }))
            {
                continue;
            }
            let mut score = i32::from(note.abs_diff(old.unwrap_or(centre))) * 3;
            if old == Some(note) {
                score -= 14;
            }
            if voice > 0 && old.is_some() {
                let old = old.unwrap_or(note);
                if !motion_accepts(preferences.motion, old, note, self.upper_centre()) {
                    score += 10_000;
                }
                if preferences.direction == Direction::Explore && note == old {
                    score += 300;
                }
            }
            for (other, next) in used.iter().enumerate() {
                if let (Some(old_a), Some(old_b), Some(new_b)) =
                    (old, self.current.notes[other], *next)
                {
                    // Reuse the interval-motion check; this helper inspects only slots 0/1.
                    if parallel_pair(
                        Voicing {
                            notes: [old_a, old_b, 0, 0],
                        },
                        Voicing {
                            notes: [note, new_b, 0, 0],
                        },
                        0,
                        1,
                    ) {
                        score += 100;
                    }
                }
            }
            if score < best_score {
                best_score = score;
                best = Some(note);
            }
        }
        best
    }
}

fn pitches(notes: impl Iterator<Item = u8>) -> PitchClassSet {
    let mut set = PitchClassSet::default();
    for note in notes {
        set.insert(note);
    }
    set
}

fn count(notes: &[Option<u8>; SUPPORT_VOICES]) -> usize {
    notes.iter().flatten().count()
}

fn minor(chord: Chord) -> bool {
    (chord.third() + 12 - chord.root) % 12 == 3
}

fn complexity(shape: Shape) -> i32 {
    match shape {
        Shape::Major | Shape::Minor => 0,
        Shape::Diminished | Shape::Augmented | Shape::Sus2 | Shape::Sus4 => 1,
        Shape::Major6 | Shape::Minor6 | Shape::Add9 | Shape::MinorAdd9 => 2,
        Shape::Major7 | Shape::Minor7 | Shape::Dominant7 | Shape::HalfDiminished7 => 3,
        _ => 4,
    }
}

fn characteristic(chord: Chord, colour: Colour) -> PitchClassSet {
    let mut result = PitchClassSet::default();
    let interval = match colour {
        Colour::Natural => None,
        Colour::SecondsFourths => match chord.shape {
            Shape::Sus4 | Shape::DominantSus4 => Some(5),
            Shape::Sus2
            | Shape::Add9
            | Shape::MinorAdd9
            | Shape::Major9
            | Shape::Minor9
            | Shape::Dominant9 => Some(2),
            _ => None,
        },
        Colour::Sixth => match chord.shape {
            Shape::Major6 | Shape::Minor6 | Shape::Major13 | Shape::Dominant13 => Some(9),
            _ => None,
        },
        Colour::Seventh => chord.seventh().map(|class| (class + 12 - chord.root) % 12),
        Colour::Extended => match chord.shape {
            Shape::Major9 | Shape::Minor9 | Shape::Dominant9 => Some(2),
            Shape::Major13 | Shape::Dominant13 => Some(9),
            Shape::DominantFlat9 => Some(1),
            Shape::DominantSharp9 => Some(3),
            Shape::DominantFlat13 => Some(8),
            _ => None,
        },
    };
    if let Some(interval) = interval {
        result.insert(chord.root + interval);
    }
    result
}

fn foundation_matches(chord: Chord, lowest: u8, foundation: Foundation) -> bool {
    match foundation {
        Foundation::Natural => true,
        Foundation::Root => chord.root == lowest,
        Foundation::Third => {
            chord.third() == lowest && matches!((lowest + 12 - chord.root) % 12, 3 | 4)
        }
        Foundation::Fifth => chord.fifth() == lowest,
        Foundation::Seventh => chord.seventh() == Some(lowest),
    }
}

fn reharm_matches(chord: Chord, reference: Chord, reharm: Reharm) -> bool {
    match reharm {
        Reharm::Faithful => true,
        Reharm::Relative => {
            chord.root == (reference.root + if minor(reference) { 3 } else { 9 }) % 12
                && minor(chord) != minor(reference)
        }
        Reharm::CommonTone => {
            chord.root != reference.root
                && (chord.tones().bits() & reference.tones().bits()).count_ones() >= 2
        }
        Reharm::Borrowed => {
            let interval = (chord.root + 12 - reference.root) % 12;
            let third = (chord.third() + 12 - chord.root) % 12;
            let mixture = if minor(reference) {
                matches!(interval, 0 | 5 | 7) && third == 4
                    || matches!(interval, 2 | 9) && third == 3
            } else {
                matches!(interval, 0 | 5) && third == 3 || matches!(interval, 8 | 10) && third == 4
            };
            mixture && chord.tones().bits() & !mode_pitches(reference).bits() != 0
        }
        Reharm::AppliedDominant => {
            chord.root == (reference.root + 7) % 12
                && natural_destination(chord).is_some()
                && (chord.third() + 12 - chord.root) % 12 == 4
        }
    }
}

fn candidate_limitations(
    chord: Chord,
    lowest: u8,
    reference: Chord,
    preferences: Preferences,
) -> u8 {
    let reharm = if preferences.direction == Direction::Substitute
        && preferences.reharm == Reharm::Faithful
    {
        Reharm::CommonTone
    } else {
        preferences.reharm
    };
    u8::from(
        preferences.colour != Colour::Natural
            && characteristic(chord, preferences.colour).is_empty(),
    ) | (u8::from(!foundation_matches(chord, lowest, preferences.foundation)) << 1)
        | (u8::from(!reharm_matches(chord, reference, reharm)) << 3)
}

fn dominant_colour(colour: Colour, event: u32) -> Shape {
    match colour {
        Colour::Natural | Colour::Seventh => Shape::Dominant7,
        Colour::SecondsFourths => Shape::DominantSus4,
        Colour::Sixth => Shape::Dominant13,
        Colour::Extended => [
            Shape::Dominant9,
            Shape::Dominant13,
            Shape::DominantFlat9,
            Shape::DominantSharp9,
        ][event as usize % 4],
    }
}

fn motion_accepts(motion: Motion, before: u8, after: u8, centre: u8) -> bool {
    match motion {
        Motion::Nearest => true,
        Motion::Falling => after < before,
        Motion::Rising => after > before,
        Motion::Inward => after.abs_diff(centre) < before.abs_diff(centre),
        Motion::Outward => after.abs_diff(centre) > before.abs_diff(centre),
    }
}

fn motion_fulfilled(before: Realization, after: Realization, motion: Motion) -> bool {
    if motion == Motion::Nearest {
        return true;
    }
    let (sum, count) = before.notes[1..]
        .iter()
        .flatten()
        .fold((0_u16, 0_u16), |(sum, count), note| {
            (sum + u16::from(*note), count + 1)
        });
    if count == 0 {
        return true;
    }
    let centre = (sum / count) as u8;
    let mut moved = false;
    for voice in 1..SUPPORT_VOICES {
        if let (Some(old), Some(new)) = (before.notes[voice], after.notes[voice]) {
            if old != new {
                if !motion_accepts(motion, old, new, centre) {
                    return false;
                }
                moved = true;
            }
        }
    }
    moved
}

fn mode_pitches(reference: Chord) -> PitchClassSet {
    let intervals = if minor(reference) {
        [0, 2, 3, 5, 7, 8, 10]
    } else {
        [0, 2, 4, 5, 7, 9, 11]
    };
    pitches(
        intervals
            .into_iter()
            .map(|interval| (reference.root + interval) % 12),
    )
}

fn voice_centre(voice: usize) -> u8 {
    // Alternating open registers leave room to contract as well as to expand.
    [40, 60, 74, 67, 79, 86][voice]
}

fn register(voice: usize) -> (u8, u8) {
    if voice == 0 {
        (28, 52)
    } else {
        (45, 93)
    }
}

fn natural_destination(chord: Chord) -> Option<Chord> {
    match chord.shape {
        Shape::Dominant7
        | Shape::Dominant9
        | Shape::Dominant13
        | Shape::DominantFlat9
        | Shape::DominantSharp9
        | Shape::DominantFlat13
        | Shape::DominantSus4 => Some(Chord {
            root: (chord.root + 5) % 12,
            shape: Shape::Major,
        }),
        _ => None,
    }
}

fn dominant_tendencies(realization: Realization) -> [Option<u8>; SUPPORT_VOICES] {
    let mut tendencies = [None; SUPPORT_VOICES];
    let (Some(chord), Some(destination)) = (realization.chord, realization.destination) else {
        return tendencies;
    };
    if natural_destination(chord).is_none() || (chord.root + 5) % 12 != destination.root {
        return tendencies;
    }
    for (voice, note) in realization.notes.into_iter().enumerate() {
        let Some(note) = note else {
            continue;
        };
        let target_class = if note % 12 == chord.third() {
            Some(destination.root)
        } else if Some(note % 12) == chord.seventh() {
            Some(destination.third())
        } else {
            None
        };
        if let Some(class) = target_class {
            for delta in [-2_i16, -1, 1, 2] {
                let target = i16::from(note) + delta;
                if (0..=127).contains(&target) && target as u8 % 12 == class {
                    tendencies[voice] = Some(target as u8);
                    break;
                }
            }
        }
    }
    tendencies
}

fn step_between_sets(a: PitchClassSet, b: PitchClassSet) -> bool {
    (0..12).any(|class| a.contains(class) && (b.contains(class + 1) || b.contains(class + 11)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(direction: Direction) -> Preferences {
        Preferences {
            direction,
            ..Preferences::default()
        }
    }

    fn chord(root: u8, shape: Shape) -> Chord {
        Chord { root, shape }
    }

    #[test]
    fn complete_literal_seventh_keeps_its_identity() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let plan = player.plan(&[52, 55, 59, 62], Preferences::default());
        assert_eq!(plan.chord, Some(chord(4, Shape::Minor7)));
        assert!(plan
            .notes
            .into_iter()
            .flatten()
            .all(|note| ![52, 55, 59, 62].contains(&note)));
    }

    #[test]
    fn secondary_dominant_remembers_minor_destination_and_exact_resolutions() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let source = [45, 60, 64];
        let home = player.plan(&source, Preferences::default());
        assert_eq!(home.chord, Some(chord(9, Shape::Minor)));
        let prepared = player.plan(&source, request(Direction::Prepare));
        assert_eq!(prepared.chord, Some(chord(11, Shape::HalfDiminished7)));
        assert_eq!(prepared.destination, home.chord);
        let dominant = player.plan(&source, request(Direction::Lean));
        assert_eq!(dominant.chord, Some(chord(4, Shape::Dominant7)));
        assert_eq!(dominant.destination, home.chord);
        let obligations = player.tendencies;
        assert!(
            obligations.into_iter().flatten().count() >= 2,
            "{dominant:?}"
        );
        let arrival = player.plan(&source, request(Direction::Arrive));
        assert_eq!(arrival.chord, home.chord);
        assert_eq!(arrival.destination, None);
        for (voice, target) in obligations.into_iter().enumerate() {
            if let Some(target) = target.filter(|target| !source.contains(target)) {
                assert_eq!(arrival.notes[voice], Some(target));
                assert!(dominant.notes[voice].unwrap().abs_diff(target) <= 2);
            }
        }
    }

    #[test]
    fn held_manual_dissonance_survives_an_arrival() {
        let mut player = Realizer::new(Key::C_MAJOR);
        player.plan(&[48, 52, 55], Preferences::default());
        player.plan(&[48, 52, 55], request(Direction::Lean));
        let plan = player.plan(&[48, 49, 52, 55], request(Direction::Arrive));
        assert_eq!(plan.chord.unwrap().root, 0);
        assert!(!plan
            .notes
            .into_iter()
            .flatten()
            .any(|note| [48, 49, 52, 55].contains(&note)));
    }

    #[test]
    fn prepared_suspension_resolves_in_its_original_voice() {
        let mut player = Realizer::new(Key::C_MAJOR);
        player.plan(&[53, 57], Preferences::default());
        let before = player.current();
        let suspended = player.plan(
            &[55, 62],
            Preferences {
                delay: true,
                ..Preferences::default()
            },
        );
        assert!(
            suspended.suspended.into_iter().any(|held| held),
            "before={before:?}, after={suspended:?}"
        );
        let targets = player.resolved;
        let released = player.plan(&[55, 62], Preferences::default());
        for (voice, held) in suspended.suspended.into_iter().enumerate() {
            if held {
                assert_eq!(released.notes[voice], targets[voice]);
            }
        }
        assert!(!released.suspended.into_iter().any(|held| held));
    }

    #[test]
    fn passing_neighbour_preserves_the_established_harmony() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let initial = player.plan(&[48, 52, 55], Preferences::default());
        let neighbour = player.plan(&[48, 53, 55], Preferences::default());
        assert_eq!(initial.chord, neighbour.chord);
        let returned = player.plan(&[48, 52, 55], Preferences::default());
        assert_eq!(returned.chord, initial.chord);
    }

    #[test]
    fn breath_releases_sound_and_preserves_the_destination() {
        let mut player = Realizer::new(Key::C_MAJOR);
        player.plan(&[57, 60, 64], Preferences::default());
        let approach = player.plan(&[57, 60, 64], request(Direction::Lean));
        let breath = player.plan(&[], Preferences::default());
        assert_eq!(count(&breath.notes), 0);
        assert_eq!(breath.destination, approach.destination);
        let arrival = player.plan(&[57, 60, 64], request(Direction::Arrive));
        assert_eq!(arrival.chord, approach.destination);
    }

    #[test]
    fn clear_forgets_the_entire_phrase() {
        let mut player = Realizer::new(Key::C_MAJOR);
        player.plan(&[60], request(Direction::Lean));
        player.clear();
        assert_eq!(player.current(), Realization::SILENT);
        assert_eq!(
            player
                .plan(&[57, 60, 64], Preferences::default())
                .chord
                .unwrap()
                .root,
            9
        );
    }

    #[test]
    fn manually_played_secondary_dominant_infers_the_diatonic_minor_destination() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let dominant = player.plan(&[52, 56, 59, 62], Preferences::default());
        assert_eq!(dominant.chord, Some(chord(4, Shape::Dominant7)));
        assert_eq!(dominant.destination, Some(chord(9, Shape::Minor)));
        let arrival = player.plan(&[57, 60, 64], request(Direction::Arrive));
        assert_eq!(arrival.chord, Some(chord(9, Shape::Minor)));
    }
    #[test]
    fn delayed_voice_and_target_survive_further_steering() {
        let mut player = Realizer::new(Key::C_MAJOR);
        player.plan(&[53, 57], Preferences::default());
        let caught = player.plan(
            &[55, 62],
            Preferences {
                delay: true,
                ..Preferences::default()
            },
        );
        let voice = caught.suspended.iter().position(|held| *held).unwrap();
        let held = caught.notes[voice];
        let target = player.resolved[voice];
        assert_eq!(held, Some(60));
        assert_eq!(target, Some(59));
        for motion in [
            Motion::Falling,
            Motion::Rising,
            Motion::Inward,
            Motion::Outward,
            Motion::Nearest,
        ] {
            let continued = player.plan(
                &[55, 62],
                Preferences {
                    delay: true,
                    motion,
                    ..Preferences::default()
                },
            );
            assert_eq!(continued.notes[voice], held);
            assert_eq!(player.resolved[voice], target);
            assert!(continued.suspended[voice]);
            assert!(
                !continued.notes.contains(&target),
                "resolution must remain delayed"
            );
        }
        let released = player.plan(&[55, 62], Preferences::default());
        assert_eq!(released.notes[voice], target);
        assert!(!released.suspended[voice]);
    }

    #[test]
    fn literal_owner_can_share_a_delayed_pitch_without_stealing_its_memory() {
        let mut player = Realizer::new(Key::C_MAJOR);
        player.plan(&[53, 57], Preferences::default());
        let caught = player.plan(
            &[55, 62],
            Preferences {
                delay: true,
                ..Preferences::default()
            },
        );
        let voice = caught.suspended.iter().position(|held| *held).unwrap();
        let delayed_pitch = caught.notes[voice].unwrap();
        let resolution = player.resolved[voice];
        let shared = player.plan(
            &[55, delayed_pitch, 62],
            Preferences {
                delay: true,
                ..Preferences::default()
            },
        );
        assert_eq!(shared.notes[voice], None);
        assert!(shared.suspended[voice]);
        let continued = player.plan(
            &[55, 62],
            Preferences {
                delay: true,
                ..Preferences::default()
            },
        );
        assert_eq!(continued.notes[voice], Some(delayed_pitch));
        let released = player.plan(&[55, 62], Preferences::default());
        assert_eq!(released.notes[voice], resolution);
    }
    #[test]
    fn diagnostics_copy_applied_preferences_and_targets_without_advancing_the_phrase() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let initial = player.diagnostics();
        assert_eq!(initial.current, Realization::SILENT);
        assert_eq!(initial.applied, Preferences::default());
        player.plan(&[53, 57], Preferences::default());
        let held = Preferences {
            delay: true,
            ..Preferences::default()
        };
        let plan = player.plan(&[55, 62], held);
        let diagnostics = player.diagnostics();
        assert_eq!(diagnostics.current, plan);
        assert_eq!(diagnostics.applied, held);
        let voice = plan.suspended.iter().position(|held| *held).unwrap();
        let pitch = plan.notes[voice].unwrap();
        let target = diagnostics.resolved[voice].unwrap();
        assert!(pitch > target && pitch - target <= 2);
        assert_eq!(player.diagnostics(), diagnostics);
        player.plan(
            &[],
            Preferences {
                colour: Colour::Extended,
                ..Preferences::default()
            },
        );
        assert_eq!(
            player.diagnostics().applied,
            held,
            "a silent breath has no new audible realization"
        );
        player.clear();
        assert_eq!(player.diagnostics(), initial);
    }

    const COLOURS: [Colour; 5] = [
        Colour::Natural,
        Colour::SecondsFourths,
        Colour::Sixth,
        Colour::Seventh,
        Colour::Extended,
    ];
    const FOUNDATIONS: [Foundation; 5] = [
        Foundation::Natural,
        Foundation::Root,
        Foundation::Third,
        Foundation::Fifth,
        Foundation::Seventh,
    ];
    const MOTIONS: [Motion; 5] = [
        Motion::Nearest,
        Motion::Falling,
        Motion::Rising,
        Motion::Inward,
        Motion::Outward,
    ];
    const REHARMS: [Reharm; 5] = [
        Reharm::Faithful,
        Reharm::Relative,
        Reharm::CommonTone,
        Reharm::Borrowed,
        Reharm::AppliedDominant,
    ];

    fn assert_literal_safe(plan: Realization, held: &[u8]) {
        for (voice, note) in plan.notes.into_iter().enumerate() {
            if let Some(note) = note {
                assert!(!held.contains(&note), "manual duplicate: {plan:?}");
                assert!(
                    !plan.notes[voice + 1..].contains(&Some(note)),
                    "support duplicate: {plan:?}"
                );
                assert!(note <= 127);
            }
        }
    }

    fn sounding(plan: Realization, held: &[u8]) -> PitchClassSet {
        PitchClassSet::from_bits(plan.pitch_classes().bits() | pitches(held.iter().copied()).bits())
    }

    #[test]
    fn every_requested_colour_has_a_characteristic_pitch_in_all_twelve_major_and_minor_keys() {
        for root in 0..12 {
            for third in [3, 4] {
                let held = [48 + root, 48 + root + third, 48 + root + 7];
                let mut player = Realizer::new(Key::C_MAJOR);
                let plain = player.plan(&held, Preferences::default());
                assert_eq!(plain.chord.unwrap().root, root);
                for colour in COLOURS.into_iter().skip(1) {
                    let plan = player.plan(
                        &held,
                        Preferences {
                            colour,
                            ..Preferences::default()
                        },
                    );
                    assert_eq!(plan.chord.unwrap().root, root, "{held:?}: {plan:?}");
                    let colour_notes = characteristic(plan.chord.unwrap(), colour);
                    assert!(!colour_notes.is_empty(), "{colour:?}: {plan:?}");
                    assert_ne!(colour_notes.bits() & sounding(plan, &held).bits(), 0);
                    assert_ne!(plan.notes, plain.notes, "{colour:?}: {plan:?}");
                    assert_eq!(player.limited & 1, 0);
                    assert_literal_safe(plan, &held);
                }
            }
        }
    }

    #[test]
    fn foundation_reinterprets_the_literal_bass_role_across_all_twelve_keys() {
        for root in 0..12 {
            let held = [48 + root, 52 + root, 55 + root];
            for (foundation, expected_root) in [
                (Foundation::Root, root),
                (Foundation::Third, (root + 9) % 12),
                (Foundation::Fifth, (root + 5) % 12),
                (Foundation::Seventh, (root + 2) % 12),
            ] {
                let mut player = Realizer::new(Key::C_MAJOR);
                let plan = player.plan(
                    &held,
                    Preferences {
                        foundation,
                        ..Preferences::default()
                    },
                );
                assert!(
                    foundation_matches(plan.chord.unwrap(), root, foundation),
                    "{foundation:?}, {held:?}: {plan:?}"
                );
                assert_eq!(
                    plan.chord.unwrap().root,
                    expected_root,
                    "{foundation:?}, {held:?}: {plan:?}"
                );
                assert_eq!(
                    plan.notes[0].map(|note| note % 12),
                    Some(root),
                    "{foundation:?}/{held:?}: {plan:?}"
                );
                let actual_lowest = held
                    .iter()
                    .copied()
                    .chain(plan.notes.into_iter().flatten())
                    .min()
                    .unwrap();
                assert_eq!(
                    actual_lowest % 12,
                    root,
                    "played foundation must remain the sounding bass"
                );
                assert!(
                    sounding(plan, &held).contains(expected_root),
                    "harmonic root must be represented upstairs: {plan:?}"
                );
                assert_eq!(
                    plan.chord.unwrap().essential().bits() & !sounding(plan, &held).bits(),
                    0
                );
                assert_eq!(player.limited & 2, 0);
                assert_literal_safe(plan, &held);
            }
        }
    }

    #[test]
    fn role_and_colour_requests_combine_without_losing_characteristic_pitches() {
        for root in 0..12 {
            let held = [48 + root, 52 + root, 55 + root];
            for foundation in FOUNDATIONS {
                for colour in COLOURS {
                    let mut player = Realizer::new(Key::C_MAJOR);
                    let plan = player.plan(
                        &held,
                        Preferences {
                            foundation,
                            colour,
                            ..Preferences::default()
                        },
                    );
                    assert!(
                        foundation_matches(plan.chord.unwrap(), root, foundation),
                        "{foundation:?}/{colour:?}: {plan:?}"
                    );
                    let essential = plan.chord.unwrap().essential();
                    assert_eq!(
                        essential.bits() & !sounding(plan, &held).bits(),
                        0,
                        "{foundation:?}/{colour:?}: {plan:?}"
                    );
                    if player.limited & 1 == 0 && colour != Colour::Natural {
                        assert_ne!(
                            characteristic(plan.chord.unwrap(), colour).bits()
                                & sounding(plan, &held).bits(),
                            0
                        );
                    }
                    assert_literal_safe(plan, &held);
                }
            }
        }
    }

    #[test]
    fn literal_sevenths_and_dense_grips_admit_colour_without_removing_manual_notes() {
        for held in [
            &[38, 41, 48][..],
            &[52, 55, 59, 62][..],
            &[48, 52, 55, 59, 62][..],
            &[35, 47, 48, 49, 52, 55, 59, 62][..],
        ] {
            let mut player = Realizer::new(Key::C_MAJOR);
            for colour in COLOURS {
                let plan = player.plan(
                    held,
                    Preferences {
                        colour,
                        ..Preferences::default()
                    },
                );
                assert_literal_safe(plan, held);
                assert_eq!(
                    plan.chord.unwrap().essential().bits() & !sounding(plan, held).bits(),
                    0,
                    "{held:?}/{colour:?}: {plan:?}"
                );
                if colour != Colour::Natural {
                    assert_eq!(player.limited & 1, 0, "{held:?}/{colour:?}: {plan:?}");
                }
            }
        }
    }

    #[test]
    fn reharms_obey_their_actual_harmonic_relations() {
        for root in 0..12 {
            for third in [3, 4] {
                let held = [48 + root, 48 + root + third, 55 + root];
                let mut player = Realizer::new(Key::C_MAJOR);
                let plain = player.plan(&held, Preferences::default());
                let reference = plain.chord.unwrap();
                for reharm in REHARMS.into_iter().skip(1) {
                    let plan = player.plan(
                        &held,
                        Preferences {
                            reharm,
                            ..Preferences::default()
                        },
                    );
                    assert!(
                        reharm_matches(plan.chord.unwrap(), reference, reharm),
                        "{held:?}/{reharm:?}: {plan:?}"
                    );
                    assert_ne!(plan.chord, plain.chord);
                    assert_ne!(plan.notes, plain.notes);
                    assert_eq!(player.limited & 8, 0);
                    assert_literal_safe(plan, &held);
                }
            }
        }
    }

    #[test]
    fn directional_motion_changes_actual_upper_voice_contours_in_all_twelve_keys() {
        for root in 0..12 {
            let held = [48 + root, 52 + root, 55 + root];
            for motion in MOTIONS.into_iter().skip(1) {
                let mut player = Realizer::new(Key::C_MAJOR);
                let before = player.plan(&held, Preferences::default());
                let after = player.plan(
                    &held,
                    Preferences {
                        motion,
                        ..Preferences::default()
                    },
                );
                assert!(
                    motion_fulfilled(before, after, motion),
                    "{held:?}/{motion:?}: {before:?} -> {after:?}"
                );
                assert_eq!(player.limited & 4, 0);
                assert_eq!(
                    before.notes[0], after.notes[0],
                    "motion must not churn the bass"
                );
                assert_literal_safe(after, &held);
            }
        }
    }

    #[test]
    fn repeated_requests_keep_the_bass_above_sub_bass_and_stop_at_register_limits() {
        for held in [&[45][..], &[48, 52, 55][..]] {
            let mut player = Realizer::new(Key::C_MAJOR);
            for _ in 0..12 {
                for foundation in FOUNDATIONS {
                    for motion in MOTIONS {
                        let plan = player.plan(
                            held,
                            Preferences {
                                foundation,
                                motion,
                                ..Preferences::default()
                            },
                        );
                        assert!(
                            plan.notes[0]
                                .is_none_or(|note| (28..=52).contains(&note) && note < held[0]),
                            "{plan:?}"
                        );
                        assert_literal_safe(plan, held);
                    }
                }
            }
        }
        let mut player = Realizer::new(Key::C_MAJOR);
        let mut before = player.plan(&[48, 52, 55], Preferences::default());
        for _ in 0..8 {
            let after = player.plan(
                &[48, 52, 55],
                Preferences {
                    motion: Motion::Rising,
                    ..Preferences::default()
                },
            );
            assert_eq!(
                player.limited & 4 == 0,
                motion_fulfilled(before, after, Motion::Rising)
            );
            before = after;
        }
    }

    #[test]
    fn low_literal_bass_omits_sub_bass_without_reusing_its_slot_as_a_treble() {
        for low in 0..=28 {
            let mut player = Realizer::new(Key::C_MAJOR);
            let plan = player.plan(&[low], Preferences::default());
            assert_eq!(plan.notes[0], None, "{plan:?}");
            assert!(count(&plan.notes) >= 2);
            assert!(plan.notes.into_iter().flatten().all(|note| note >= 45));
        }
    }

    #[test]
    fn pedal_keeps_its_actual_generated_pitch_through_harmonic_and_literal_changes() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let first = player.plan(&[48, 52, 55], Preferences::default());
        let bass = first.notes[0];
        for held in [&[45, 52, 60][..], &[53, 57, 60][..], &[55, 59, 62][..]] {
            let plan = player.plan(
                held,
                Preferences {
                    pedal: true,
                    colour: Colour::Seventh,
                    ..Preferences::default()
                },
            );
            assert_eq!(plan.notes[0], bass);
        }
        let released = player.plan(&[55, 59, 62], Preferences::default());
        assert_ne!(released.notes[0], bass);
    }

    #[test]
    fn deceptive_cadence_resolves_g_dominant_to_a_minor_without_removing_literal_notes() {
        let mut player = Realizer::new(Key::C_MAJOR);
        player.plan(&[43, 53, 59, 62], Preferences::default());
        let plan = player.plan(&[43, 53, 59, 62], request(Direction::Deceive));
        assert_eq!(plan.chord, Some(chord(9, Shape::Minor)));
        assert!(sounding(plan, &[43, 53, 59, 62]).contains(9));
        assert_literal_safe(plan, &[43, 53, 59, 62]);
    }

    #[test]
    fn explore_varies_voicing_and_substitute_varies_interpretation() {
        let held = [48, 52, 55];
        let mut player = Realizer::new(Key::C_MAJOR);
        let plain = player.plan(&held, Preferences::default());
        let explored = player.plan(&held, request(Direction::Explore));
        assert_eq!(plain.chord, explored.chord);
        assert_ne!(plain.notes, explored.notes);
        assert_eq!(plain.notes[0], explored.notes[0]);
        let mut prior = explored.chord;
        for _ in 0..8 {
            let plan = player.plan(&held, request(Direction::Substitute));
            assert_ne!(plan.chord, prior);
            assert!(reharm_matches(
                plan.chord.unwrap(),
                plain.chord.unwrap(),
                Reharm::CommonTone
            ));
            assert_eq!(player.limited & 8, 0);
            prior = plan.chord;
        }
    }

    #[test]
    fn incompatible_requests_are_reported_from_the_actual_chord() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let prefs = Preferences {
            foundation: Foundation::Root,
            reharm: Reharm::Relative,
            ..Preferences::default()
        };
        let plan = player.plan(&[48, 52, 55], prefs);
        assert_eq!(plan.chord.unwrap().root, 0);
        assert_eq!(player.limited, 8);
        let snapshot = player.diagnostics();
        player.plan(&[], Preferences::default());
        assert_eq!(player.diagnostics().limited, snapshot.limited);
    }

    #[test]
    fn actual_chromatic_phrases_preserve_literal_notes_and_structural_coverage() {
        let phrase: [&[u8]; 9] = [
            &[48, 52, 55],
            &[48, 53, 55],
            &[48, 52, 55],
            &[45, 52, 60],
            &[47, 53, 56],
            &[52, 56, 59, 62],
            &[45, 52, 60],
            &[43, 53, 59],
            &[48, 52, 60],
        ];
        let mut player = Realizer::new(Key::C_MAJOR);
        for (index, held) in phrase.into_iter().cycle().take(90).enumerate() {
            let plan = player.plan(
                held,
                Preferences {
                    colour: COLOURS[index % 5],
                    foundation: FOUNDATIONS[(index / 5) % 5],
                    motion: MOTIONS[(index / 25) % 5],
                    reharm: REHARMS[(index / 3) % 5],
                    ..Preferences::default()
                },
            );
            assert_literal_safe(plan, held);
            assert!(count(&plan.notes) >= 2, "{held:?}: {plan:?}");
            assert_eq!(
                plan.chord.unwrap().essential().bits() & !sounding(plan, held).bits(),
                0,
                "{held:?}: {plan:?}"
            );
        }
    }

    #[test]
    fn invalid_midi_values_are_ignored() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let plan = player.plan(
            &[0, 127, 128, 255],
            Preferences {
                colour: Colour::Extended,
                direction: Direction::Lean,
                ..Preferences::default()
            },
        );
        assert_literal_safe(plan, &[0, 127, 128, 255]);
        assert_eq!(CHORD_CANDIDATES, SHAPES.len() * 12);
        assert_eq!(
            player.plan(&[128, 255], Preferences::default()).notes,
            [None; SUPPORT_VOICES]
        );
    }
    #[test]
    fn borrowed_fifth_is_minor_subdominant_and_contains_the_actual_modal_alteration() {
        for root in 0..12 {
            let held = [48 + root, 52 + root, 55 + root];
            let mut player = Realizer::new(Key::C_MAJOR);
            let plan = player.plan(
                &held,
                Preferences {
                    foundation: Foundation::Fifth,
                    reharm: Reharm::Borrowed,
                    ..Preferences::default()
                },
            );
            let borrowed = plan.chord.unwrap();
            assert_eq!(borrowed.root, (root + 5) % 12, "{held:?}: {plan:?}");
            assert_eq!(borrowed.third(), (root + 8) % 12);
            assert!(
                sounding(plan, &held).contains(root + 8),
                "actual iv third must sound"
            );
            assert_eq!(player.limited & 8, 0);
            assert_literal_safe(plan, &held);
        }
    }

    #[test]
    fn arrival_and_deception_keep_requested_colour_at_the_functional_root() {
        for direction in [Direction::Arrive, Direction::Deceive] {
            for colour in COLOURS.into_iter().skip(1) {
                let mut player = Realizer::new(Key::C_MAJOR);
                let held = [48, 52, 55];
                player.plan(&held, Preferences::default());
                player.plan(&held, request(Direction::Lean));
                let plan = player.plan(
                    &held,
                    Preferences {
                        colour,
                        direction,
                        ..Preferences::default()
                    },
                );
                assert_eq!(
                    plan.chord.unwrap().root,
                    if direction == Direction::Arrive { 0 } else { 9 }
                );
                assert!(!characteristic(plan.chord.unwrap(), colour).is_empty());
                assert_ne!(
                    characteristic(plan.chord.unwrap(), colour).bits()
                        & sounding(plan, &held).bits(),
                    0,
                    "{direction:?}/{colour:?}: {plan:?}"
                );
                assert_eq!(player.limited & 1, 0);
                assert_literal_safe(plan, &held);
            }
        }
    }

    #[test]
    fn applied_dominant_keeps_its_saved_destination_consistent_after_a_source_change() {
        let mut player = Realizer::new(Key::C_MAJOR);
        player.plan(&[48, 52, 55], Preferences::default());
        player.plan(&[48, 52, 55], request(Direction::Lean));
        let held = [43, 59, 62];
        let applied = player.plan(
            &held,
            Preferences {
                reharm: Reharm::AppliedDominant,
                ..Preferences::default()
            },
        );
        let dominant = applied.chord.unwrap();
        assert_eq!((dominant.root + 5) % 12, applied.destination.unwrap().root);
        assert_eq!((dominant.third() + 12 - dominant.root) % 12, 4);
        assert!(dominant.seventh().is_some());
        let arrived = player.plan(&held, request(Direction::Arrive));
        assert_eq!(
            arrived.chord.unwrap().root,
            applied.destination.unwrap().root
        );
    }
    #[test]
    fn predominant_combines_available_colour_without_losing_minor_half_diminished_function() {
        let mut player = Realizer::new(Key::C_MAJOR);
        let held = [48, 52, 55];
        player.plan(&held, Preferences::default());
        let prepared = player.plan(
            &held,
            Preferences {
                colour: Colour::Extended,
                direction: Direction::Prepare,
                ..Preferences::default()
            },
        );
        assert_eq!(prepared.chord, Some(chord(2, Shape::Minor9)));
        assert!(sounding(prepared, &held).contains(4));
        assert_eq!(player.limited & 1, 0);
        player.clear();
        let minor_held = [45, 60, 64];
        player.plan(&minor_held, Preferences::default());
        let prepared = player.plan(
            &minor_held,
            Preferences {
                colour: Colour::Extended,
                direction: Direction::Prepare,
                ..Preferences::default()
            },
        );
        assert_eq!(prepared.chord, Some(chord(11, Shape::HalfDiminished7)));
        assert_eq!(
            player.limited & 1,
            1,
            "catalogue has no half-diminished ninth recipe"
        );
    }
}
