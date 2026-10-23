//! Fixed-anchor chord colours and directed resolutions for the Asterisk instrument.
//!
//! The catalog has twenty-five shapes at twelve roots. Each shape declares its
//! structural tones; a prepared melodic decoration may briefly imply a triad
//! tone; an established dominant root may remain implied beneath its continuing
//! guide pair. Only a perfect fifth is otherwise optional. The two played notes
//! remain exact, including when they are a chord's third, seventh or extension.
//! Ordinary playing follows tonal functions, prepared steps and voice-specific
//! tendency tones; explicit colour requests retain the wider palette. This is
//! a bounded performance vocabulary, not a general harmony grammar.

use super::voicing::{
    classify_six_four_motion, forbidden_parallels, parallel_pair, SixFourContext,
};
use super::{
    ChordKind, Degree, HarmonicFunction, HarmonicIntent, Key, PitchClassSet, ToneRole, Voicing,
};

/// The real keyboard voice whose chord role is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorVoice {
    /// The literal left-hand note.
    Bass,
    /// The literal right-hand note.
    Melody,
}

/// One immediate harmony request, independent of the two played pitches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// Follow the played phrase, balancing function, preparation and small motion.
    Follow,
    /// Prepare a predominant approach using the literal played notes.
    Prepare,
    /// Substitute a chord while retaining its functional direction.
    Substitute,
    /// Reinterpret the bass or melody without moving its played pitch.
    Reinterpret(AnchorVoice),
    /// Reduce colour/tension while retaining compatible played notes.
    Relax,
    /// Seek directed pull; without guidance, request the next colour tier.
    Tension,
    /// Realize the saved compatible destination, or settle when none is pending.
    Resolve,
    /// Try another sounding voicing, colour or common functional substitute.
    Reharm,
    /// Reinterpret one exact keyboard note as a structural chord tone.
    Role {
        /// Which literal keyboard note receives the role.
        voice: AnchorVoice,
        /// Root, third, fifth or seventh; extensions have no structural role.
        role: ToneRole,
    },
}

/// A physical preference which lasts only while its thumb is held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Steering {
    /// Let the played phrase continue without an additional request.
    #[default]
    Neutral,
    /// Favor a calmer continuation without requiring a cadence.
    Ease,
    /// Favor a predominant approach while physically held.
    Prepare,
    /// Favor functional alternatives while physically held.
    Substitute,
    /// Favor directed harmonic pull.
    Lean,
    /// Seek the destination established by the sounded phrase.
    Arrive,
}

impl Steering {
    /// Functional direction; easing remains a continuation, not a forced tonic.
    #[must_use]
    pub const fn intent(self) -> HarmonicIntent {
        match self {
            Self::Neutral | Self::Ease | Self::Prepare | Self::Substitute => {
                HarmonicIntent::Continue
            }
            Self::Lean => HarmonicIntent::Lean,
            Self::Arrive => HarmonicIntent::Arrive,
        }
    }
}

/// The phrase-level meaning of an ordinary played-note event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HarmonicEvent {
    /// Keep the current harmonic thought when a legal continuation exists.
    Continue,
    /// Reconsider the harmony from the played notes and saved phrase context.
    #[default]
    Reconsider,
    /// Give an emphasized melody attack a structural harmonic meaning if possible.
    Structural,
    /// Prefer the saved destination when the played notes permit its arrival.
    Arrive,
}

/// Momentary musical preferences supplied by the physical controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Guidance {
    /// Phrase meaning; explicit commands and held direction controls take priority.
    pub event: HarmonicEvent,
    /// Current thumb direction.
    pub steering: Steering,
    /// Held bass or melody reinterpretation bar.
    pub interpretation: Option<AnchorVoice>,
    /// Permit a chromatic functional detour or substitution.
    pub borrowing: bool,
    /// Permit suspended, extended and altered chord colours.
    pub extensions: bool,
    /// Extra held-key pitch classes guide choices without supplying sounding voices.
    pub extra_tones: PitchClassSet,
}

/// A small explicit chord-shape vocabulary; sevenths are never implicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Major triad.
    Major,
    /// Minor triad.
    Minor,
    /// Diminished triad, including its essential diminished fifth.
    Diminished,
    /// Augmented triad, including its essential augmented fifth.
    Augmented,
    /// Major triad plus major sixth.
    Major6,
    /// Minor triad plus major sixth.
    Minor6,
    /// Major triad plus ninth.
    Add9,
    /// Minor triad plus ninth.
    MinorAdd9,
    /// Major seventh chord.
    Major7,
    /// Minor seventh chord.
    Minor7,
    /// Dominant seventh chord.
    Dominant7,
    /// Half-diminished seventh chord.
    HalfDiminished7,
    /// Fully diminished seventh chord.
    Diminished7,
    /// Minor triad plus major seventh.
    MinorMajor7,
    /// Major seventh plus ninth; its perfect fifth may be omitted.
    Major9,
    /// Minor seventh plus ninth; its perfect fifth may be omitted.
    Minor9,
    /// Dominant seventh plus ninth; its perfect fifth may be omitted.
    Dominant9,
    /// Dominant seventh plus flat thirteenth; its perfect fifth may be omitted.
    DominantFlat13,
    /// Root, second and perfect fifth; the second replaces the third.
    Sus2,
    /// Root, fourth and perfect fifth; the fourth replaces the third.
    Sus4,
    /// Dominant seventh with its third replaced by the fourth.
    DominantSus4,
    /// Dominant seventh plus the natural thirteenth.
    Dominant13,
    /// Major seventh plus the natural thirteenth.
    Major13,
    /// Dominant seventh plus the flat ninth.
    DominantFlat9,
    /// Dominant seventh plus the sharp ninth.
    DominantSharp9,
}

impl Shape {
    const ALL: [Self; 25] = [
        Self::Major,
        Self::Minor,
        Self::Diminished,
        Self::Augmented,
        Self::Major6,
        Self::Minor6,
        Self::Add9,
        Self::MinorAdd9,
        Self::Major7,
        Self::Minor7,
        Self::Dominant7,
        Self::HalfDiminished7,
        Self::Diminished7,
        Self::MinorMajor7,
        Self::Major9,
        Self::Minor9,
        Self::Dominant9,
        Self::DominantFlat13,
        Self::Sus2,
        Self::Sus4,
        Self::DominantSus4,
        Self::Dominant13,
        Self::Major13,
        Self::DominantFlat9,
        Self::DominantSharp9,
    ];

    const fn third(self) -> u8 {
        match self {
            Self::Sus2 => 2,
            Self::Sus4 | Self::DominantSus4 => 5,
            Self::Minor
            | Self::Diminished
            | Self::Minor6
            | Self::MinorAdd9
            | Self::Minor7
            | Self::HalfDiminished7
            | Self::Diminished7
            | Self::MinorMajor7
            | Self::Minor9 => 3,
            _ => 4,
        }
    }

    const fn fifth(self) -> u8 {
        match self {
            Self::Diminished | Self::HalfDiminished7 | Self::Diminished7 => 6,
            Self::Augmented => 8,
            _ => 7,
        }
    }

    const fn seventh(self) -> Option<u8> {
        match self {
            Self::Major7 | Self::Major9 | Self::MinorMajor7 | Self::Major13 => Some(11),
            Self::Minor7
            | Self::Dominant7
            | Self::HalfDiminished7
            | Self::Minor9
            | Self::Dominant9
            | Self::DominantFlat13
            | Self::DominantSus4
            | Self::Dominant13
            | Self::DominantFlat9
            | Self::DominantSharp9 => Some(10),
            Self::Diminished7 => Some(9),
            _ => None,
        }
    }

    const fn colour(self) -> Option<u8> {
        match self {
            Self::Major6 | Self::Minor6 | Self::Dominant13 | Self::Major13 => Some(9),
            Self::Add9 | Self::MinorAdd9 | Self::Major9 | Self::Minor9 | Self::Dominant9 => Some(2),
            Self::DominantFlat13 => Some(8),
            Self::DominantFlat9 => Some(1),
            Self::DominantSharp9 => Some(3),
            _ => None,
        }
    }

    const fn suspended(self) -> bool {
        matches!(self, Self::Sus2 | Self::Sus4 | Self::DominantSus4)
    }

    const fn licensed_extension(self) -> bool {
        matches!(
            self,
            Self::Sus2
                | Self::Sus4
                | Self::DominantSus4
                | Self::Dominant13
                | Self::Major13
                | Self::DominantFlat9
                | Self::DominantSharp9
        )
    }

    const fn tension(self) -> u8 {
        match self {
            Self::Major | Self::Minor => 0,
            Self::Sus2 | Self::Sus4 => 1,
            Self::Major6
            | Self::Minor6
            | Self::Add9
            | Self::MinorAdd9
            | Self::Major7
            | Self::Minor7 => 1,
            Self::DominantFlat13 => 3,
            _ => 2,
        }
    }
}

/// Chord identity, independent of inversion, spacing or the current voicing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    /// Root pitch class in 0..12.
    pub root: u8,
    /// Explicit intervals above that root.
    pub shape: Shape,
}

impl Chord {
    /// Constructs a chord with a validated pitch-class root.
    #[must_use]
    pub const fn new(root: u8, shape: Shape) -> Option<Self> {
        if root < 12 {
            Some(Self { root, shape })
        } else {
            None
        }
    }

    /// Every permitted chord tone, including its declared colour.
    #[must_use]
    pub fn tones(self) -> PitchClassSet {
        let mut tones = self.essential();
        tones.insert(self.fifth());
        tones
    }

    /// Structural tones; selection can separately prove an implied dominant root.
    #[must_use]
    pub fn essential(self) -> PitchClassSet {
        let mut tones = PitchClassSet::default();
        for tone in [
            Some(self.root),
            Some(self.third()),
            self.seventh(),
            self.colour(),
        ]
        .into_iter()
        .flatten()
        {
            tones.insert(tone);
        }
        if self.shape.fifth() != 7 || matches!(self.shape, Shape::Sus2 | Shape::Sus4) {
            tones.insert(self.fifth());
        }
        tones
    }

    /// Third-slot pitch class; a suspended shape replaces it with its second/fourth.
    #[must_use]
    pub const fn third(self) -> u8 {
        (self.root % 12 + self.shape.third()) % 12
    }

    /// Fifth pitch class, including altered fifths.
    #[must_use]
    pub const fn fifth(self) -> u8 {
        (self.root % 12 + self.shape.fifth()) % 12
    }

    /// Seventh pitch class, when explicitly present.
    #[must_use]
    pub const fn seventh(self) -> Option<u8> {
        match self.shape.seventh() {
            Some(tone) => Some((self.root % 12 + tone) % 12),
            None => None,
        }
    }

    const fn colour(self) -> Option<u8> {
        match self.shape.colour() {
            Some(tone) => Some((self.root % 12 + tone) % 12),
            None => None,
        }
    }

    /// Structural role of a MIDI note; ninths/sixths/thirteenths return `None`.
    #[must_use]
    pub fn role(self, note: u8) -> Option<ToneRole> {
        let pitch = note % 12;
        if pitch == self.root % 12 {
            Some(ToneRole::Root)
        } else if pitch == self.third() && !self.shape.suspended() {
            Some(ToneRole::Third)
        } else if pitch == self.fifth() {
            Some(ToneRole::Fifth)
        } else if Some(pitch) == self.seventh() {
            Some(ToneRole::Seventh)
        } else {
            None
        }
    }

    fn leading(self) -> Option<u8> {
        match self.shape {
            Shape::Dominant7
            | Shape::Dominant9
            | Shape::DominantFlat13
            | Shape::Dominant13
            | Shape::DominantFlat9
            | Shape::DominantSharp9
            | Shape::Augmented => Some(self.third()),
            Shape::Diminished | Shape::HalfDiminished7 | Shape::Diminished7 => Some(self.root % 12),
            Shape::MinorMajor7 => self.seventh(),
            _ => None,
        }
    }
}

/// A voice-specific tendency that can survive an intervening literal note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceTendency {
    /// Pitch carrying the unresolved tendency.
    pub from: u8,
    /// Its saved resolution pitch in this same inner voice.
    pub to: u8,
}

/// A prepared passing/neighbour note or a suspension over a newly arrived bass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MelodyOrnament {
    /// Structural chord tone: the departure for a passing/neighbour note, or
    /// the arrival for a held suspension.
    pub origin: u8,
    /// Current literal non-chord melody note.
    pub pitch: u8,
    /// Diatonic preparation direction, or suspension resolution direction.
    pub direction: i8,
    /// The melody was held through a harmony change and awaits a step into
    /// that chord, rather than moving as a passing note.
    pub suspension: bool,
}

/// One prepared inner excursion, advanced by its originating keyboard bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InnerOrnament {
    /// Actual intervening pitch, which may be caught independently.
    pub from: u8,
    /// Previously verified structural endpoint in this same voice.
    pub to: u8,
    /// Only a new pitch in this bank advances the excursion automatically.
    pub bank: AnchorVoice,
}

/// One resolved chord and its currently caught sounding pitches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Resolved target pitches, including targets beneath catches.
    pub voicing: Voicing,
    /// Actual pitches with every supplied catch overlaid.
    pub realized: Voicing,
    /// Identity of the resolved target chord.
    pub chord: Chord,
    /// An established root is implied while its actual guide pair and destination continue.
    pub root_implied: bool,
    /// Local colour tier, 0 through 3; this is not a universal tension measure.
    pub tension: u8,
    /// Compatible destination of a directed dominant or leading-tone colour.
    pub resolution: Option<Chord>,
    /// Harmonic destination, even while the current literal anchors prevent it.
    pub tendency: Option<Chord>,
    /// Phrase direction retained while the two keyboards continue playing.
    pub guidance: HarmonicIntent,
    /// Voice-specific obligations retained while a catch delays their arrival.
    pub tendencies: [Option<VoiceTendency>; 2],
    /// Prepared literal melody motion temporarily outside the structural chord.
    pub ornament: Option<MelodyOrnament>,
    /// Prepared non-chord bass pedal, kept at its exact played pitch.
    pub pedal: Option<u8>,
    /// Continuously held tonic/dominant bass prepared in a structural harmony.
    pub pedal_prepared: Option<u8>,
    /// Newest meaningful bass pitches first; repeated snapshots do not append.
    pub bass_history: [Option<u8>; 3],
    /// Newest actually sounded pitches first, independently for tenor and alto.
    pub inner_history: [[Option<u8>; 3]; 2],
    /// At most one generated inner non-chord excursion is active at a time.
    pub inner_ornaments: [Option<InnerOrnament>; 2],
    /// A caught voice differs from its saved target, even if it is a chord tone.
    pub suspended: [bool; 2],
    /// Only the two fixed outer notes force a parallel or leading-tone conflict.
    pub anchor_conflict: bool,
    /// Lower is preferred within the event's harmonic priority.
    pub score: u32,
    /// Eight preceding audible realizations, softly avoided by Reharm.
    pub recent_realizations: [Option<Voicing>; 8],
    /// Reharm counter: its remainder selects a family, while its longer history
    /// varies choices among similarly ranked legal voicings.
    pub variation: u32,
}

/// Checks target or partial-release geometry without changing either anchor.
#[must_use]
pub fn valid_realization(key: Key, chord: Chord, voicing: Voicing) -> bool {
    valid_caught_realization(key, chord, voicing, [None; 2])
}

/// Actual caught notes are player-forced; generated guide tones remain unique.
pub(crate) fn valid_caught_realization(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    held: [Option<u8>; 2],
) -> bool {
    let [bass, tenor, alto, melody] = voicing.notes;
    if !(24..=48).contains(&bass)
        || !(48..=67).contains(&tenor)
        || !(53..=76).contains(&alto)
        || !(60..=88).contains(&melody)
        || !(bass < tenor && tenor < alto && alto < melody)
        || alto - tenor > 12
        || melody - alto > 12
    {
        return false;
    }
    for leading in [
        key.leading_tone(),
        chord_leading(key, chord),
        chord.seventh(),
    ]
    .into_iter()
    .flatten()
    {
        if voicing
            .notes
            .iter()
            .filter(|note| **note % 12 == leading)
            .count()
            > 1
            && voicing.notes[1..3]
                .iter()
                .enumerate()
                .any(|(index, note)| *note % 12 == leading && held[index] != Some(*note))
        {
            return false;
        }
    }
    true
}

/// Selects from at most 300 chord recipes and 20×24 inner-note pairs per recipe.
///
/// Both anchors are literal. Holds are mandatory and must match their previous
/// actual voices. A saved resolution is an exact chord request, not permission
/// to move an anchor or substitute a different destination on failure.
pub fn select(
    key: Key,
    bass: u8,
    melody: u8,
    previous: Option<Plan>,
    held: [Option<u8>; 2],
    gesture: Gesture,
) -> Option<Plan> {
    let required = if gesture == Gesture::Resolve {
        previous.and_then(|old| old.resolution)
    } else {
        None
    };
    search(
        key, bass, melody, previous, held, gesture, required, false, None,
    )
}

/// Selects a harmony using the current physical thumb and bar preferences.
pub fn select_steered(
    key: Key,
    bass: u8,
    melody: u8,
    previous: Option<Plan>,
    held: [Option<u8>; 2],
    gesture: Gesture,
    guidance: Guidance,
) -> Option<Plan> {
    let required = (gesture == Gesture::Resolve)
        .then(|| previous.and_then(|old| old.resolution))
        .flatten();
    search(
        key,
        bass,
        melody,
        previous,
        held,
        gesture,
        required,
        false,
        Some(guidance),
    )
}

/// Changes the felt preference without replanning or cancelling heard obligations.
#[must_use]
pub const fn with_steering(mut plan: Plan, steering: Steering) -> Plan {
    plan.guidance = steering.intent();
    plan
}

/// Revoices the same chord around remaining catches after an unsafe partial release.
#[cfg(test)]
fn release(key: Key, previous: Plan, held: [Option<u8>; 2]) -> Option<Plan> {
    release_voices(key, previous, held, [true; 2])
}

/// Resolves only the released catches; another voice's excursion remains pending.
pub(crate) fn release_voices(
    key: Key,
    mut previous: Plan,
    held: [Option<u8>; 2],
    released: [bool; 2],
) -> Option<Plan> {
    for index in 0..2 {
        if released[index] && held[index].is_none() {
            previous.inner_ornaments[index] = None;
        }
    }
    let goal = previous.tendency.filter(|target| {
        target.tones().contains(previous.voicing.notes[0])
            && target.tones().contains(previous.voicing.notes[3])
            && released
                .iter()
                .enumerate()
                .any(|(index, released)| *released && previous.tendencies[index].is_some())
    });
    let select = |required| {
        search(
            key,
            previous.voicing.notes[0],
            previous.voicing.notes[3],
            Some(previous),
            held,
            Gesture::Follow,
            Some(required),
            true,
            None,
        )
    };
    let mut next = goal.and_then(select).or_else(|| select(previous.chord))?;
    next.recent_realizations = previous.recent_realizations;
    Some(next)
}

/// Clears a cached route that became unavailable after a manual catch release.
pub(crate) fn refresh_resolution(key: Key, mut plan: Plan, held: [Option<u8>; 2]) -> Plan {
    for index in 0..2 {
        if plan.inner_ornaments[index]
            .is_some_and(|pending| plan.realized.notes[index + 1] != pending.from)
        {
            plan.inner_ornaments[index] = None;
        }
        plan.inner_history[index] =
            push_pitch(plan.inner_history[index], plan.realized.notes[index + 1]);
    }
    plan.tendencies = voice_tendencies(
        key,
        plan.chord,
        plan.voicing,
        plan.realized,
        Some(plan),
        Gesture::Follow,
        held,
        plan.guidance,
    );
    if plan.tendency == Some(plan.chord) {
        plan.tendency = abstract_destination(key, plan.chord).or(plan.tendency);
    }
    if plan.tendencies.iter().any(Option::is_some)
        && plan.realized != plan.voicing
        && tonal_function(key, plan.chord) == Some(HarmonicFunction::Tonic)
    {
        plan.tendency = Some(plan.chord);
        plan.guidance = HarmonicIntent::Arrive;
    }
    if plan.realized == plan.voicing
        && plan.ornament.is_none()
        && plan.tendencies.iter().all(Option::is_none)
        && (plan.tendency == Some(plan.chord)
            || (plan.guidance == HarmonicIntent::Arrive
                && tonal_function(key, plan.chord) == Some(HarmonicFunction::Tonic)))
    {
        plan.tendency = None;
        plan.guidance = HarmonicIntent::Continue;
    }
    plan.resolution = plan
        .tendency
        .filter(|target| {
            target.tones().contains(plan.voicing.notes[0])
                && target.tones().contains(plan.voicing.notes[3])
        })
        .or_else(|| {
            destination(
                key,
                plan.chord,
                plan.voicing.notes[0],
                plan.voicing.notes[3],
            )
        });
    if let Some(target) = plan.resolution {
        if search(
            key,
            plan.voicing.notes[0],
            plan.voicing.notes[3],
            Some(plan),
            held,
            Gesture::Resolve,
            Some(target),
            false,
            None,
        )
        .is_none()
        {
            plan.resolution = None;
        }
    }
    plan
}

#[expect(
    clippy::too_many_arguments,
    reason = "one bounded search shared by selection and partial release"
)]
fn search(
    key: Key,
    bass: u8,
    melody: u8,
    previous: Option<Plan>,
    held: [Option<u8>; 2],
    gesture: Gesture,
    required: Option<Chord>,
    releasing: bool,
    preferences: Option<Guidance>,
) -> Option<Plan> {
    if bass > 127 || melody > 127 || held.iter().any(|note| note.is_some_and(|n| n > 127)) {
        return None;
    }
    for (index, pitch) in held.into_iter().enumerate() {
        if pitch
            .is_some_and(|pitch| previous.is_none_or(|old| old.realized.notes[index + 1] != pitch))
        {
            return None;
        }
    }
    let steering = preferences.map(|value| value.steering);
    let interpretation = preferences.and_then(|value| value.interpretation);
    let borrowing = preferences.is_some_and(|value| value.borrowing);
    let extensions = preferences.is_some_and(|value| value.extensions);
    let extra_tones = preferences.map_or(PitchClassSet::default(), |value| value.extra_tones);
    let colour_requested = matches!(gesture, Gesture::Substitute | Gesture::Reharm)
        || (gesture == Gesture::Follow && steering == Some(Steering::Substitute));
    let event = preferences
        .filter(|value| {
            gesture == Gesture::Follow
                && value.steering == Steering::Neutral
                && value.interpretation.is_none()
        })
        .map_or(HarmonicEvent::Reconsider, |value| value.event);
    let retaining_dominant =
        gesture == Gesture::Follow && steering == Some(Steering::Lean) && interpretation.is_none();
    let mut best: Option<Plan> = None;
    let mut best_event_rank = u8::MAX;
    let guidance = steering
        .map(Steering::intent)
        .unwrap_or_else(|| match gesture {
            Gesture::Tension => HarmonicIntent::Lean,
            Gesture::Relax | Gesture::Resolve => HarmonicIntent::Arrive,
            _ => previous.map_or(HarmonicIntent::Continue, |old| old.guidance),
        });
    for root in 0..12 {
        for shape in Shape::ALL {
            let chord = Chord { root, shape };
            let continuing_root = established_root(key, chord, bass, previous);
            if shape.licensed_extension()
                && !extensions
                && !(previous.is_some_and(|old| old.chord == chord)
                    && (releasing || required == Some(chord)))
                && !(gesture == Gesture::Follow
                    && continuing_root
                    && chord.colour() == Some(melody % 12))
            {
                continue;
            }
            let tones = chord.tones();
            let ornament = melody_ornament(key, chord, bass, melody, previous);
            let pedal = pedal_context(key, chord, bass, previous, gesture, borrowing);
            if required.is_some_and(|required| required != chord)
                || (!tones.contains(bass) && pedal.is_none())
                || (!tones.contains(melody) && ornament.is_none())
            {
                continue;
            }
            if let Gesture::Role { voice, role } = gesture {
                let pitch = match voice {
                    AnchorVoice::Bass => bass,
                    AnchorVoice::Melody => melody,
                };
                if chord.role(pitch) != Some(role) {
                    continue;
                }
            }
            if gesture == Gesture::Substitute
                && !borrowing
                && previous.is_some_and(|old| !compatible_substitute(key, old, chord))
            {
                continue;
            }
            let essential = chord.essential();
            let event_rank = if retaining_dominant {
                1
            } else {
                event_rank(key, event, chord, previous, ornament)
            };
            if event_rank > best_event_rank
                && !((event == HarmonicEvent::Structural || retaining_dominant)
                    && continuing_root
                    && chord.colour() == Some(melody % 12))
            {
                continue;
            }
            let resolution = destination(key, chord, bass, melody);
            let recipe_cost =
                if let Some(preferences) = preferences {
                    steered_recipe_cost(key, chord, resolution, previous, gesture, preferences)
                } else {
                    recipe_cost(key, chord, resolution, previous, gesture)
                } + if gesture == Gesture::Follow {
                    follow_cost(
                        key,
                        chord,
                        bass,
                        melody,
                        previous,
                        guidance,
                        ornament,
                        steering == Some(Steering::Prepare),
                    ) + if steering.is_some() {
                        bass_figure_cost(key, chord, bass, melody, previous)
                    } else {
                        0
                    }
                } else {
                    0
                } + if colour_requested && extensions && !shape.licensed_extension() {
                    // A held colour permission must be audible when a rich, legal
                    // shell fits; generic tier costs otherwise bury that palette.
                    512
                } else {
                    0
                } + if colour_requested && borrowing {
                    borrowed_cost(key, chord, previous)
                } else {
                    0
                } + if ornament.is_some() && previous.is_some_and(|old| old.chord != chord) {
                    // Prefer a structural melody meaning before creating a new dissonance.
                    128
                } else {
                    0
                } + 64 * (extra_tones.bits() & !tones.bits()).count_ones()
                    + interpretation_cost(
                        chord,
                        bass,
                        melody,
                        previous,
                        gesture,
                        interpretation.or(match gesture {
                            Gesture::Reinterpret(voice) => Some(voice),
                            _ => None,
                        }),
                        ornament,
                        pedal,
                    );
            for tenor in 48..=67 {
                if !tones.contains(tenor) {
                    continue;
                }
                for alto in 53..=76 {
                    if !tones.contains(alto) {
                        continue;
                    }
                    let voicing = Voicing {
                        notes: [bass, tenor, alto, melody],
                    };
                    let mut present = PitchClassSet::default();
                    for note in voicing.notes {
                        present.insert(note);
                    }
                    let mut coverage = present;
                    let root_implied = !present.contains(chord.root)
                        && continuing_root
                        && previous
                            .is_some_and(|old| old.realized.notes[1..3] == voicing.notes[1..3]);
                    let event_rank = if (event == HarmonicEvent::Structural || retaining_dominant)
                        && root_implied
                        && chord.colour() == Some(melody % 12)
                    {
                        0
                    } else {
                        event_rank
                    };
                    if event_rank > best_event_rank {
                        continue;
                    }
                    if root_implied {
                        coverage.insert(chord.root);
                    }
                    if gesture == Gesture::Follow {
                        if let Some(origin) =
                            implied_melody_tone(key, chord, voicing, previous, ornament)
                        {
                            coverage.insert(origin);
                        }
                    }
                    if essential.bits() & !coverage.bits() != 0
                        || !valid_realization(key, chord, voicing)
                    {
                        continue;
                    }
                    let mut realized = voicing;
                    let mut suspended = [false; 2];
                    for (index, pitch) in held.into_iter().enumerate() {
                        if let Some(pitch) = pitch {
                            realized.notes[index + 1] = pitch;
                            suspended[index] = pitch != voicing.notes[index + 1];
                        }
                    }
                    let inner_ornaments = pending_ornaments(voicing, previous, held, gesture);
                    for (index, pending) in inner_ornaments.into_iter().enumerate() {
                        if let Some(pending) = pending {
                            realized.notes[index + 1] = held[index].unwrap_or(pending.from);
                        }
                    }
                    if root_implied && realized.notes[1..3] != voicing.notes[1..3] {
                        continue;
                    }
                    if !valid_caught_realization(key, chord, realized, held)
                        || forbidden_parallels(realized, voicing, false)
                    {
                        continue;
                    }
                    if let Some(old) = previous {
                        if forbidden_parallels(old.realized, realized, true)
                            || forbidden_parallels(old.realized, voicing, true)
                            || (gesture == Gesture::Resolve
                                && required.is_some()
                                && !resolution_targets(key, old, chord, voicing))
                        {
                            continue;
                        }
                        if releasing
                            && old.tendencies.iter().enumerate().any(|(index, debt)| {
                                debt.is_some_and(|debt| {
                                    held[index].is_none()
                                        && voicing.notes[index + 1] != debt.to
                                        && voicing.notes[index + 1] != old.realized.notes[index + 1]
                                })
                            })
                        {
                            continue;
                        }
                    }
                    let anchor_conflict = previous
                        .is_some_and(|old| parallel_pair(old.realized, voicing, 0, 3))
                        || [key.leading_tone(), chord_leading(key, chord)]
                            .into_iter()
                            .flatten()
                            .any(|pitch| bass % 12 == pitch && melody % 12 == pitch);
                    let mut score = recipe_cost;
                    score += tendency_cost(
                        key,
                        chord,
                        voicing,
                        previous,
                        if steering.is_some()
                            && matches!(
                                gesture,
                                Gesture::Relax
                                    | Gesture::Prepare
                                    | Gesture::Substitute
                                    | Gesture::Reinterpret(_)
                            )
                        {
                            Gesture::Resolve
                        } else {
                            gesture
                        },
                    );
                    if steering.is_some() && gesture == Gesture::Follow {
                        score += contour_cost(key, chord, voicing, previous, held);
                    }
                    if cadential_destination(key, chord, voicing, previous, guidance).is_some()
                        && voicing.notes[1..].iter().all(|note| note % 12 != bass % 12)
                    {
                        score += 64;
                    }
                    if !present.contains(chord.fifth()) {
                        score += 24;
                    }
                    if let Some(old) = previous {
                        for voice in 1..3 {
                            let from = if releasing && held[voice - 1].is_none() {
                                old.voicing.notes[voice]
                            } else {
                                old.realized.notes[voice]
                            };
                            let distance = u32::from(from.abs_diff(voicing.notes[voice]));
                            score += distance * distance;
                        }
                        if old.realized == realized
                            && !matches!(
                                gesture,
                                Gesture::Follow | Gesture::Relax | Gesture::Resolve
                            )
                        {
                            score += 8192;
                        }
                        if old.chord != chord {
                            score += 12;
                        }
                        if varied_gesture(gesture) {
                            if old.recent_realizations.contains(&Some(realized)) {
                                score += 2048;
                            }
                            score += variation_bias(old.variation, realized);
                        }
                    } else {
                        score += u32::from(tenor.abs_diff(55)) + u32::from(alto.abs_diff(64));
                    }
                    if event_rank == best_event_rank && best.is_some_and(|old| score >= old.score) {
                        continue;
                    }
                    if gesture == Gesture::Follow {
                        score += cadence_space_cost(
                            key,
                            chord,
                            voicing,
                            previous.filter(|_| steering.is_some()),
                            guidance,
                        );
                    }
                    if event_rank < best_event_rank || best.is_none_or(|old| score < old.score) {
                        best_event_rank = event_rank;
                        best = Some(Plan {
                            voicing,
                            realized,
                            chord,
                            root_implied,
                            tension: shape.tension(),
                            resolution,
                            tendency: phrase_destination(
                                key, chord, realized, previous, guidance, gesture, held,
                            ),
                            guidance,
                            tendencies: voice_tendencies(
                                key, chord, voicing, realized, previous, gesture, held, guidance,
                            ),
                            ornament,
                            pedal,
                            pedal_prepared: prepared_pedal(key, chord, bass, previous),
                            bass_history: bass_history(previous, bass),
                            inner_history: inner_history(previous, realized),
                            inner_ornaments,
                            suspended,
                            anchor_conflict,
                            score,
                            recent_realizations: previous.map_or([None; 8], |old| {
                                let mut recent = old.recent_realizations;
                                if old.realized != realized {
                                    recent.rotate_right(1);
                                    recent[0] = Some(old.realized);
                                }
                                recent
                            }),
                            variation: previous.map_or(0, |old| {
                                if varied_gesture(gesture) {
                                    old.variation.wrapping_add(1)
                                } else {
                                    old.variation
                                }
                            }),
                        });
                    }
                }
            }
        }
    }
    // Validate one exact destination after choosing the colour. Destinations
    // are plain triads, so validation cannot acquire another cadence.
    if gesture == Gesture::Resolve {
        best
    } else {
        let mut best = best?;
        if steering.is_some() && !releasing && !best.root_implied {
            best = decorate_inner(key, best, previous, held, gesture);
        }
        Some(refresh_resolution(key, best, held))
    }
}

fn event_rank(
    key: Key,
    event: HarmonicEvent,
    chord: Chord,
    previous: Option<Plan>,
    ornament: Option<MelodyOrnament>,
) -> u8 {
    let Some(old) = previous else {
        return 0;
    };
    match event {
        HarmonicEvent::Reconsider => 0,
        HarmonicEvent::Structural => 1 + u8::from(ornament.is_some()),
        HarmonicEvent::Arrive => u8::from(old.tendency.is_some_and(|goal| chord != goal)),
        HarmonicEvent::Continue => u8::from(
            chord.root != old.chord.root
                || chord.third() != old.chord.third()
                || chord.fifth() != old.chord.fifth()
                || chord.seventh() != old.chord.seventh()
                || abstract_destination(key, chord) != abstract_destination(key, old.chord),
        ),
    }
}

fn established_root(key: Key, chord: Chord, bass: u8, previous: Option<Plan>) -> bool {
    let Some(old) = previous else {
        return false;
    };
    let Some(goal) = abstract_destination(key, chord) else {
        return false;
    };
    chord.shape.third() == 4
        && chord.shape.seventh() == Some(10)
        && old.chord.root == chord.root
        && old.chord.third() == chord.third()
        && old.chord.seventh() == chord.seventh()
        && old.tendency == Some(goal)
        && abstract_destination(key, old.chord) == Some(goal)
        && old.realized.notes[0] == bass
        && old.pedal_prepared == Some(bass)
        && (old.root_implied
            || old
                .realized
                .notes
                .iter()
                .any(|note| *note % 12 == chord.root))
        && [chord.third(), chord.seventh().unwrap_or(chord.root)]
            .into_iter()
            .all(|guide| {
                old.realized.notes[1..3]
                    .iter()
                    .any(|note| *note % 12 == guide)
            })
}

fn prepared_pedal(key: Key, chord: Chord, bass: u8, previous: Option<Plan>) -> Option<u8> {
    let stable = [key.tonic(), key.pitch(Degree::V)].contains(&(bass % 12));
    (stable
        && (chord.tones().contains(bass)
            || previous.is_some_and(|old| old.pedal_prepared == Some(bass))))
    .then_some(bass)
}

fn pedal_context(
    key: Key,
    chord: Chord,
    bass: u8,
    previous: Option<Plan>,
    gesture: Gesture,
    borrowing: bool,
) -> Option<u8> {
    if chord.tones().contains(bass)
        || matches!(
            gesture,
            Gesture::Role {
                voice: AnchorVoice::Bass,
                ..
            }
        )
        || !(matches!(chord.shape, Shape::Major | Shape::Minor | Shape::Dominant7)
            || (borrowing && dominant_destination(key, chord).is_some())
            || established_root(key, chord, bass, previous)
            || previous.is_some_and(|old| old.chord == chord && old.pedal == Some(bass)))
    {
        return None;
    }
    let old = previous?;
    if old.voicing.notes[0] != bass || old.pedal_prepared != Some(bass) {
        return None;
    }
    let diatonic = key.degree(chord.root).is_some_and(|degree| {
        let triad = key.chord(degree, ChordKind::Triad);
        triad.third == chord.third() && triad.fifth == chord.fifth()
    });
    (diatonic
        || old.chord == chord && old.pedal == Some(bass)
        || (borrowing && dominant_destination(key, chord).is_some()))
    .then_some(bass)
}

fn push_pitch(mut history: [Option<u8>; 3], note: u8) -> [Option<u8>; 3] {
    if history[0] != Some(note) {
        history.rotate_right(1);
        history[0] = Some(note);
    }
    history
}

fn bass_history(previous: Option<Plan>, bass: u8) -> [Option<u8>; 3] {
    push_pitch(previous.map_or([None; 3], |old| old.bass_history), bass)
}

fn inner_history(previous: Option<Plan>, realized: Voicing) -> [[Option<u8>; 3]; 2] {
    core::array::from_fn(|index| {
        push_pitch(
            previous.map_or([None; 3], |old| old.inner_history[index]),
            realized.notes[index + 1],
        )
    })
}

fn bass_figure_cost(key: Key, chord: Chord, bass: u8, melody: u8, previous: Option<Plan>) -> u32 {
    let Some(old) = previous else {
        return 0;
    };
    let from = old.voicing.notes[0];
    if from == bass {
        return 0;
    }
    let direction = [-1, 1]
        .into_iter()
        .find(|step| key.diatonic_step(from, *step) == Some(bass));
    let expected = match (key.degree(from), key.degree(bass), direction) {
        (Some(Degree::I), Some(Degree::Ii), Some(1)) if old.voicing.notes[3] == melody => {
            Some(key.pitch(Degree::V))
        }
        (Some(Degree::Iii), Some(Degree::Ii), Some(-1))
        | (Some(Degree::V), Some(Degree::Iv), Some(-1))
        | (Some(Degree::Vi), Some(Degree::Vii), Some(1)) => Some(key.pitch(Degree::V)),
        (Some(Degree::Ii), Some(Degree::Iii), Some(1))
        | (Some(Degree::Ii), Some(Degree::I), Some(-1))
        | (Some(Degree::Vii), Some(Degree::I), Some(1)) => Some(key.tonic()),
        _ => None,
    };
    let mut cost: u32 = if expected.is_some_and(|root| root != chord.root) {
        112
    } else {
        0
    };
    if let Some(earlier) = old.bass_history[1] {
        let previous_interval = (from + 12 - earlier % 12) % 12;
        let interval = (bass + 12 - from % 12) % 12;
        if [5, 7].contains(&interval) && interval == previous_interval && chord.root != bass % 12 {
            cost += 96;
        }
        if bass == earlier && chord.role(bass) == Some(ToneRole::Root) {
            cost = cost.saturating_sub(24);
        }
    }
    cost
}

fn pending_ornaments(
    voicing: Voicing,
    previous: Option<Plan>,
    held: [Option<u8>; 2],
    gesture: Gesture,
) -> [Option<InnerOrnament>; 2] {
    core::array::from_fn(|index| {
        let old = previous?;
        let pending = old.inner_ornaments[index]?;
        let bank = match pending.bank {
            AnchorVoice::Bass => 0,
            AnchorVoice::Melody => 3,
        };
        let advance = old.voicing.notes[bank] != voicing.notes[bank];
        if held[index].is_some()
            || (!advance
                && !matches!(
                    gesture,
                    Gesture::Resolve
                        | Gesture::Reharm
                        | Gesture::Role { .. }
                        | Gesture::Prepare
                        | Gesture::Substitute
                        | Gesture::Reinterpret(_)
                ))
        {
            // A provisional bass harmony may block the endpoint without stealing
            // the prepared note; a later originating-bank gesture rechecks it.
            return Some(pending);
        }
        None
    })
}

fn decorative_event(chord: Chord, previous: Option<Plan>, voicing: Voicing) -> Option<Plan> {
    let old = previous?;
    (old.chord == chord
        && matches!(chord.shape, Shape::Major | Shape::Minor)
        && old.voicing.notes[0] == voicing.notes[0]
        && old.voicing.notes[3].abs_diff(voicing.notes[3]) >= 5
        && chord.tones().contains(old.voicing.notes[3])
        && chord.tones().contains(voicing.notes[3])
        && old.ornament.is_none()
        && old.inner_ornaments.iter().all(Option::is_none)
        && old.tendencies.iter().all(Option::is_none))
    .then_some(old)
}

fn passing_pitch(key: Key, chord: Chord, from: u8, to: u8) -> Option<u8> {
    let direction = if to > from { 1 } else { -1 };
    let passing = key.diatonic_step(from, direction)?;
    (from != to
        && key.diatonic_step(passing, direction) == Some(to)
        && !chord.tones().contains(passing))
    .then_some(passing)
}

#[expect(
    clippy::too_many_arguments,
    reason = "verify a single prepared voice against its actual and target harmony"
)]
fn ornament_realization(
    key: Key,
    chord: Chord,
    mut realized: Voicing,
    voice: usize,
    pitch: u8,
    old: Voicing,
    target: Voicing,
    held: [Option<u8>; 2],
) -> Option<Voicing> {
    realized.notes[voice] = pitch;
    let present = realized
        .notes
        .into_iter()
        .fold(0u16, |set, note| set | (1 << (note % 12)));
    (chord.essential().bits() & !present == 0
        && valid_caught_realization(key, chord, realized, held)
        && !forbidden_parallels(old, realized, true)
        && !forbidden_parallels(realized, target, false))
    .then_some(realized)
}

fn contour_cost(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Plan>,
    held: [Option<u8>; 2],
) -> u32 {
    let Some(old) = previous else {
        return 0;
    };
    let mut cost = 0;
    for index in 0..2 {
        if let [Some(from), Some(before), _] = old.inner_history[index] {
            let last = i16::from(from) - i16::from(before);
            let next = i16::from(voicing.notes[index + 1]) - i16::from(from);
            if last.abs() > 4 && last * next > 0 {
                cost += 24;
            }
        }
    }
    if decorative_event(chord, previous, voicing).is_some() {
        let contrary = voicing.notes[3] > old.voicing.notes[3];
        let useful = (0..2).any(|index| {
            let voice = index + 1;
            held[index].is_none()
                && (voicing.notes[voice] < old.realized.notes[voice]) == contrary
                && passing_pitch(key, chord, old.realized.notes[voice], voicing.notes[voice])
                    .and_then(|pitch| {
                        ornament_realization(
                            key,
                            chord,
                            voicing,
                            voice,
                            pitch,
                            old.realized,
                            voicing,
                            held,
                        )
                    })
                    .is_some()
        });
        if !useful {
            cost += 64;
        }
    }
    cost
}

fn decorate_inner(
    key: Key,
    mut plan: Plan,
    previous: Option<Plan>,
    held: [Option<u8>; 2],
    gesture: Gesture,
) -> Plan {
    if gesture != Gesture::Follow || plan.inner_ornaments.iter().any(Option::is_some) {
        return plan;
    }
    let Some(old) = decorative_event(plan.chord, previous, plan.voicing) else {
        return plan;
    };
    for index in 0..2 {
        if held[index].is_some() {
            continue;
        }
        let voice = index + 1;
        if let Some(pitch) = passing_pitch(
            key,
            plan.chord,
            old.realized.notes[voice],
            plan.voicing.notes[voice],
        ) {
            if let Some(realized) = ornament_realization(
                key,
                plan.chord,
                plan.realized,
                voice,
                pitch,
                old.realized,
                plan.voicing,
                held,
            ) {
                plan.realized = realized;
                plan.inner_ornaments[index] = Some(InnerOrnament {
                    from: pitch,
                    to: plan.voicing.notes[voice],
                    bank: AnchorVoice::Melody,
                });
                plan.inner_history = inner_history(previous, realized);
                return plan;
            }
        }
    }
    // A prepared neighbour is useful when a sustained structural voice can
    // answer the melody without removing the chord's root/third foundation.
    let direction = if plan.voicing.notes[3] > old.voicing.notes[3] {
        -1
    } else {
        1
    };
    for index in 0..2 {
        if held[index].is_some() {
            continue;
        }
        let voice = index + 1;
        let target = plan.voicing.notes[voice];
        if target != old.realized.notes[voice] {
            continue;
        }
        let Some(pitch) = key.diatonic_step(target, direction) else {
            continue;
        };
        if plan.chord.tones().contains(pitch) {
            continue;
        }
        if let Some(realized) = ornament_realization(
            key,
            plan.chord,
            plan.realized,
            voice,
            pitch,
            old.realized,
            plan.voicing,
            held,
        ) {
            plan.realized = realized;
            plan.inner_ornaments[index] = Some(InnerOrnament {
                from: pitch,
                to: target,
                bank: AnchorVoice::Melody,
            });
            plan.inner_history = inner_history(previous, realized);
            break;
        }
    }
    plan
}

fn varied_gesture(gesture: Gesture) -> bool {
    matches!(
        gesture,
        Gesture::Reharm | Gesture::Substitute | Gesture::Reinterpret(_)
    )
}

fn compatible_substitute(key: Key, old: Plan, chord: Chord) -> bool {
    if old.chord == chord {
        return true;
    }
    let same_function = match tonal_function(key, old.chord) {
        Some(function) => tonal_function(key, chord) == Some(function),
        None => old.chord.root == chord.root,
    };
    same_function
        && old
            .tendency
            .is_none_or(|goal| chord == goal || abstract_destination(key, chord) == Some(goal))
}

#[expect(
    clippy::too_many_arguments,
    reason = "rank one literal anchor's meaning alongside its prepared ornament and pedal"
)]
fn interpretation_cost(
    chord: Chord,
    bass: u8,
    melody: u8,
    previous: Option<Plan>,
    gesture: Gesture,
    interpretation: Option<AnchorVoice>,
    ornament: Option<MelodyOrnament>,
    pedal: Option<u8>,
) -> u32 {
    let Some(voice) = interpretation else {
        return 0;
    };
    let pitch = match voice {
        AnchorVoice::Bass => bass,
        AnchorVoice::Melody => melody,
    };
    let role = chord.role(pitch);
    let mut cost = match voice {
        AnchorVoice::Bass => match role {
            Some(ToneRole::Third | ToneRole::Seventh) => 0,
            Some(ToneRole::Fifth) => 32,
            Some(ToneRole::Root) => 192,
            None if pedal.is_some() => 8,
            None => 96,
        },
        AnchorVoice::Melody => {
            if role == Some(ToneRole::Seventh)
                || chord.colour() == Some(pitch % 12)
                || ornament.is_some()
                || (chord.shape.suspended() && pitch % 12 == chord.third())
            {
                0
            } else {
                192
            }
        }
    };
    if gesture == Gesture::Reinterpret(voice) {
        if let Some(old) = previous {
            if chord.root == old.chord.root {
                cost += 256;
            }
            let old_pitch = match voice {
                AnchorVoice::Bass => old.voicing.notes[0],
                AnchorVoice::Melody => old.voicing.notes[3],
            };
            if role == old.chord.role(old_pitch)
                && (chord.colour() == Some(pitch % 12))
                    == (old.chord.colour() == Some(old_pitch % 12))
            {
                cost += 256;
            }
        }
    }
    cost
}

fn steered_recipe_cost(
    key: Key,
    chord: Chord,
    resolution: Option<Chord>,
    previous: Option<Plan>,
    gesture: Gesture,
    preferences: Guidance,
) -> u32 {
    let steering = preferences.steering;
    use HarmonicFunction::{Dominant, Predominant, Tonic};
    let function = tonal_function(key, chord);
    let request = if matches!(gesture, Gesture::Follow | Gesture::Reinterpret(_)) {
        match steering {
            Steering::Prepare => Gesture::Prepare,
            Steering::Lean => Gesture::Tension,
            Steering::Arrive => Gesture::Resolve,
            _ => gesture,
        }
    } else {
        gesture
    };
    let mut cost = match request {
        Gesture::Prepare => {
            (if function == Some(Predominant) {
                0
            } else {
                384
            }) + 16 * u32::from(chord.shape.tension())
        }
        Gesture::Tension => {
            (match function {
                Some(Dominant) => {
                    if chord.seventh().is_some() {
                        0
                    } else {
                        48
                    }
                }
                Some(Predominant) => 96,
                _ => 320,
            }) + 8 * u32::from(chord.shape.tension())
        }
        Gesture::Relax => {
            48 * u32::from(chord.shape.tension()) + if function == Some(Dominant) { 64 } else { 0 }
        }
        Gesture::Resolve => {
            8 * u32::from(chord.shape.tension()) + if function == Some(Tonic) { 0 } else { 96 }
        }
        Gesture::Reinterpret(_) => 16 * u32::from(chord.shape.tension()),
        _ => recipe_cost(key, chord, resolution, previous, gesture),
    };
    if !matches!(
        request,
        Gesture::Follow
            | Gesture::Reharm
            | Gesture::Substitute
            | Gesture::Reinterpret(_)
            | Gesture::Role { .. }
    ) {
        cost += 8 * (chord.tones().bits() & !key.pitches().bits()).count_ones();
        if key.degree(chord.root).is_none() {
            cost += 80;
        }
        if abstract_destination(key, chord).is_none() && request == Gesture::Tension {
            cost += 128;
        }
        if request == Gesture::Tension {
            if resolution.is_none() {
                cost += 64;
            }
            if abstract_destination(key, chord).is_some_and(|goal| key.degree(goal.root).is_none())
            {
                cost += 80;
            }
        }
    }
    if steering == Steering::Ease && gesture == Gesture::Follow {
        cost += 24 * u32::from(chord.shape.tension());
    }
    if steering == Steering::Substitute
        && !preferences.borrowing
        && previous.is_some_and(|old| !compatible_substitute(key, old, chord))
    {
        cost += 512;
    }
    cost
}

fn key_triad(key: Key, root: u8) -> Option<Chord> {
    let triad = key.chord(key.degree(root)?, ChordKind::Triad);
    if triad.fifth != (root + 7) % 12 {
        return None;
    }
    Some(Chord {
        root,
        shape: if triad.third == (root + 3) % 12 {
            Shape::Minor
        } else {
            Shape::Major
        },
    })
}

fn dominant_destination(key: Key, chord: Chord) -> Option<Chord> {
    if !matches!(
        chord.shape,
        Shape::Dominant7
            | Shape::Dominant9
            | Shape::DominantFlat13
            | Shape::Dominant13
            | Shape::DominantFlat9
            | Shape::DominantSharp9
            | Shape::Augmented
    ) {
        return None;
    }
    let root = (chord.root + 5) % 12;
    if let Some(goal) = key_triad(key, root) {
        return Some(goal);
    }
    // A chromatic bII dominant can substitute for V without inventing a new key.
    if chord.shape != Shape::Augmented && key.degree(chord.root).is_none() {
        if let Some(goal) = key_triad(key, (chord.root + 11) % 12) {
            return Some(goal);
        }
    }
    Some(Chord {
        root,
        shape: Shape::Major,
    })
}

fn tritone_substitute(key: Key, chord: Chord) -> bool {
    dominant_destination(key, chord).is_some_and(|goal| goal.root == (chord.root + 11) % 12)
}

fn borrowed_cost(key: Key, chord: Chord, previous: Option<Plan>) -> u32 {
    let chromatic = chord.tones().bits() & !key.pitches().bits() != 0;
    let same_goal = previous
        .and_then(|old| old.tendency)
        .is_some_and(|goal| abstract_destination(key, chord) == Some(goal));
    if tritone_substitute(key, chord) && same_goal {
        0
    } else if chromatic {
        if abstract_destination(key, chord).is_some() {
            32
        } else {
            160
        }
    } else {
        224
    }
}

// Diatonic major triads retain their normal function; an altered major triad
// can tonicize a diatonic major/minor chord a fifth below it.
fn applied_major_destination(key: Key, chord: Chord) -> Option<Chord> {
    if chord.shape != Shape::Major {
        return None;
    }
    let source = key.chord(key.degree(chord.root)?, ChordKind::Triad);
    if source.third == chord.third() && source.fifth == chord.fifth() {
        return None;
    }
    let root = (chord.root + 5) % 12;
    let goal = key.chord(key.degree(root)?, ChordKind::Triad);
    if goal.fifth != (root + 7) % 12 {
        return None;
    }
    Some(Chord {
        root,
        shape: if goal.third == (root + 3) % 12 {
            Shape::Minor
        } else {
            Shape::Major
        },
    })
}

fn destination(key: Key, chord: Chord, bass: u8, melody: u8) -> Option<Chord> {
    let target = abstract_destination(key, chord)?;
    if target.tones().contains(bass) && target.tones().contains(melody) {
        return Some(target);
    }
    // Try the opposite chord quality when chromatic anchors require it.
    let shape = match target.shape {
        Shape::Major => Shape::Minor,
        Shape::Minor => Shape::Major,
        _ => return None,
    };
    let other = Chord {
        root: target.root,
        shape,
    };
    (other.tones().contains(bass) && other.tones().contains(melody)).then_some(other)
}

fn tonal_function(key: Key, chord: Chord) -> Option<HarmonicFunction> {
    use HarmonicFunction::{Dominant, Predominant};
    if applied_major_destination(key, chord).is_some() {
        return Some(Dominant);
    }
    if key.degree(chord.root) == Some(Degree::Ii)
        && matches!(chord.shape, Shape::Diminished | Shape::HalfDiminished7)
    {
        return Some(Predominant);
    }
    if matches!(
        chord.shape,
        Shape::Dominant7
            | Shape::Dominant9
            | Shape::DominantFlat13
            | Shape::Dominant13
            | Shape::DominantFlat9
            | Shape::DominantSharp9
            | Shape::DominantSus4
            | Shape::Augmented
            | Shape::Diminished
            | Shape::HalfDiminished7
            | Shape::Diminished7
    ) {
        return Some(Dominant);
    }
    let degree = key.degree(chord.root)?;
    let triad = key.chord(degree, ChordKind::Triad);
    if chord.shape.suspended() || (triad.third == chord.third() && triad.fifth == chord.fifth()) {
        Some(triad.function())
    } else if degree == Degree::Iv && chord.third() == (chord.root + 3) % 12 {
        Some(Predominant)
    } else {
        None
    }
}

fn chord_leading(key: Key, chord: Chord) -> Option<u8> {
    if tritone_substitute(key, chord) {
        chord.seventh()
    } else if applied_major_destination(key, chord).is_some() {
        Some(chord.third())
    } else if tonal_function(key, chord) == Some(HarmonicFunction::Predominant) {
        None
    } else {
        chord.leading()
    }
}

fn six_four(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Plan>,
    guidance: HarmonicIntent,
) -> Option<SixFourContext> {
    if !matches!(chord.shape, Shape::Major | Shape::Minor)
        || chord.role(voicing.notes[0]) != Some(ToneRole::Fifth)
    {
        return None;
    }
    Some(classify_six_four_motion(
        key,
        voicing,
        previous.map(|old| old.realized),
        previous.is_some_and(|old| old.chord == chord),
        chord.root == key.tonic()
            && (guidance == HarmonicIntent::Lean
                || previous.is_some_and(|old| {
                    tonal_function(key, old.chord) == Some(HarmonicFunction::Predominant)
                })),
    ))
}

fn abstract_destination(key: Key, chord: Chord) -> Option<Chord> {
    if chord.shape.suspended() {
        return Some(Chord {
            root: chord.root,
            shape: if chord.shape == Shape::DominantSus4 {
                Shape::Dominant7
            } else {
                key_triad(key, chord.root).map_or(Shape::Major, |goal| goal.shape)
            },
        });
    }
    if let Some(target) = dominant_destination(key, chord) {
        return Some(target);
    }
    if let Some(target) = applied_major_destination(key, chord) {
        return Some(target);
    }
    if tonal_function(key, chord) == Some(HarmonicFunction::Predominant)
        && (chord.seventh().is_some() || chord.shape == Shape::Diminished)
    {
        return Some(Chord {
            root: key.pitch(Degree::V),
            shape: if chord.seventh().is_some() {
                Shape::Dominant7
            } else {
                Shape::Major
            },
        });
    }
    let root = match chord.shape {
        Shape::Dominant7 | Shape::Dominant9 | Shape::DominantFlat13 | Shape::Augmented => {
            (chord.root % 12 + 5) % 12
        }
        Shape::Diminished | Shape::HalfDiminished7 | Shape::Diminished7 => {
            (chord.root % 12 + 1) % 12
        }
        Shape::MinorMajor7 => chord.root % 12,
        _ if tonal_function(key, chord) == Some(HarmonicFunction::Dominant) => key.tonic(),
        _ => return None,
    };
    let minor = key
        .degree(root)
        .is_some_and(|degree| key.chord(degree, ChordKind::Triad).third == (root + 3) % 12);
    Some(Chord {
        root,
        shape: if minor { Shape::Minor } else { Shape::Major },
    })
}

fn cadential_destination(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Plan>,
    guidance: HarmonicIntent,
) -> Option<Chord> {
    matches!(
        six_four(key, chord, voicing, previous, guidance),
        Some(SixFourContext::Cadential)
    )
    .then_some(Chord {
        root: key.pitch(Degree::V),
        shape: Shape::Major,
    })
}

fn phrase_destination(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Plan>,
    guidance: HarmonicIntent,
    gesture: Gesture,
    held: [Option<u8>; 2],
) -> Option<Chord> {
    if gesture == Gesture::Substitute {
        if let Some(old) = previous.filter(|old| compatible_substitute(key, *old, chord)) {
            if let Some(goal) = old.tendency {
                return Some(goal);
            }
        }
    }
    if matches!(
        gesture,
        Gesture::Reharm | Gesture::Role { .. } | Gesture::Reinterpret(_)
    ) && held.iter().all(Option::is_none)
        && tonal_function(key, chord) == Some(HarmonicFunction::Tonic)
    {
        return abstract_destination(key, chord);
    }
    if let Some(old) = previous {
        if old.tendency == Some(chord) {
            return abstract_destination(key, chord).or(Some(chord));
        }
        if old
            .tendencies
            .iter()
            .enumerate()
            .any(|(index, debt)| debt.is_some_and(|debt| voicing.notes[index + 1] == debt.from))
        {
            return old.tendency.or_else(|| abstract_destination(key, chord));
        }
    }
    if let Some(goal) = cadential_destination(key, chord, voicing, previous, guidance) {
        return Some(goal);
    }
    abstract_destination(key, chord)
}

fn melody_ornament(
    key: Key,
    chord: Chord,
    bass: u8,
    melody: u8,
    previous: Option<Plan>,
) -> Option<MelodyOrnament> {
    if chord.tones().contains(melody) {
        return None;
    }
    let old = previous?;
    if old.chord == chord && old.realized.notes[0] == bass {
        if let Some(ornament) = old.ornament {
            return (ornament.pitch == melody).then_some(ornament);
        }
        let origin = old.realized.notes[3];
        if !chord.tones().contains(origin) {
            return None;
        }
        return [-1, 1]
            .into_iter()
            .find(|step| key.diatonic_step(origin, *step) == Some(melody))
            .map(|direction| MelodyOrnament {
                origin,
                pitch: melody,
                direction,
                suspension: false,
            });
    }
    let interval = (melody % 12 + 12 - chord.root) % 12;
    if old.realized.notes[3] != melody
        || !(old.chord.tones().contains(melody)
            || old
                .ornament
                .is_some_and(|ornament| ornament.pitch == melody))
        || !matches!(interval, 2 | 5 | 9)
    {
        return None;
    }
    [-1, 1].into_iter().find_map(|direction| {
        let target = key.diatonic_step(melody, direction)?;
        ((60..=88).contains(&target)
            && chord.tones().contains(target)
            && (interval != 5
                || chord.shape.third() != 4
                || (direction == -1 && target % 12 == chord.third())))
        .then_some(MelodyOrnament {
            origin: target,
            pitch: melody,
            direction,
            suspension: true,
        })
    })
}

fn implied_melody_tone(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Plan>,
    ornament: Option<MelodyOrnament>,
) -> Option<u8> {
    let old = previous?;
    let ornament = ornament?;
    (matches!(chord.shape, Shape::Major | Shape::Minor)
        && old.chord == chord
        && old.realized.notes[..3] == voicing.notes[..3]
        && !ornament.suspension
        && key.leading_tone() != Some(ornament.origin % 12)
        && chord_leading(key, chord) != Some(ornament.origin % 12)
        && (old.realized.notes[3] == ornament.origin || old.ornament == Some(ornament)))
    .then_some(ornament.origin)
}

#[expect(
    clippy::too_many_arguments,
    reason = "separate the held preparation preference from the heard phrase's harmonic intent"
)]
fn follow_cost(
    key: Key,
    chord: Chord,
    bass: u8,
    melody: u8,
    previous: Option<Plan>,
    guidance: HarmonicIntent,
    ornament: Option<MelodyOrnament>,
    preparing: bool,
) -> u32 {
    use HarmonicFunction::{Dominant, Predominant, Tonic};
    let voicing = Voicing {
        notes: [bass, 0, 0, melody],
    };
    let context = six_four(key, chord, voicing, previous, guidance);
    let Some(old) = previous else {
        return if chord.root != key.tonic() { 4 } else { 0 }
            + if matches!(context, Some(SixFourContext::Unprepared)) {
                96
            } else {
                0
            };
    };
    let function = tonal_function(key, chord);
    let old_function = tonal_function(key, old.chord);
    let function = if matches!(context, Some(SixFourContext::Cadential)) {
        Some(Dominant)
    } else {
        function
    };
    let mut cost = match (old_function, function) {
        (Some(Predominant), Some(Dominant)) | (Some(Dominant), Some(Tonic)) => 0,
        (Some(Tonic), Some(Tonic)) => {
            if old.chord == chord {
                0
            } else {
                16
            }
        }
        (Some(Tonic), Some(Predominant)) => 64,
        (Some(Tonic), Some(Dominant)) => 80,
        (Some(Predominant), Some(Tonic)) if bass % 12 == key.tonic() => 0,
        (Some(a), Some(b)) if a == b => 32,
        (Some(Dominant), Some(Predominant)) => 160,
        _ => 96,
    };
    if let Some(goal) = old.tendency {
        if !preparing
            && goal.tones().contains(bass)
            && goal.tones().contains(melody)
            && chord != goal
        {
            cost += 512;
        }
    }
    cost += match chord.role(bass) {
        Some(ToneRole::Root) => 0,
        Some(ToneRole::Third) => 4,
        Some(ToneRole::Seventh) => 24,
        Some(ToneRole::Fifth) => match context {
            Some(SixFourContext::Arpeggiated | SixFourContext::Cadential) => 8,
            Some(SixFourContext::Pedal | SixFourContext::Passing) => 16,
            _ => 96,
        },
        None => {
            if old.chord.root == chord.root
                && old.realized.notes[3] == melody
                && [-1, 1]
                    .into_iter()
                    .any(|step| key.diatonic_step(old.realized.notes[0], step) == Some(bass))
            {
                24
            } else {
                160
            }
        }
    };
    let ordinary = matches!(
        chord.shape,
        Shape::Major | Shape::Minor | Shape::Diminished | Shape::Dominant7 | Shape::HalfDiminished7
    );
    if !ordinary {
        cost += 32 * u32::from(chord.shape.tension().max(1));
    }
    if key.degree(chord.root).is_none_or(|degree| {
        let diatonic = key.chord(degree, ChordKind::Triad);
        diatonic.third != chord.third() || diatonic.fifth != chord.fifth()
    }) {
        // Ordinary playing stays tonal; an explicit colour request can still
        // introduce an applied/borrowed chord and its cached destination.
        cost += 96;
    }
    if guidance == HarmonicIntent::Lean {
        if function == Some(Predominant) && chord.seventh().is_none() {
            cost += 64;
        }
        if function == Some(Dominant) && chord.seventh().is_none() {
            cost += 16;
        }
    }
    if guidance == HarmonicIntent::Arrive && function != Some(Tonic) {
        cost += 96;
    }
    if guidance != HarmonicIntent::Lean
        && function == Some(Dominant)
        && chord.root != key.pitch(Degree::V)
        && chord.root != key.pitch(Degree::Vii)
    {
        cost += 96;
    }
    let settling_melody = [
        (Degree::Ii, Degree::I),
        (Degree::Vii, Degree::I),
        (Degree::Iv, Degree::Iii),
    ]
    .into_iter()
    .any(|(from, to)| {
        old.realized.notes[3] % 12 == key.pitch(from) && melody % 12 == key.pitch(to)
    });
    if bass % 12 == key.tonic()
        && settling_melody
        && !(chord.root == key.tonic() && matches!(chord.shape, Shape::Major | Shape::Minor))
    {
        cost += 192;
    }
    if ornament.is_some() {
        cost += 8;
    }
    cost
}

fn tendency_cost(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Plan>,
    gesture: Gesture,
) -> u32 {
    if !matches!(gesture, Gesture::Follow | Gesture::Resolve) {
        return 0;
    }
    let Some(old) = previous else {
        return 0;
    };
    let arrival = old.tendency.is_some_and(|goal| {
        goal.tones().contains(voicing.notes[0]) && goal.tones().contains(voicing.notes[3])
    }) || old.tendency == Some(chord)
        || (tonal_function(key, old.chord) == Some(HarmonicFunction::Dominant)
            && tonal_function(key, chord) == Some(HarmonicFunction::Tonic));
    old.tendencies
        .iter()
        .enumerate()
        .filter_map(|(index, debt)| debt.map(|debt| (index + 1, debt)))
        .map(|(voice, debt)| {
            if voicing.notes[voice] == debt.to {
                0
            } else if arrival {
                1024
            } else if voicing.notes[voice] == debt.from {
                0
            } else {
                96
            }
        })
        .sum()
}

// When the outer seventh/leading tone predicts a melodic resolution, avoid
// placing an inner leading tone where no upper voice can fit after it rises.
fn cadence_space_cost(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Plan>,
    guidance: HarmonicIntent,
) -> u32 {
    let cadential = cadential_destination(key, chord, voicing, previous, guidance);
    let Some(goal) = cadential.or_else(|| abstract_destination(key, chord)) else {
        return 0;
    };
    let melody = voicing.notes[3];
    let predicted = if cadential.is_some() && !goal.tones().contains(melody) {
        [1, 2]
            .into_iter()
            .filter_map(|step| melody.checked_sub(step))
            .find(|note| goal.tones().contains(*note))
    } else if chord_leading(key, chord) == Some(melody % 12) {
        melody.checked_add(1)
    } else if chord.seventh() == Some(melody % 12) {
        [1, 2]
            .into_iter()
            .filter_map(|step| melody.checked_sub(step))
            .find(|note| goal.tones().contains(*note))
    } else if goal.tones().contains(melody) {
        Some(melody)
    } else {
        None
    };
    let Some(melody) = predicted else {
        return 0;
    };
    let debts = voice_tendencies(
        key,
        chord,
        voicing,
        voicing,
        previous,
        Gesture::Follow,
        [None; 2],
        guidance,
    );
    let continuing_bass = previous.and_then(|old| {
        [-1, 1]
            .into_iter()
            .find(|step| key.diatonic_step(old.voicing.notes[0], *step) == Some(voicing.notes[0]))
            .and_then(|step| key.diatonic_step(voicing.notes[0], step))
            .filter(|bass| (24..=48).contains(bass) && goal.tones().contains(*bass))
    });
    let bass = continuing_bass.or_else(|| {
        (24..=48)
            .filter(|note| note % 12 == goal.root)
            .min_by_key(|note| note.abs_diff(voicing.notes[0]))
    });
    let Some(bass) = bass else {
        return 0;
    };
    let mut possible = false;
    let valid = |target: Voicing| {
        let present = target
            .notes
            .into_iter()
            .fold(0u16, |bits, note| bits | (1 << (note % 12)));
        valid_realization(key, goal, target)
            && goal.essential().bits() & !present == 0
            && (previous.is_none() || !forbidden_parallels(voicing, target, true))
    };
    match debts {
        [Some(low), Some(high)] => {
            possible = valid(Voicing {
                notes: [bass, low.to, high.to, melody],
            });
        }
        [Some(low), None] => {
            for alto in 53..=76 {
                if goal.tones().contains(alto)
                    && valid(Voicing {
                        notes: [bass, low.to, alto, melody],
                    })
                {
                    possible = true;
                    break;
                }
            }
        }
        [None, Some(high)] => {
            for tenor in 48..=67 {
                if goal.tones().contains(tenor)
                    && valid(Voicing {
                        notes: [bass, tenor, high.to, melody],
                    })
                {
                    possible = true;
                    break;
                }
            }
        }
        [None, None] => return 0,
    }
    if possible {
        0
    } else {
        256
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "derive voice debts using the same heard harmony and cadential context as selection"
)]
fn voice_tendencies(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    realized: Voicing,
    previous: Option<Plan>,
    gesture: Gesture,
    held: [Option<u8>; 2],
    guidance: HarmonicIntent,
) -> [Option<VoiceTendency>; 2] {
    let cadential = cadential_destination(key, chord, voicing, previous, guidance);
    let goal = cadential.or_else(|| abstract_destination(key, chord));
    core::array::from_fn(|index| {
        let voice = index + 1;
        let from = realized.notes[voice];
        if let Some(debt) = previous.and_then(|old| old.tendencies[index]) {
            if from == debt.from && from != debt.to {
                let recoloured = matches!(gesture, Gesture::Reharm | Gesture::Role { .. })
                    && held[index].is_none()
                    && chord.tones().contains(from)
                    && tonal_function(key, chord) == Some(HarmonicFunction::Tonic);
                if !recoloured {
                    return Some(debt);
                }
            }
        }
        if from != voicing.notes[voice] {
            return Some(VoiceTendency {
                from,
                to: voicing.notes[voice],
            });
        }
        let target = goal?;
        let leading = chord_leading(key, chord).or_else(|| {
            (tonal_function(key, chord) == Some(HarmonicFunction::Dominant))
                .then(|| key.leading_tone())
                .flatten()
        });
        let to = if chord.shape.suspended() && from % 12 == chord.third() {
            let third = target.third();
            [-2i16, -1, 1, 2]
                .into_iter()
                .map(|step| i16::from(from) + step)
                .find(|note| (0..=127).contains(note) && *note as u8 % 12 == third)
                .map(|note| note as u8)
        } else if leading == Some(from % 12) {
            from.checked_add(1)
        } else if (tritone_substitute(key, chord) && from % 12 == chord.third())
            || chord.seventh() == Some(from % 12)
            || (cadential.is_some() && [chord.root, chord.third()].contains(&(from % 12)))
            || (matches!(
                chord.shape,
                Shape::Diminished | Shape::HalfDiminished7 | Shape::Diminished7
            ) && tonal_function(key, chord) == Some(HarmonicFunction::Dominant)
                && from % 12 == chord.fifth())
        {
            [1, 2]
                .into_iter()
                .filter_map(|step| from.checked_sub(step))
                .find(|note| target.tones().contains(*note))
        } else {
            None
        }?;
        Some(VoiceTendency { from, to })
    })
}

fn recipe_cost(
    key: Key,
    chord: Chord,
    resolution: Option<Chord>,
    previous: Option<Plan>,
    gesture: Gesture,
) -> u32 {
    let tier = chord.shape.tension();
    let mut cost = match gesture {
        Gesture::Tension => {
            let desired = previous.map_or(1, |old| (old.tension.min(3) + 1).min(3));
            2048 * u32::from(tier.abs_diff(desired))
        }
        Gesture::Relax | Gesture::Resolve => 1024 * u32::from(tier),
        Gesture::Follow => previous.map_or(256 * u32::from(tier), |_| 0),
        Gesture::Prepare => {
            24 * u32::from(tier)
                + if tonal_function(key, chord) == Some(HarmonicFunction::Predominant) {
                    0
                } else {
                    384
                }
        }
        Gesture::Reharm | Gesture::Role { .. } | Gesture::Substitute | Gesture::Reinterpret(_) => {
            previous.map_or(0, |old| 48 * u32::from(tier.abs_diff(old.tension)))
        }
    };
    if tier >= 2
        && resolution.is_none()
        && (gesture != Gesture::Follow || abstract_destination(key, chord).is_none())
    {
        cost += 192;
    }
    if let Some(degree) = key.degree(chord.root) {
        let triad = key.chord(degree, ChordKind::Triad);
        if triad.third != chord.third() || triad.fifth != chord.fifth() {
            cost += 32;
        }
        if matches!(gesture, Gesture::Relax | Gesture::Resolve) {
            cost += match degree {
                Degree::I => 0,
                Degree::Vi | Degree::Iii => 48,
                _ => 96,
            };
        }
        if let Some(old) = previous {
            let old_degree = key.degree(old.chord.root);
            let same_function = matches!(
                (old_degree, Some(degree)),
                (
                    Some(Degree::I | Degree::Iii | Degree::Vi),
                    Some(Degree::I | Degree::Iii | Degree::Vi)
                ) | (Some(Degree::Ii | Degree::Iv), Some(Degree::Ii | Degree::Iv))
                    | (Some(Degree::V | Degree::Vii), Some(Degree::V | Degree::Vii))
            );
            if !same_function && gesture == Gesture::Reharm {
                cost += 24;
            }
        }
    } else {
        cost += 80;
    }
    cost += 8 * (chord.tones().bits() & !key.pitches().bits()).count_ones();
    if let Some(old) = previous.filter(|_| gesture == Gesture::Reharm) {
        let misses_family = match old.variation % 3 {
            0 => chord == old.chord,
            1 => chord != old.chord,
            _ => tier == old.tension,
        };
        if misses_family {
            cost += 384;
        }
    }
    cost
}

// The small 0..127 perturbation changes only U's soft ranking. Recent-note
// avoidance, harmonic family and hard voice-leading rules remain independent.
fn variation_bias(counter: u32, voicing: Voicing) -> u32 {
    let notes = u32::from(voicing.notes[1]) | (u32::from(voicing.notes[2]) << 8);
    let mut mixed = counter.wrapping_mul(0x9e37_79b9) ^ notes;
    mixed = (mixed ^ (mixed >> 16)).wrapping_mul(0x85eb_ca6b);
    mixed = (mixed ^ (mixed >> 13)).wrapping_mul(0xc2b2_ae35);
    (mixed ^ (mixed >> 16)) & 127
}

fn resolution_targets(key: Key, previous: Plan, chord: Chord, target: Voicing) -> bool {
    if previous.resolution != Some(chord) {
        return false;
    }
    for voice in 1..3 {
        let note = previous.realized.notes[voice];
        if let Some(debt) = previous.tendencies[voice - 1].filter(|debt| note == debt.from) {
            if target.notes[voice] != debt.to {
                return false;
            }
            continue;
        }
        if chord_leading(key, previous.chord) == Some(note % 12) {
            if note.checked_add(1) != Some(target.notes[voice]) {
                return false;
            }
        } else if previous.chord.seventh() == Some(note % 12)
            && (target.notes[voice] >= note || note - target.notes[voice] > 2)
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test assertions")]
mod tests {
    use super::*;

    fn choose(previous: Option<Plan>, gesture: Gesture) -> Plan {
        select(Key::C_MAJOR, 36, 64, previous, [None; 2], gesture).unwrap()
    }

    fn applied_dominant() -> Plan {
        let mut plan = choose(None, Gesture::Follow);
        for _ in 0..3 {
            plan = choose(Some(plan), Gesture::Tension);
        }
        plan
    }

    fn follow(previous: Option<Plan>, bass: u8, melody: u8) -> Plan {
        select(
            Key::C_MAJOR,
            bass,
            melody,
            previous,
            [None; 2],
            Gesture::Follow,
        )
        .unwrap()
    }

    fn steered(previous: Option<Plan>, bass: u8, melody: u8, gesture: Gesture) -> Plan {
        guided(previous, bass, melody, gesture, Guidance::default())
    }

    fn guided(
        previous: Option<Plan>,
        bass: u8,
        melody: u8,
        gesture: Gesture,
        guidance: Guidance,
    ) -> Plan {
        select_steered(
            Key::C_MAJOR,
            bass,
            melody,
            previous,
            [None; 2],
            gesture,
            guidance,
        )
        .unwrap()
    }

    #[test]
    fn extra_held_tones_suggest_compatible_chords_under_the_same_anchors() {
        let home = steered(None, 36, 64, Gesture::Follow);
        let with_extra = |note: u8| {
            guided(
                Some(home),
                36,
                64,
                Gesture::Follow,
                Guidance {
                    extra_tones: PitchClassSet::from_bits(1 << (note % 12)),
                    ..Guidance::default()
                },
            )
        };
        let sixth = with_extra(45);
        let seventh = with_extra(47);
        assert_eq!(sixth.chord, Chord::new(9, Shape::Minor).unwrap());
        assert_eq!(seventh.chord, Chord::new(0, Shape::Major7).unwrap());
        for (plan, extra) in [(sixth, 45), (seventh, 47)] {
            assert_eq!(plan.realized.notes[0], 36);
            assert_eq!(plan.realized.notes[3], 64);
            assert!(plan.chord.tones().contains(extra));
            assert!(valid_realization(Key::C_MAJOR, plan.chord, plan.realized));
            assert_eq!(
                tonal_function(Key::C_MAJOR, plan.chord),
                Some(HarmonicFunction::Tonic)
            );
        }
    }

    #[test]
    fn arbitrary_extra_cluster_neither_vetoes_a_plan_nor_supplies_missing_voices() {
        let home = steered(None, 36, 64, Gesture::Follow);
        let cluster = PitchClassSet::from_bits(0x0fff);
        for gesture in [
            Gesture::Follow,
            Gesture::Prepare,
            Gesture::Tension,
            Gesture::Resolve,
        ] {
            let plan = guided(
                Some(home),
                36,
                64,
                gesture,
                Guidance {
                    extra_tones: cluster,
                    ..Guidance::default()
                },
            );
            assert_eq!(plan.realized.notes[0], 36);
            assert_eq!(plan.realized.notes[3], 64);
            assert!(valid_realization(Key::C_MAJOR, plan.chord, plan.realized));
            assert!(!plan.root_implied);
            let mut actual = PitchClassSet::default();
            for note in plan.realized.notes {
                actual.insert(note);
            }
            assert_eq!(plan.chord.essential().bits() & !actual.bits(), 0);
            assert_ne!(cluster.bits() & !plan.chord.tones().bits(), 0);
        }
    }

    #[test]
    fn phrase_events_distinguish_decoration_emphasis_and_arrival() {
        let home = steered(None, 36, 64, Gesture::Follow);
        for event in [HarmonicEvent::Continue, HarmonicEvent::Reconsider] {
            let decoration = guided(
                Some(home),
                36,
                65,
                Gesture::Follow,
                Guidance {
                    event,
                    ..Guidance::default()
                },
            );
            assert_eq!(decoration.chord, home.chord);
            assert_eq!(decoration.realized.notes[..3], home.realized.notes[..3]);
            assert!(decoration.ornament.is_some());
        }
        let emphasized = guided(
            Some(home),
            36,
            65,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Structural,
                ..Guidance::default()
            },
        );
        assert!(emphasized.ornament.is_none());
        assert!(emphasized.chord.tones().contains(65));
        assert_ne!(emphasized.chord, home.chord);

        let tension = steered(Some(home), 36, 64, Gesture::Tension);
        let arrival = guided(
            Some(tension),
            36,
            64,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Arrive,
                ..Guidance::default()
            },
        );
        assert_eq!(Some(arrival.chord), tension.tendency);
        assert_eq!(arrival.realized.notes, [36, 57, 60, 64]);
        assert!(arrival.tendencies.iter().all(Option::is_none));

        let held_prepare = guided(
            Some(home),
            36,
            64,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Continue,
                steering: Steering::Prepare,
                ..Guidance::default()
            },
        );
        assert_eq!(
            tonal_function(Key::C_MAJOR, held_prepare.chord),
            Some(HarmonicFunction::Predominant)
        );
    }

    #[test]
    fn established_dominant_root_can_be_implied_under_a_literal_thirteenth() {
        let home = steered(None, 36, 67, Gesture::Follow);
        let dominant = steered(Some(home), 36, 67, Gesture::Tension);
        assert_eq!(dominant.realized.notes, [36, 59, 65, 67]);
        assert!(!dominant.root_implied);
        let thirteenth = guided(
            Some(dominant),
            36,
            76,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Continue,
                ..Guidance::default()
            },
        );
        assert_eq!(thirteenth.chord, Chord::new(7, Shape::Dominant13).unwrap());
        assert_eq!(thirteenth.realized.notes, [36, 59, 65, 76]);
        assert!(thirteenth.root_implied);
        assert_eq!(thirteenth.tendency, dominant.tendency);
        assert_eq!(thirteenth.tendencies, dominant.tendencies);
        assert!(thirteenth.inner_ornaments.iter().all(Option::is_none));
        let structural_colour = guided(
            Some(dominant),
            36,
            76,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Structural,
                ..Guidance::default()
            },
        );
        assert_eq!(structural_colour.realized, thirteenth.realized);
        assert!(structural_colour.root_implied);
        let held_intensity = guided(
            Some(dominant),
            36,
            76,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Structural,
                steering: Steering::Lean,
                ..Guidance::default()
            },
        );
        assert_eq!(held_intensity.realized, thirteenth.realized);
        assert!(held_intensity.root_implied);
        let explicit_arrival = guided(
            Some(dominant),
            36,
            76,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Arrive,
                ..Guidance::default()
            },
        );
        assert_eq!(Some(explicit_arrival.chord), dominant.tendency);
        assert!(!explicit_arrival.root_implied);
        let held_arrival = guided(
            Some(thirteenth),
            36,
            76,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Continue,
                steering: Steering::Arrive,
                ..Guidance::default()
            },
        );
        assert_eq!(Some(held_arrival.chord), dominant.tendency);
        assert!(!held_arrival.root_implied);
        let refreshed = refresh_resolution(Key::C_MAJOR, thirteenth, [None; 2]);
        assert_eq!(refreshed, thirteenth);
        let retained =
            release_voices(Key::C_MAJOR, thirteenth, [Some(59), Some(65)], [false; 2]).unwrap();
        assert!(retained.root_implied);
        assert_eq!(retained.realized, thirteenth.realized);
        let arrival = steered(Some(retained), 36, 76, Gesture::Resolve);
        assert_eq!(Some(arrival.chord), dominant.tendency);
        assert_eq!(arrival.realized.notes, [36, 60, 64, 76]);
        assert!(!arrival.root_implied);
    }

    #[test]
    fn root_implication_cannot_be_invented_or_survive_lost_guide_or_pedal_proof() {
        let chord = Chord::new(7, Shape::Dominant13).unwrap();
        let home = steered(None, 36, 67, Gesture::Follow);
        let dominant = steered(Some(home), 36, 67, Gesture::Tension);
        assert!(!established_root(Key::C_MAJOR, chord, 36, None));
        assert!(!established_root(Key::C_MAJOR, chord, 36, Some(home)));
        assert!(!established_root(Key::C_MAJOR, chord, 38, Some(dominant)));
        let mut lost = dominant;
        lost.pedal_prepared = None;
        assert!(!established_root(Key::C_MAJOR, chord, 36, Some(lost)));
        lost = dominant;
        lost.realized.notes[2] = 64;
        assert!(!established_root(Key::C_MAJOR, chord, 36, Some(lost)));
        assert!(search(
            Key::C_MAJOR,
            36,
            76,
            Some(lost),
            [None; 2],
            Gesture::Follow,
            Some(chord),
            false,
            Some(Guidance {
                extensions: true,
                ..Guidance::default()
            })
        )
        .is_none());
        lost = dominant;
        lost.tendency = Chord::new(9, Shape::Minor);
        assert!(!established_root(Key::C_MAJOR, chord, 36, Some(lost)));
        let fresh = guided(
            None,
            36,
            76,
            Gesture::Follow,
            Guidance {
                event: HarmonicEvent::Continue,
                extensions: true,
                ..Guidance::default()
            },
        );
        assert!(!fresh.root_implied);
    }

    #[test]
    fn prepare_and_direct_bars_change_harmony_around_literal_keyboards() {
        let home = steered(None, 36, 64, Gesture::Follow);
        let approach = steered(Some(home), 36, 64, Gesture::Prepare);
        assert_eq!(approach.chord, Chord::new(5, Shape::Major7).unwrap());
        assert_eq!(approach.tendency, Chord::new(7, Shape::Dominant7));
        let bass = steered(Some(home), 36, 64, Gesture::Reinterpret(AnchorVoice::Bass));
        let melody = steered(
            Some(home),
            36,
            64,
            Gesture::Reinterpret(AnchorVoice::Melody),
        );
        assert_eq!(bass.chord, Chord::new(9, Shape::Minor).unwrap());
        assert_eq!(bass.chord.role(36), Some(ToneRole::Third));
        assert_eq!(melody.chord, Chord::new(5, Shape::Major7).unwrap());
        assert_eq!(melody.chord.role(64), Some(ToneRole::Seventh));
        for voice in [AnchorVoice::Bass, AnchorVoice::Melody] {
            let mut previous = home;
            for _ in 0..4 {
                let next = guided(
                    Some(previous),
                    36,
                    64,
                    Gesture::Reinterpret(voice),
                    Guidance {
                        interpretation: Some(voice),
                        ..Guidance::default()
                    },
                );
                assert_ne!(next.realized, previous.realized);
                assert_eq!(next.realized.notes[0], 36);
                assert_eq!(next.realized.notes[3], 64);
                assert!(!forbidden_parallels(previous.realized, next.realized, true));
                previous = next;
            }
        }
    }

    #[test]
    fn substitution_preserves_function_and_pending_destination_without_borrowing() {
        let home = steered(None, 36, 64, Gesture::Follow);
        for initial in [home, steered(Some(home), 36, 64, Gesture::Tension)] {
            let mut previous = initial;
            for _ in 0..8 {
                let next = guided(
                    Some(previous),
                    36,
                    64,
                    Gesture::Substitute,
                    Guidance {
                        steering: Steering::Substitute,
                        ..Guidance::default()
                    },
                );
                assert!(compatible_substitute(Key::C_MAJOR, previous, next.chord));
                assert_eq!(
                    tonal_function(Key::C_MAJOR, next.chord),
                    tonal_function(Key::C_MAJOR, initial.chord)
                );
                assert_eq!(next.tendency, initial.tendency);
                assert_eq!([next.realized.notes[0], next.realized.notes[3]], [36, 64]);
                previous = next;
            }
        }
    }

    #[test]
    fn independent_licenses_choose_audible_colours_and_directed_borrowing() {
        let home = steered(None, 36, 64, Gesture::Follow);
        let colours = [(false, false), (true, false), (false, true), (true, true)].map(
            |(borrowing, extensions)| {
                guided(
                    Some(home),
                    36,
                    64,
                    Gesture::Substitute,
                    Guidance {
                        steering: Steering::Substitute,
                        borrowing,
                        extensions,
                        ..Guidance::default()
                    },
                )
            },
        );
        assert_eq!(
            colours.map(|plan| plan.realized.notes),
            [
                [36, 55, 59, 64], // Em/C, a tonic-family substitute.
                [36, 56, 60, 64], // Eaug/C, an applied pull to Am.
                [36, 57, 59, 64], // Cmaj13, a tonic seventh/thirteenth shell.
                [36, 57, 58, 64], // C13, a borrowed pull toward F.
            ]
        );
        assert_eq!(colours[1].tendency, Chord::new(9, Shape::Minor));
        assert_eq!(
            colours[1].tendencies[0],
            Some(VoiceTendency { from: 56, to: 57 })
        );
        assert_eq!(colours[2].chord.shape, Shape::Major13);
        assert_eq!(colours[3].tendency, Chord::new(5, Shape::Major));
        assert_eq!(
            colours[3].tendencies[1],
            Some(VoiceTendency { from: 58, to: 57 })
        );
        for rich in [colours[2], colours[3]] {
            let held = [Some(rich.realized.notes[1]), Some(rich.realized.notes[2])];
            let refreshed = refresh_resolution(Key::C_MAJOR, rich, held);
            assert_eq!(refreshed.realized, rich.realized);
            assert_eq!(refreshed.tendencies, rich.tendencies);
            let retained = release_voices(Key::C_MAJOR, rich, [None; 2], [true; 2]).unwrap();
            assert_eq!(retained.chord, rich.chord);
            assert_eq!(retained.realized, rich.realized);
            let ordinary = steered(Some(rich), 36, 64, Gesture::Substitute);
            assert!(!ordinary.chord.shape.licensed_extension());
        }
    }

    #[test]
    fn licenses_preserve_prepare_and_arrive_intent_including_a_saved_goal() {
        let home = steered(None, 36, 64, Gesture::Follow);
        let tension = steered(Some(home), 36, 64, Gesture::Tension);
        for (borrowing, extensions) in [(false, false), (true, false), (false, true), (true, true)]
        {
            let preferences = Guidance {
                borrowing,
                extensions,
                ..Guidance::default()
            };
            for (gesture, steering) in [
                (Gesture::Prepare, Steering::Prepare),
                (Gesture::Follow, Steering::Prepare),
            ] {
                let prepared = guided(
                    Some(home),
                    36,
                    64,
                    gesture,
                    Guidance {
                        steering,
                        ..preferences
                    },
                );
                assert_eq!(
                    tonal_function(Key::C_MAJOR, prepared.chord),
                    Some(HarmonicFunction::Predominant)
                );
                let continuing = guided(
                    Some(prepared),
                    38,
                    65,
                    Gesture::Follow,
                    Guidance {
                        steering: Steering::Prepare,
                        ..preferences
                    },
                );
                assert_eq!(
                    tonal_function(Key::C_MAJOR, continuing.chord),
                    Some(HarmonicFunction::Predominant)
                );
            }
            for (gesture, steering) in [
                (Gesture::Resolve, Steering::Arrive),
                (Gesture::Follow, Steering::Arrive),
            ] {
                let settled = guided(
                    Some(home),
                    36,
                    64,
                    gesture,
                    Guidance {
                        steering,
                        ..preferences
                    },
                );
                assert_eq!(
                    tonal_function(Key::C_MAJOR, settled.chord),
                    Some(HarmonicFunction::Tonic)
                );
                assert!(settled.tendency.is_none());
                assert!(settled.tendencies.iter().all(Option::is_none));
            }
            let arrival = guided(
                Some(tension),
                36,
                64,
                Gesture::Resolve,
                Guidance {
                    steering: Steering::Arrive,
                    ..preferences
                },
            );
            assert_eq!(Some(arrival.chord), tension.resolution);
            assert!(arrival.tendencies.iter().all(Option::is_none));
        }
    }

    #[test]
    fn suspended_shapes_replace_the_third_and_owe_its_local_resolution() {
        for (shape, notes, target, expected) in [
            (
                Shape::Sus2,
                [36, 55, 62, 72],
                Shape::Major,
                VoiceTendency { from: 62, to: 64 },
            ),
            (
                Shape::Sus4,
                [36, 55, 65, 72],
                Shape::Major,
                VoiceTendency { from: 65, to: 64 },
            ),
        ] {
            let chord = Chord::new(0, shape).unwrap();
            assert_eq!(chord.role(chord.third()), None);
            assert!(chord.essential().contains(chord.fifth()));
            assert_eq!(
                abstract_destination(Key::C_MAJOR, chord),
                Chord::new(0, target)
            );
            let voicing = Voicing { notes };
            assert!(valid_realization(Key::C_MAJOR, chord, voicing));
            let debts = voice_tendencies(
                Key::C_MAJOR,
                chord,
                voicing,
                voicing,
                None,
                Gesture::Follow,
                [None; 2],
                HarmonicIntent::Continue,
            );
            assert_eq!(debts[1], Some(expected));
        }
        let chord = Chord::new(7, Shape::DominantSus4).unwrap();
        assert_eq!(
            abstract_destination(Key::C_MAJOR, chord),
            Chord::new(7, Shape::Dominant7)
        );
        let voicing = Voicing {
            notes: [43, 60, 65, 67],
        };
        let debts = voice_tendencies(
            Key::C_MAJOR,
            chord,
            voicing,
            voicing,
            None,
            Gesture::Follow,
            [None; 2],
            HarmonicIntent::Continue,
        );
        assert_eq!(debts[0], Some(VoiceTendency { from: 60, to: 59 }));
    }

    #[test]
    fn tritone_substitution_keeps_shared_goal_and_opposite_guide_directions() {
        let home = steered(None, 43, 71, Gesture::Follow);
        let mut dominant = search(
            Key::C_MAJOR,
            43,
            71,
            Some(home),
            [None; 2],
            Gesture::Tension,
            Chord::new(7, Shape::Dominant7),
            false,
            Some(Guidance {
                steering: Steering::Lean,
                ..Guidance::default()
            }),
        )
        .unwrap();
        // Fix the registration to isolate the shared-guide substitution rather
        // than the selector's choice of octave for the source seventh.
        dominant.voicing = Voicing {
            notes: [43, 62, 65, 71],
        };
        dominant.realized = dominant.voicing;
        dominant = refresh_resolution(Key::C_MAJOR, dominant, [None; 2]);
        assert_eq!(dominant.realized.notes, [43, 62, 65, 71]);
        let substitute = Chord::new(1, Shape::Dominant7).unwrap();
        let borrowed = search(
            Key::C_MAJOR,
            43,
            71,
            Some(dominant),
            [None; 2],
            Gesture::Substitute,
            Some(substitute),
            false,
            Some(Guidance {
                borrowing: true,
                ..Guidance::default()
            }),
        )
        .unwrap();
        assert_eq!(borrowed.realized.notes, [43, 61, 65, 71]);
        assert_eq!(borrowed.pedal, Some(43));
        assert_eq!(borrowed.tendency, Chord::new(0, Shape::Major));
        assert_eq!(chord_leading(Key::C_MAJOR, substitute), Some(11));
        assert_eq!(
            borrowed.tendencies[1],
            Some(VoiceTendency { from: 65, to: 64 })
        );
        let arrival = steered(Some(borrowed), 36, 72, Gesture::Follow);
        assert_eq!(arrival.chord, Chord::new(0, Shape::Major).unwrap());
        assert_eq!(arrival.realized.notes[2], 64);
        assert_eq!(
            abstract_destination(Key::C_MAJOR, Chord::new(5, Shape::Dominant7).unwrap()),
            Chord::new(10, Shape::Major)
        );
        let augmented = Chord::new(1, Shape::Augmented).unwrap();
        assert!(!tritone_substitute(Key::C_MAJOR, augmented));
        assert_eq!(
            abstract_destination(Key::C_MAJOR, augmented),
            Chord::new(6, Shape::Major)
        );
        assert_eq!(chord_leading(Key::C_MAJOR, augmented), Some(5));
    }

    #[test]
    fn altered_major_triads_tonicize_without_doubling_their_leading_tone() {
        for (root, target, shape) in [
            (2, 7, Shape::Major),
            (4, 9, Shape::Minor),
            (9, 2, Shape::Minor),
            (11, 4, Shape::Minor),
        ] {
            let chord = Chord::new(root, Shape::Major).unwrap();
            assert_eq!(
                applied_major_destination(Key::C_MAJOR, chord),
                Chord::new(target, shape)
            );
            assert_eq!(
                tonal_function(Key::C_MAJOR, chord),
                Some(HarmonicFunction::Dominant)
            );
            assert_eq!(chord_leading(Key::C_MAJOR, chord), Some(chord.third()));
        }
        for (root, function) in [
            (0, HarmonicFunction::Tonic),
            (5, HarmonicFunction::Predominant),
            (7, HarmonicFunction::Dominant),
        ] {
            let chord = Chord::new(root, Shape::Major).unwrap();
            assert_eq!(applied_major_destination(Key::C_MAJOR, chord), None);
            assert_eq!(tonal_function(Key::C_MAJOR, chord), Some(function));
        }
        let minor = Key::new(9, super::super::Mode::NaturalMinor).unwrap();
        let e = Chord::new(4, Shape::Major).unwrap();
        assert_eq!(abstract_destination(minor, e), Chord::new(9, Shape::Minor));
        assert_eq!(chord_leading(minor, e), Some(8));

        let first = steered(None, 38, 69, Gesture::Follow);
        let applied = steered(Some(first), 38, 69, Gesture::Reharm);
        assert_eq!(applied.chord, Chord::new(2, Shape::Major).unwrap());
        assert_eq!(applied.realized.notes, [38, 54, 62, 69]);
        assert_eq!(
            applied.tendencies[0],
            Some(VoiceTendency { from: 54, to: 55 })
        );
        assert_eq!(applied.tendency, Chord::new(7, Shape::Major));
        assert!(!valid_realization(
            Key::C_MAJOR,
            applied.chord,
            Voicing {
                notes: [38, 54, 66, 74]
            }
        ));
        let arrival = steered(Some(applied), 43, 71, Gesture::Follow);
        assert_eq!(arrival.realized.notes, [43, 55, 62, 71]);
    }

    #[test]
    fn leading_tone_chords_resolve_the_diminished_fifth_down() {
        let first = steered(None, 45, 71, Gesture::Follow);
        let leading = steered(Some(first), 45, 71, Gesture::Tension);
        assert_eq!(
            leading.chord,
            Chord::new(11, Shape::HalfDiminished7).unwrap()
        );
        assert_eq!(leading.realized.notes, [45, 53, 62, 71]);
        assert_eq!(
            leading.tendencies[0],
            Some(VoiceTendency { from: 53, to: 52 })
        );
        let arrival = steered(Some(leading), 36, 72, Gesture::Follow);
        assert_eq!(arrival.realized.notes[1], 52);
        assert!(!forbidden_parallels(
            leading.realized,
            arrival.realized,
            true
        ));

        let key = Key::new(9, super::super::Mode::NaturalMinor).unwrap();
        let predominant = Chord::new(11, Shape::HalfDiminished7).unwrap();
        let voicing = Voicing {
            notes: [47, 57, 65, 74],
        };
        let debts = voice_tendencies(
            key,
            predominant,
            voicing,
            voicing,
            None,
            Gesture::Follow,
            [None; 2],
            HarmonicIntent::Continue,
        );
        assert_eq!(debts[0], Some(VoiceTendency { from: 57, to: 56 }));
        assert_eq!(debts[1], None);
    }

    #[test]
    fn cadential_six_four_doubles_bass_and_owes_its_sixth_and_fourth_down() {
        let opening = steered(None, 36, 64, Gesture::Follow);
        let predominant = steered(Some(opening), 38, 65, Gesture::Follow);
        let cadence = steered(Some(predominant), 43, 72, Gesture::Follow);
        assert_eq!(cadence.chord, Chord::new(0, Shape::Major).unwrap());
        assert_eq!(cadence.realized.notes, [43, 55, 64, 72]);
        assert_eq!(
            cadence.tendencies[1],
            Some(VoiceTendency { from: 64, to: 62 })
        );
        let delayed = select_steered(
            Key::C_MAJOR,
            43,
            71,
            Some(cadence),
            [None, Some(64)],
            Gesture::Follow,
            Guidance {
                steering: Steering::Neutral,
                interpretation: None,
                ..Guidance::default()
            },
        )
        .unwrap();
        assert_eq!(delayed.realized.notes[2], 64);
        assert_eq!(delayed.voicing.notes[2], 62);
        let arrival = release_voices(Key::C_MAJOR, delayed, [None; 2], [false, true]).unwrap();
        assert_eq!(arrival.realized.notes, [43, 55, 62, 71]);

        let fourth = Voicing {
            notes: [43, 67, 72, 76],
        };
        assert!(valid_realization(Key::C_MAJOR, cadence.chord, fourth));
        let debts = voice_tendencies(
            Key::C_MAJOR,
            cadence.chord,
            fourth,
            fourth,
            Some(predominant),
            Gesture::Follow,
            [None; 2],
            HarmonicIntent::Continue,
        );
        assert_eq!(debts[1], Some(VoiceTendency { from: 72, to: 71 }));
    }

    #[test]
    fn ordinary_inversions_keep_their_context_and_fresh_octaves_establish_tonic() {
        let opening = steered(None, 36, 60, Gesture::Follow);
        assert_eq!(opening.chord, Chord::new(0, Shape::Major).unwrap());
        for melody in [64, 67] {
            let first = steered(None, 40, melody, Gesture::Follow);
            assert_eq!(first.chord, opening.chord);
        }
        let arpeggio = steered(
            Some(steered(None, 36, 64, Gesture::Follow)),
            43,
            72,
            Gesture::Follow,
        );
        assert_eq!(arpeggio.chord, opening.chord);
        assert!(arpeggio.tendencies.iter().all(Option::is_none));
        let passing = steered(
            Some(steered(None, 45, 72, Gesture::Follow)),
            43,
            72,
            Gesture::Follow,
        );
        assert_eq!(passing.chord, opening.chord);
        assert!(passing.tendencies.iter().all(Option::is_none));
    }

    #[test]
    fn prepared_tonic_pedal_covers_dominant_guides_and_resolves_either_catch_first() {
        let tonic = steered(None, 36, 67, Gesture::Follow);
        let dominant = steered(Some(tonic), 36, 67, Gesture::Tension);
        assert_eq!(tonic.realized.notes, [36, 55, 64, 67]);
        assert_eq!(dominant.chord, Chord::new(7, Shape::Dominant7).unwrap());
        assert_eq!(dominant.realized.notes, [36, 59, 65, 67]);
        assert_eq!(dominant.pedal, Some(36));
        let held = [Some(59), Some(65)];
        let pending = select_steered(
            Key::C_MAJOR,
            36,
            67,
            Some(dominant),
            held,
            Gesture::Resolve,
            Guidance {
                steering: Steering::Arrive,
                interpretation: None,
                ..Guidance::default()
            },
        )
        .unwrap();
        assert_eq!(pending.voicing.notes, [36, 60, 64, 67]);
        assert_eq!(pending.realized, dominant.realized);
        for first in 0..2 {
            let mut remaining = held;
            let mut released = [false; 2];
            remaining[first] = None;
            released[first] = true;
            let partial = release_voices(Key::C_MAJOR, pending, remaining, released).unwrap();
            assert_eq!(
                partial.realized.notes[first + 1],
                pending.voicing.notes[first + 1]
            );
            assert_eq!(
                partial.realized.notes[2 - first],
                dominant.realized.notes[2 - first]
            );
            released = [true; 2];
            released[first] = false;
            let arrival = release_voices(Key::C_MAJOR, partial, [None; 2], released).unwrap();
            assert_eq!(arrival.realized, pending.voicing);
            assert!(arrival.pedal.is_none());
        }
    }

    #[test]
    fn pedal_needs_continuous_preparation_and_never_overrides_a_bass_role() {
        let tonic = steered(None, 36, 67, Gesture::Follow);
        let g7 = Chord::new(7, Shape::Dominant7).unwrap();
        assert_eq!(
            pedal_context(Key::C_MAJOR, g7, 36, Some(tonic), Gesture::Follow, false,),
            Some(36)
        );
        let mut after_rest = tonic;
        after_rest.pedal_prepared = None;
        assert_eq!(
            pedal_context(
                Key::C_MAJOR,
                g7,
                36,
                Some(after_rest),
                Gesture::Follow,
                false,
            ),
            None
        );
        assert_eq!(
            pedal_context(Key::C_MAJOR, g7, 36, None, Gesture::Follow, false,),
            None
        );
        let role = steered(
            Some(tonic),
            36,
            67,
            Gesture::Role {
                voice: AnchorVoice::Bass,
                role: ToneRole::Root,
            },
        );
        assert_eq!(role.chord.role(36), Some(ToneRole::Root));
        assert!(role.pedal.is_none());
        assert_eq!(
            pedal_context(
                Key::C_MAJOR,
                Chord::new(11, Shape::Diminished7).unwrap(),
                36,
                Some(tonic),
                Gesture::Follow,
                false,
            ),
            None,
        );
    }

    #[test]
    fn directional_bass_figures_choose_inversions_with_a_legal_next_step() {
        let rising = [(36, 67), (38, 67), (40, 67)];
        let falling = [(40, 67), (38, 65), (36, 64)];
        for (phrase, expected) in [
            (
                rising,
                [[36, 55, 64, 67], [38, 55, 59, 67], [40, 55, 60, 67]],
            ),
            (
                falling,
                [[40, 55, 60, 67], [38, 55, 59, 65], [36, 55, 60, 64]],
            ),
        ] {
            let mut previous = None;
            for ((bass, melody), notes) in phrase.into_iter().zip(expected) {
                let next = steered(previous, bass, melody, Gesture::Follow);
                assert_eq!(next.realized.notes, notes);
                if let Some(old) = previous {
                    assert!(!forbidden_parallels(old.realized, next.realized, true));
                }
                previous = Some(next);
            }
            let last = previous.unwrap();
            assert_eq!(
                last.bass_history,
                [Some(phrase[2].0), Some(phrase[1].0), Some(phrase[0].0)]
            );
            let repeated = steered(Some(last), phrase[2].0, phrase[2].1, Gesture::Follow);
            assert_eq!(repeated.bass_history, last.bass_history);
        }
    }

    #[test]
    fn inner_passing_line_advances_by_melody_and_other_catch_release_preserves_it() {
        let initial = steered(None, 36, 67, Gesture::Follow);
        let passing = steered(Some(initial), 36, 72, Gesture::Follow);
        assert_eq!(passing.realized.notes, [36, 53, 64, 72]);
        assert_eq!(passing.voicing.notes, [36, 52, 64, 72]);
        assert_eq!(
            passing.inner_ornaments,
            [
                Some(InnerOrnament {
                    from: 53,
                    to: 52,
                    bank: AnchorVoice::Melody
                }),
                None
            ]
        );
        let unchanged = steered(Some(passing), 36, 72, Gesture::Follow);
        assert_eq!(unchanged.inner_ornaments, passing.inner_ornaments);
        assert_eq!(unchanged.realized, passing.realized);
        let other_release =
            release_voices(Key::C_MAJOR, passing, [None; 2], [false, true]).unwrap();
        assert_eq!(other_release.realized.notes[1], 53);
        assert_eq!(other_release.inner_ornaments, passing.inner_ornaments);
        let caught = select_steered(
            Key::C_MAJOR,
            36,
            67,
            Some(passing),
            [Some(53), None],
            Gesture::Follow,
            Guidance {
                steering: Steering::Neutral,
                interpretation: None,
                ..Guidance::default()
            },
        )
        .unwrap();
        assert_eq!(caught.realized.notes, [36, 53, 64, 67]);
        let released = release_voices(Key::C_MAJOR, caught, [None; 2], [true, false]).unwrap();
        assert_eq!(released.realized.notes, [36, 52, 64, 67]);
        assert!(released.inner_ornaments.iter().all(Option::is_none));
        let ordinary = steered(Some(passing), 36, 67, Gesture::Follow);
        assert_eq!(ordinary.realized, released.realized);
        assert_eq!(ordinary.inner_history[0], [Some(52), Some(53), Some(55)]);
    }

    #[test]
    fn prepared_inner_neighbour_keeps_foundation_and_its_checked_endpoint() {
        let initial = steered(None, 36, 67, Gesture::Follow);
        let mut structural = initial;
        structural.voicing.notes[3] = 72;
        structural.realized = structural.voicing;
        let decorated = decorate_inner(
            Key::C_MAJOR,
            structural,
            Some(initial),
            [None; 2],
            Gesture::Follow,
        );
        assert_eq!(decorated.realized.notes, [36, 53, 64, 72]);
        assert_eq!(
            decorated.inner_ornaments[0],
            Some(InnerOrnament {
                from: 53,
                to: 55,
                bank: AnchorVoice::Melody
            })
        );
        assert!(decorated.inner_ornaments[1].is_none());
        assert!(!forbidden_parallels(
            initial.realized,
            decorated.realized,
            true
        ));
        assert!(!forbidden_parallels(
            decorated.realized,
            decorated.voicing,
            false
        ));
    }

    #[test]
    fn functional_pull_has_two_real_guides_and_releasing_steering_keeps_their_debts() {
        let initial = steered(None, 36, 64, Gesture::Follow);
        let dominant = steered(Some(initial), 36, 64, Gesture::Tension);
        assert_eq!(
            dominant.chord,
            Chord::new(4, Shape::DominantFlat13).unwrap()
        );
        assert_eq!(dominant.realized.notes, [36, 56, 62, 64]);
        assert_eq!(
            dominant.tendencies,
            [
                Some(VoiceTendency { from: 56, to: 57 }),
                Some(VoiceTendency { from: 62, to: 60 })
            ]
        );
        let neutral = with_steering(dominant, Steering::Neutral);
        assert_eq!(neutral.guidance, HarmonicIntent::Continue);
        assert_eq!(neutral.tendencies, dominant.tendencies);
        assert_eq!(neutral.tendency, dominant.tendency);
        assert_eq!(neutral.realized, dominant.realized);
        let arrival = steered(Some(neutral), 45, 64, Gesture::Follow);
        assert_eq!(arrival.realized.notes, [45, 57, 60, 64]);
        let coloured = steered(None, 36, 71, Gesture::Follow);
        let settled = steered(Some(coloured), 36, 71, Gesture::Resolve);
        assert_eq!(settled.chord, Chord::new(0, Shape::Major7).unwrap());
        assert_eq!(settled.realized.notes[3], 71);
    }

    #[test]
    fn ordinary_playing_connects_predominant_dominant_and_tonic() {
        let tonic = follow(None, 36, 64);
        let predominant = follow(Some(tonic), 38, 65);
        let dominant = follow(Some(predominant), 43, 65);
        let arrival = follow(Some(dominant), 36, 64);
        assert_eq!(predominant.chord, Chord::new(2, Shape::Minor).unwrap());
        assert_eq!(dominant.chord, Chord::new(7, Shape::Dominant7).unwrap());
        assert_eq!(arrival.chord, Chord::new(0, Shape::Major).unwrap());
        for (old, next) in [
            (tonic, predominant),
            (predominant, dominant),
            (dominant, arrival),
        ] {
            assert!(!forbidden_parallels(old.realized, next.realized, true));
            assert!(valid_realization(Key::C_MAJOR, next.chord, next.voicing));
        }
    }

    #[test]
    fn ordinary_jazz_arrival_preserves_a_real_seventh_guide_and_played_major_seventh() {
        let tonic = follow(None, 36, 64);
        let predominant = follow(Some(tonic), 38, 72);
        let dominant = follow(Some(predominant), 43, 71);
        let arrival = follow(Some(dominant), 36, 71);
        assert_eq!(predominant.chord, Chord::new(2, Shape::Minor7).unwrap());
        assert_eq!(dominant.chord, Chord::new(7, Shape::Dominant7).unwrap());
        assert_eq!(arrival.chord, Chord::new(0, Shape::Major7).unwrap());
        assert!((1..3).any(|voice| {
            let note = predominant.realized.notes[voice];
            note % 12 == 5
                && dominant.realized.notes[voice] == note
                && arrival.realized.notes[voice] == note - 1
        }));
        assert_eq!(arrival.realized.notes[3], 71);
    }

    #[test]
    fn prepared_passing_neighbour_and_suspended_melody_keep_literal_notes() {
        let tonic = follow(None, 36, 64);
        let passing = follow(Some(tonic), 36, 65);
        assert_eq!(passing.chord, tonic.chord);
        assert!(!passing.ornament.unwrap().suspension);
        assert_eq!(passing.realized.notes[3], 65);
        let through = follow(Some(passing), 36, 67);
        let neighbour = follow(Some(passing), 36, 64);
        assert_eq!(through.chord, tonic.chord);
        assert_eq!(neighbour.chord, tonic.chord);
        assert!(through.ornament.is_none() && neighbour.ornament.is_none());
        let predominant = follow(Some(tonic), 38, 65);
        let dominant = follow(Some(predominant), 43, 65);
        let suspended = follow(Some(dominant), 36, 65);
        assert_eq!(suspended.chord, tonic.chord);
        assert!(suspended.ornament.unwrap().suspension);
        let arrival = follow(Some(suspended), 36, 64);
        assert_eq!(arrival.chord, tonic.chord);
        assert!(arrival.ornament.is_none());
        let changed_bass = follow(Some(passing), 38, 65);
        assert!(changed_bass.ornament.is_none());
    }

    #[test]
    fn prepared_melody_neighbours_leave_settled_lower_voices_in_place() {
        let mut previous = steered(None, 36, 64, Gesture::Follow);
        let foundation = previous.realized.notes[..3].to_vec();
        for melody in [65, 64, 62, 64] {
            let next = steered(Some(previous), 36, melody, Gesture::Follow);
            assert_eq!(next.chord, Chord::new(0, Shape::Major).unwrap());
            assert_eq!(next.realized.notes[..3], foundation);
            assert_eq!(next.realized.notes[3], melody);
            assert_eq!(next.ornament.is_some(), melody != 64);
            assert!(next.tendencies.iter().all(Option::is_none));
            previous = next;
        }
        let low_landing = steered(Some(previous), 36, 60, Gesture::Follow);
        assert_ne!(low_landing.realized.notes[..3], foundation);
        assert!(low_landing.ornament.is_none());
        assert!(valid_realization(
            Key::C_MAJOR,
            low_landing.chord,
            low_landing.realized
        ));

        let dominant = steered(None, 43, 71, Gesture::Follow);
        let passing = steered(Some(dominant), 43, 72, Gesture::Follow);
        assert!(passing.realized.notes[..3]
            .iter()
            .any(|note| *note % 12 == 11));
        assert_ne!(passing.realized.notes[..3], dominant.realized.notes[..3]);
    }

    #[test]
    fn a_held_melody_ornament_does_not_block_commands_or_freeze_reharmony() {
        let home = steered(None, 36, 64, Gesture::Follow);
        let passing = steered(Some(home), 36, 65, Gesture::Follow);
        let substitute = guided(
            Some(passing),
            36,
            65,
            Gesture::Substitute,
            Guidance {
                steering: Steering::Substitute,
                ..Guidance::default()
            },
        );
        assert_ne!(substitute.realized, passing.realized);
        assert!(compatible_substitute(
            Key::C_MAJOR,
            passing,
            substitute.chord
        ));
        assert_eq!(substitute.realized.notes[3], 65);
        assert!(substitute.ornament.is_some());
        let present = substitute
            .realized
            .notes
            .into_iter()
            .fold(0u16, |bits, note| bits | (1 << (note % 12)));
        assert_eq!(substitute.chord.essential().bits() & !present, 0);
        let settled = guided(
            Some(passing),
            36,
            65,
            Gesture::Resolve,
            Guidance {
                steering: Steering::Arrive,
                ..Guidance::default()
            },
        );
        assert_eq!(settled.chord, home.chord);
        assert_eq!(settled.realized.notes[3], 65);
        assert_eq!(settled.ornament, passing.ornament);
        assert!(settled.tendency.is_none());
        assert!(settled.tendencies.iter().all(Option::is_none));
    }

    #[test]
    fn held_melody_tensions_need_preparation_and_a_local_chord_tone_target() {
        let home = steered(None, 36, 64, Gesture::Follow);
        for chord in [
            Chord::new(2, Shape::Minor).unwrap(),
            Chord::new(7, Shape::Dominant7).unwrap(),
        ] {
            let ornament = melody_ornament(Key::C_MAJOR, chord, 36, 64, Some(home)).unwrap();
            assert_eq!(ornament.origin, 62);
            assert_eq!(ornament.direction, -1);
            assert!(ornament.suspension);
            assert!(chord.tones().contains(ornament.origin));
        }
        let passing = steered(Some(home), 36, 65, Gesture::Follow);
        let seventh = Chord::new(0, Shape::Major7).unwrap();
        let fourth = melody_ornament(Key::C_MAJOR, seventh, 36, 65, Some(passing)).unwrap();
        assert_eq!(fourth.origin, 64);
        assert_eq!(fourth.direction, -1);
        assert!(fourth.suspension);
        assert!(melody_ornament(Key::C_MAJOR, seventh, 36, 65, None).is_none());
        assert!(melody_ornament(Key::C_MAJOR, seventh, 36, 65, Some(home)).is_none());
        assert!(melody_ornament(Key::C_MAJOR, seventh, 36, 127, Some(home)).is_none());
        let low_melody = steered(None, 47, 60, Gesture::Follow);
        assert!(melody_ornament(
            Key::C_MAJOR,
            Chord::new(7, Shape::Major7).unwrap(),
            47,
            60,
            Some(low_melody)
        )
        .is_none());
        assert!(melody_ornament(
            Key::C_MAJOR,
            Chord::new(0, Shape::Minor).unwrap(),
            36,
            64,
            Some(home)
        )
        .is_none());
    }

    #[test]
    fn applied_tendencies_survive_an_unavailable_intermediate_and_both_release_orders() {
        let dominant = applied_dominant();
        let held = [Some(56), Some(62)];
        let intermediate =
            select(Key::C_MAJOR, 38, 64, Some(dominant), held, Gesture::Follow).unwrap();
        assert_eq!(intermediate.tendency, dominant.tendency);
        assert_eq!(intermediate.tendencies, dominant.tendencies);
        assert_eq!(&intermediate.realized.notes[1..3], &[56, 62]);
        let pending = select(
            Key::C_MAJOR,
            45,
            64,
            Some(intermediate),
            held,
            Gesture::Follow,
        )
        .unwrap();
        assert_eq!(pending.chord, Chord::new(9, Shape::Minor).unwrap());
        assert_eq!(pending.voicing.notes, [45, 57, 60, 64]);
        for first in 0..2 {
            let mut remaining = held;
            remaining[first] = None;
            let partial = release(Key::C_MAJOR, pending, remaining).unwrap();
            assert_eq!(
                partial.realized.notes[first + 1],
                pending.voicing.notes[first + 1]
            );
            assert_eq!(
                partial.realized.notes[2 - first],
                pending.realized.notes[2 - first]
            );
            let arrival = release(Key::C_MAJOR, partial, [None; 2]).unwrap();
            assert_eq!(arrival.realized, pending.voicing);
            assert_eq!(arrival.guidance, HarmonicIntent::Continue);
        }
    }

    #[test]
    fn generated_sevenths_are_unique_but_actual_catches_are_player_forced() {
        let chord = Chord::new(7, Shape::Dominant7).unwrap();
        let duplicate = Voicing {
            notes: [43, 53, 65, 71],
        };
        assert!(!valid_realization(Key::C_MAJOR, chord, duplicate));
        assert!(!valid_caught_realization(
            Key::C_MAJOR,
            chord,
            duplicate,
            [Some(53), None]
        ));
        assert!(valid_caught_realization(
            Key::C_MAJOR,
            chord,
            duplicate,
            [Some(53), Some(65)]
        ));
        let borrowed = Chord::new(2, Shape::HalfDiminished7).unwrap();
        assert_eq!(
            tonal_function(Key::C_MAJOR, borrowed),
            Some(HarmonicFunction::Predominant)
        );
        assert_eq!(abstract_destination(Key::C_MAJOR, borrowed), Some(chord));
        assert_eq!(chord_leading(Key::C_MAJOR, borrowed), None);
        assert_eq!(
            abstract_destination(Key::C_MAJOR, Chord::new(2, Shape::Diminished).unwrap()),
            Chord::new(7, Shape::Major)
        );
    }

    #[test]
    fn thumb_direction_guides_subsequent_played_notes() {
        let initial = follow(None, 36, 64);
        let relaxed = choose(Some(initial), Gesture::Relax);
        let leaning = choose(Some(initial), Gesture::Tension);
        let relaxed_next = follow(Some(relaxed), 38, 65);
        let leaning_next = follow(Some(leaning), 38, 65);
        assert_eq!(relaxed_next.chord, Chord::new(2, Shape::Minor).unwrap());
        assert_ne!(leaning_next.realized, relaxed_next.realized);
        assert_eq!(leaning_next.guidance, HarmonicIntent::Lean);
    }

    #[test]
    fn colours_and_applied_resolution_keep_both_real_keyboard_notes() {
        let initial = choose(None, Gesture::Follow);
        assert_eq!(
            initial.chord,
            Chord {
                root: 0,
                shape: Shape::Major
            }
        );
        let coloured = choose(Some(initial), Gesture::Tension);
        assert_eq!(coloured.tension, 1);
        assert_ne!(initial.realized, coloured.realized);
        let dominant = applied_dominant();
        assert_eq!(
            dominant.chord,
            Chord {
                root: 4,
                shape: Shape::DominantFlat13
            }
        );
        assert_eq!(dominant.realized.notes, [36, 56, 62, 64]);
        let arrival = choose(Some(dominant), Gesture::Resolve);
        assert_eq!(Some(arrival.chord), dominant.resolution);
        assert_eq!(
            arrival.chord,
            Chord {
                root: 9,
                shape: Shape::Minor
            }
        );
        assert_eq!(arrival.realized.notes, [36, 57, 60, 64]);
    }

    #[test]
    fn role_requests_reinterpret_literal_notes_without_moving_them() {
        let initial = choose(None, Gesture::Follow);
        for (voice, role) in [
            (AnchorVoice::Bass, ToneRole::Third),
            (AnchorVoice::Bass, ToneRole::Fifth),
            (AnchorVoice::Melody, ToneRole::Root),
            (AnchorVoice::Melody, ToneRole::Seventh),
        ] {
            let next = choose(Some(initial), Gesture::Role { voice, role });
            assert_eq!([next.realized.notes[0], next.realized.notes[3]], [36, 64]);
            let note = if voice == AnchorVoice::Bass { 36 } else { 64 };
            assert_eq!(next.chord.role(note), Some(role));
        }
        assert!(select(
            Key::C_MAJOR,
            36,
            61,
            None,
            [None; 2],
            Gesture::Role {
                voice: AnchorVoice::Bass,
                role: ToneRole::Root,
            }
        )
        .is_none());
    }

    #[test]
    fn both_chromatic_catches_survive_resolution_and_each_release_order() {
        let dominant = applied_dominant();
        let held = [Some(56), Some(62)];
        let pending = select(Key::C_MAJOR, 36, 64, Some(dominant), held, Gesture::Resolve).unwrap();
        assert_eq!(pending.realized.notes, [36, 56, 62, 64]);
        assert_eq!(pending.voicing.notes, [36, 57, 60, 64]);
        assert_eq!(pending.suspended, [true, true]);
        for first in 0..2 {
            let mut remaining = held;
            remaining[first] = None;
            let partial = release(Key::C_MAJOR, pending, remaining).unwrap();
            assert_eq!(partial.chord, pending.chord);
            assert_eq!(
                partial.realized.notes[first + 1],
                pending.voicing.notes[first + 1]
            );
            assert_eq!(
                partial.realized.notes[2 - first],
                pending.realized.notes[2 - first]
            );
            let resolved = release(Key::C_MAJOR, partial, [None; 2]).unwrap();
            assert_eq!(resolved.realized, pending.voicing);
        }
    }

    #[test]
    fn bad_hold_proof_and_midi_boundaries_never_relax_constraints() {
        assert!(Chord::new(12, Shape::Major).is_none());
        let unchecked = Chord {
            root: 255,
            shape: Shape::DominantFlat13,
        };
        assert_eq!(
            unchecked.tones(),
            Chord::new(3, Shape::DominantFlat13).unwrap().tones()
        );
        assert!(select(
            Key::C_MAJOR,
            36,
            64,
            None,
            [Some(55), None],
            Gesture::Reharm
        )
        .is_none());
        let initial = choose(None, Gesture::Follow);
        assert!(select(
            Key::C_MAJOR,
            36,
            64,
            Some(initial),
            [Some(54), None],
            Gesture::Reharm
        )
        .is_none());
        for (bass, melody) in [(23, 64), (49, 64), (36, 59), (36, 89), (255, 64), (36, 128)] {
            assert!(select(Key::C_MAJOR, bass, melody, None, [None; 2], Gesture::Follow).is_none());
        }
    }

    #[test]
    fn all_physical_anchor_pairs_keep_coverage_geometry_and_reachable_cached_targets() {
        for bass in [36, 38, 40, 41, 43, 45, 47, 48] {
            for melody in [60, 62, 64, 65, 67, 69, 71, 72, 74, 76] {
                let mut previous = None;
                for gesture in [
                    Gesture::Follow,
                    Gesture::Reharm,
                    Gesture::Tension,
                    Gesture::Tension,
                    Gesture::Tension,
                    Gesture::Resolve,
                    Gesture::Relax,
                ] {
                    let Some(next) =
                        select(Key::C_MAJOR, bass, melody, previous, [None; 2], gesture)
                    else {
                        continue;
                    };
                    assert_eq!(
                        [next.voicing.notes[0], next.voicing.notes[3]],
                        [bass, melody]
                    );
                    assert!(valid_realization(Key::C_MAJOR, next.chord, next.voicing));
                    let present = next
                        .voicing
                        .notes
                        .into_iter()
                        .fold(0u16, |bits, note| bits | (1 << (note % 12)));
                    assert_eq!(next.chord.essential().bits() & !present, 0);
                    if let Some(old) = previous {
                        assert!(!forbidden_parallels(old.realized, next.realized, true));
                    }
                    if let Some(target) = next.resolution {
                        let arrival = select(
                            Key::C_MAJOR,
                            bass,
                            melody,
                            Some(next),
                            [None; 2],
                            Gesture::Resolve,
                        )
                        .unwrap();
                        assert_eq!(arrival.chord, target);
                    }
                    previous = Some(next);
                }
            }
        }
    }

    #[test]
    fn diminished_fifths_and_colour_tones_are_essential() {
        for shape in [
            Shape::Diminished,
            Shape::HalfDiminished7,
            Shape::Diminished7,
            Shape::Augmented,
        ] {
            let chord = Chord { root: 0, shape };
            assert!(chord.essential().contains(chord.fifth()));
        }
        for shape in [
            Shape::Major6,
            Shape::Add9,
            Shape::Dominant9,
            Shape::DominantFlat13,
        ] {
            let chord = Chord { root: 0, shape };
            assert!(chord.essential().contains(chord.colour().unwrap()));
        }
    }

    #[test]
    fn repeated_reharm_explores_substitutions_voicings_and_colours() {
        extern crate std;
        for (bass, melody) in [(36, 64), (36, 72), (43, 71)] {
            let mut previous =
                select(Key::C_MAJOR, bass, melody, None, [None; 2], Gesture::Follow).unwrap();
            let mut chords = std::vec::Vec::new();
            let mut sounds = std::vec::Vec::new();
            for _ in 0..12 {
                let next = select(
                    Key::C_MAJOR,
                    bass,
                    melody,
                    Some(previous),
                    [None; 2],
                    Gesture::Reharm,
                )
                .unwrap();
                assert_ne!(next.realized, previous.realized);
                if !chords.contains(&next.chord) {
                    chords.push(next.chord);
                }
                if !sounds.contains(&next.realized) {
                    sounds.push(next.realized);
                }
                previous = next;
            }
            assert!(chords.len() >= 3, "{bass}/{melody}: {chords:?}");
            assert!(sounds.len() >= 4, "{bass}/{melody}: {sounds:?}");
        }
    }

    #[test]
    fn late_reharm_windows_do_not_collapse_into_the_previous_short_cycles() {
        extern crate std;
        for (bass, melody) in [(43, 60), (36, 62), (41, 62), (36, 64), (36, 72)] {
            let mut previous =
                select(Key::C_MAJOR, bass, melody, None, [None; 2], Gesture::Follow).unwrap();
            let mut window = std::vec::Vec::new();
            for step in 0..512 {
                let next = select(
                    Key::C_MAJOR,
                    bass,
                    melody,
                    Some(previous),
                    [None; 2],
                    Gesture::Reharm,
                )
                .unwrap();
                assert_ne!(next.realized, previous.realized);
                assert_eq!(
                    [next.realized.notes[0], next.realized.notes[3]],
                    [bass, melody]
                );
                assert!(valid_realization(Key::C_MAJOR, next.chord, next.realized));
                assert!(!forbidden_parallels(previous.realized, next.realized, true));
                if step % 128 >= 64 && !window.contains(&next.realized) {
                    window.push(next.realized);
                }
                if step % 128 == 127 {
                    assert!(
                        window.len() >= 8,
                        "{bass}/{melody}, late window{step}: {window:?}"
                    );
                    window.clear();
                }
                if step % 16 == 0 {
                    if let Some(target) = next.resolution {
                        let arrival = select(
                            Key::C_MAJOR,
                            bass,
                            melody,
                            Some(next),
                            [None; 2],
                            Gesture::Resolve,
                        )
                        .unwrap();
                        assert_eq!(arrival.chord, target);
                    }
                }
                previous = next;
            }
        }
    }

    #[test]
    fn variation_counter_wraps_even_when_both_catches_prevent_an_audible_change() {
        let mut previous = applied_dominant();
        previous.variation = u32::MAX;
        let held = [
            Some(previous.realized.notes[1]),
            Some(previous.realized.notes[2]),
        ];
        let next = select(Key::C_MAJOR, 36, 64, Some(previous), held, Gesture::Reharm).unwrap();
        assert_eq!(next.variation, 0);
        assert_eq!(next.realized, previous.realized);
        assert_eq!(next.recent_realizations, previous.recent_realizations);
        if let Some(target) = next.resolution {
            let arrival = select(Key::C_MAJOR, 36, 64, Some(next), held, Gesture::Resolve).unwrap();
            assert_eq!(arrival.chord, target);
        }
    }
}
