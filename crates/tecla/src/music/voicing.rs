//! Bounded four-part harmonization of a played bass and melody.
//!
//! The search considers fourteen diatonic triads/sevenths and at most 20 × 22
//! inner-note pairs per chord. [`solve`] and [`nudge`] require complete chords
//! and reject every parallel perfect interval or doubled leading tone.
//! [`harmonize`] adds a major-key phrase policy: a perfect fifth may be omitted,
//! and conflicts forced by the two fixed anchors are reported separately.
//! Generated voice pairs, registers, voice order and upper spacing remain hard
//! constraints. Six-four contexts are ranked, including explicit inversion as
//! arpeggiated bass; this is a bounded vocabulary, not all Baroque counterpoint.

use super::theory::{
    Chord, ChordKind, ChordQuality, Degree, HarmonicFunction, Key, Mode, PitchClassSet, ToneRole,
};

/// Bass, tenor, alto, soprano in ascending MIDI-note order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voicing {
    /// MIDI pitches ordered bass, tenor, alto, soprano.
    pub notes: [u8; 4],
}

/// Register direction requested by an instrument's movement controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Motion {
    /// Preserve common tones and minimize inner-voice movement.
    #[default]
    Smooth,
    /// Favor lower inner voices and avoid upward movement.
    Lower,
    /// Favor higher inner voices and avoid downward movement.
    Higher,
}

/// Concrete movement and spacing controls for this four-voice instrument.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VoicingPreference {
    /// Preferred register direction for the generated inner voices.
    pub motion: Motion,
    /// Favor a separation near an octave; false favors compact revoicing.
    pub open: bool,
}

/// A phrase command for the supported diatonic major-key harmony vocabulary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HarmonicIntent {
    /// Prolong the current function or continue a descending-fifths sequence.
    #[default]
    Continue,
    /// Approach a dominant, then sustain its directed pull toward an arrival.
    Lean,
    /// Prefer a tonic arrival and resolve a preceding dominant's inner voices.
    Arrive,
}

/// Constraints on chord identity, independent of its bass inversion or voicing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HarmonyPolicy {
    /// The requested direction of the phrase.
    pub intent: HarmonicIntent,
    /// Require this exact diatonic chord, for an explicit inversion request.
    pub required: Option<Chord>,
    /// Exclude this chord, for an actual reharmonization rather than revoicing.
    pub exclude: Option<Chord>,
    /// Prefer another degree, then another chord kind, then an audible revoicing.
    /// A legal current harmony remains available when variation is blocked.
    pub variation: bool,
    /// Softly avoid immediately returning to this preceding phrase degree.
    pub avoid_degree: Option<Degree>,
    /// Prefer the next degree in a descending-fifths sequence when compatible.
    pub sequence: bool,
}

/// The resolved harmony and any inner voices that begin with a prepared hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Solution {
    /// Resolved pitches, including the targets of prepared suspensions.
    pub voicing: Voicing,
    /// Pitches initially sounded: requested holds replace their target notes.
    /// The instrument updates this after a timed or manual resolution before
    /// passing the solution back as the previously sounded harmony.
    pub realized: Voicing,
    /// Diatonic chord identity; instrument realizations may omit its fifth.
    pub chord: Chord,
    /// Whether tenor and alto respectively begin with a dissonant suspension.
    /// A held common chord tone is false because it needs no resolution.
    pub suspended: [bool; 2],
    /// The player's fixed outer notes force parallel fifths/octaves or a doubled
    /// leading tone. Generated voice pairs still obey those constraints.
    pub anchor_conflict: bool,
    /// Lower is preferred; ties retain the first candidate deterministically.
    pub score: u32,
}

/// Finds a complete harmony without allocating or relaxing hard constraints.
///
/// `previous` describes the previously sounded harmony. Each requested `held`
/// pitch must be its corresponding previous inner voice. Common chord tones
/// remain fixed; nonchord tones need preparation in the previous chord or an
/// already prepared suspension. A request is mandatory: impossible holds return `None`, allowing
/// the instrument to make an explicit decision to retry without that request.
/// `voicing` in the result contains the resolution targets, not the held pitches.
pub fn solve(
    key: Key,
    bass: u8,
    melody: u8,
    previous: Option<Solution>,
    held: [Option<u8>; 2],
    preference: VoicingPreference,
) -> Option<Solution> {
    search(
        key,
        bass,
        melody,
        previous,
        held,
        SearchPolicy::Strict {
            preference,
            step: Motion::Smooth,
        },
    )
}

/// Moves at least one inner voice in `direction`, without moving either inner
/// voice the other way. Anchors, caught notes, and all harmonic rules still win.
/// Returns `None` at the legal register boundary; callers can keep sounding the
/// current harmony. Repeated calls choose nearby legal steps, rather than
/// jumping straight to the requested end of the register.
pub fn nudge(
    key: Key,
    bass: u8,
    melody: u8,
    previous: Solution,
    held: [Option<u8>; 2],
    preference: VoicingPreference,
    direction: Motion,
) -> Option<Solution> {
    search(
        key,
        bass,
        melody,
        Some(previous),
        held,
        SearchPolicy::Strict {
            preference,
            step: direction,
        },
    )
}

/// Harmonizes fixed anchors with a small diatonic major-key phrase policy.
///
/// Unlike [`solve`], it may omit a perfect fifth while keeping root, third and any
/// seventh. A retained chordal seventh may continue into a downward suspension.
/// It reports conflicts forced by the two outer notes instead of
/// rejecting all inner voices. Crossing, spacing and generated-voice parallels
/// remain hard constraints. An explicit dominant-to-I arrival requires inner
/// leading tones to rise and chord sevenths to descend; prepared holds retain
/// those resolution targets. Natural minor and chromatic grammar are excluded.
pub fn harmonize(
    key: Key,
    bass: u8,
    melody: u8,
    previous: Option<Solution>,
    held: [Option<u8>; 2],
    policy: HarmonyPolicy,
) -> Option<Solution> {
    if key.mode() != Mode::Major {
        return None;
    }
    search(
        key,
        bass,
        melody,
        previous,
        held,
        SearchPolicy::Harmony(policy),
    )
}

#[derive(Clone, Copy)]
enum SearchPolicy {
    Strict {
        preference: VoicingPreference,
        step: Motion,
    },
    Harmony(HarmonyPolicy),
}

/// Named chord-coverage policies: the instrument may omit only the fifth.
#[derive(Clone, Copy)]
enum FifthOmission {
    Forbid,
    Penalize,
}

fn search(
    key: Key,
    bass: u8,
    melody: u8,
    previous: Option<Solution>,
    held: [Option<u8>; 2],
    policy: SearchPolicy,
) -> Option<Solution> {
    let (preference, step, harmony, omission) = match policy {
        SearchPolicy::Strict { preference, step } => {
            (preference, step, None, FifthOmission::Forbid)
        }
        SearchPolicy::Harmony(harmony) => (
            VoicingPreference::default(),
            Motion::Smooth,
            Some(harmony),
            FifthOmission::Penalize,
        ),
    };
    if !(24..=48).contains(&bass)
        || !(60..=88).contains(&melody)
        || !key.contains(bass)
        || !key.contains(melody)
    {
        return None;
    }
    if held.iter().any(Option::is_some) && previous.is_none() {
        return None;
    }

    let mut best: Option<Solution> = None;
    for degree in Degree::ALL {
        for kind in [ChordKind::Triad, ChordKind::Seventh] {
            let chord = key.chord(degree, kind);
            if harmony.is_some_and(|policy| {
                policy.required.is_some_and(|required| required != chord)
                    || policy.exclude == Some(chord)
            }) {
                continue;
            }
            if !chord.tones.contains(bass) || !chord.tones.contains(melody) {
                continue;
            }
            for tenor in 48..=67 {
                if !chord.tones.contains(tenor) {
                    continue;
                }
                for alto in 55..=76 {
                    if !chord.tones.contains(alto) {
                        continue;
                    }
                    let voicing = Voicing {
                        notes: [bass, tenor, alto, melody],
                    };
                    let Some(omitted_fifth) = chord_coverage(chord, voicing, omission) else {
                        continue;
                    };
                    if !valid_realization(key, voicing, harmony.is_some()) {
                        continue;
                    }
                    let Some((realized, suspended)) =
                        realize_holds(key, chord, voicing, previous, held, harmony.is_some())
                    else {
                        continue;
                    };
                    if let Some(old) = previous {
                        let before = &old.realized.notes[1..3];
                        let after = &realized.notes[1..3];
                        let direction_ok = match step {
                            Motion::Smooth => true,
                            Motion::Lower => {
                                after != before
                                    && after.iter().zip(before).all(|(to, from)| to <= from)
                            }
                            Motion::Higher => {
                                after != before
                                    && after.iter().zip(before).all(|(to, from)| to >= from)
                            }
                        };
                        if !direction_ok
                            || forbidden_parallels(old.realized, realized, harmony.is_some())
                            || forbidden_parallels(old.realized, voicing, harmony.is_some())
                        {
                            continue;
                        }
                    }
                    if parallel_perfects(realized, voicing) {
                        continue;
                    }
                    if harmony.is_some_and(|policy| {
                        !cadence_targets(key, chord, voicing, previous, policy.intent)
                    }) {
                        continue;
                    }
                    let anchor_conflict = harmony.is_some()
                        && (outer_leading_tones(key, voicing)
                            || previous
                                .is_some_and(|old| parallel_pair(old.realized, voicing, 0, 3)));
                    let mut score = rank(
                        key,
                        chord,
                        voicing,
                        previous,
                        VoicingPreference {
                            motion: if step == Motion::Smooth {
                                preference.motion
                            } else {
                                Motion::Smooth
                            },
                            ..preference
                        },
                        harmony.is_none_or(|policy| {
                            policy.intent == HarmonicIntent::Continue && !policy.variation
                        }),
                    );
                    if let Some(policy) = harmony {
                        score += phrase_cost(key, chord, voicing, previous, policy);
                        if policy.variation {
                            if let Some(old) = previous {
                                if chord.degree == old.chord.degree {
                                    score += 4096;
                                }
                                if chord == old.chord {
                                    score += 4096;
                                }
                                if realized == old.realized {
                                    score += 4096;
                                }
                            }
                        }
                        if policy.avoid_degree == Some(chord.degree) {
                            score += 512;
                        }
                        if omitted_fifth {
                            score += 48;
                        }
                        if anchor_conflict {
                            score += 512;
                        }
                    }
                    if best.is_none_or(|old| score < old.score) {
                        best = Some(Solution {
                            voicing,
                            realized,
                            chord,
                            suspended,
                            anchor_conflict,
                            score,
                        });
                    }
                }
            }
        }
    }
    best
}

fn valid_spacing(voicing: Voicing) -> bool {
    let [bass, tenor, alto, soprano] = voicing.notes;
    bass < tenor
        && tenor < alto
        && alto < soprano
        && alto - tenor <= 12
        && soprano - alto <= 12
        && (48..=67).contains(&tenor)
        && (55..=76).contains(&alto)
}

fn complete_chord(chord: Chord, voicing: Voicing) -> bool {
    let mut present = PitchClassSet::default();
    for note in voicing.notes {
        present.insert(note);
    }
    present == chord.tones
}

fn chord_coverage(chord: Chord, voicing: Voicing, omission: FifthOmission) -> Option<bool> {
    if complete_chord(chord, voicing) {
        return Some(false);
    }
    if matches!(omission, FifthOmission::Forbid) || chord.quality() == ChordQuality::Diminished {
        return None;
    }
    let mut expected = chord.tones;
    expected.remove(chord.fifth);
    let mut present = PitchClassSet::default();
    for note in voicing.notes {
        present.insert(note);
    }
    if present == expected {
        Some(true)
    } else {
        None
    }
}

fn outer_leading_tones(key: Key, voicing: Voicing) -> bool {
    key.leading_tone()
        .is_some_and(|pitch| voicing.notes[0] % 12 == pitch && voicing.notes[3] % 12 == pitch)
}

/// Checks actual sounding geometry and leading-tone doubling after a partial
/// suspension release. Only a duplicate forced by both outer anchors is allowed.
pub(crate) fn valid_realization(key: Key, voicing: Voicing, allow_anchor_conflict: bool) -> bool {
    valid_spacing(voicing)
        && (!doubled_leading_tone(key, voicing)
            || (allow_anchor_conflict
                && outer_leading_tones(key, voicing)
                && !voicing.notes[1..3]
                    .iter()
                    .any(|note| key.leading_tone() == Some(note % 12))))
}

fn doubled_leading_tone(key: Key, voicing: Voicing) -> bool {
    key.leading_tone().is_some_and(|pitch| {
        voicing
            .notes
            .into_iter()
            .filter(|note| note % 12 == pitch)
            .count()
            > 1
    })
}

fn realize_holds(
    key: Key,
    chord: Chord,
    target: Voicing,
    previous: Option<Solution>,
    held: [Option<u8>; 2],
    instrument_harmony: bool,
) -> Option<(Voicing, [bool; 2])> {
    let mut realized = target;
    let mut suspended = [false; 2];
    for (index, pitch) in held.into_iter().enumerate() {
        let Some(pitch) = pitch else {
            continue;
        };
        let old = previous?;
        let voice = index + 1;
        if old.realized.notes[voice] != pitch {
            return None;
        }
        if chord.tones.contains(pitch) {
            if target.notes[voice] != pitch {
                return None;
            }
            continue;
        }
        let prepared = old.chord.tones.contains(pitch)
            && matches!(
                pitch.abs_diff(old.realized.notes[0]) % 12,
                0 | 3 | 4 | 7 | 8 | 9
            );
        // A consonance caught before V7 can become its chordal seventh, then a
        // suspension over I. Retaining it keeps the same downward obligation.
        let continued_seventh = instrument_harmony && old.chord.seventh == Some(pitch % 12);
        if (!prepared && !old.suspended[index] && !continued_seventh)
            || key.diatonic_step(pitch, -1) != Some(target.notes[voice])
        {
            return None;
        }
        let held_interval = (pitch + 12 - target.notes[0] % 12) % 12;
        let resolved_interval = (target.notes[voice] + 12 - target.notes[0] % 12) % 12;
        if !matches!(held_interval, 1 | 2 | 5 | 10 | 11)
            || !matches!(resolved_interval, 0 | 3 | 4 | 7 | 8 | 9)
        {
            return None;
        }
        realized.notes[voice] = pitch;
        suspended[index] = true;
    }
    if valid_realization(key, realized, instrument_harmony) {
        Some((realized, suspended))
    } else {
        None
    }
}

/// Stationary voices do not form a parallel: both voices must move in the same
/// direction and retain the same perfect interval class.
fn parallel_perfects(old: Voicing, new: Voicing) -> bool {
    forbidden_parallels(old, new, false)
}

pub(crate) fn forbidden_parallels(old: Voicing, new: Voicing, allow_anchors: bool) -> bool {
    for lower in 0..4 {
        for upper in lower + 1..4 {
            if !(allow_anchors && lower == 0 && upper == 3) && parallel_pair(old, new, lower, upper)
            {
                return true;
            }
        }
    }
    false
}

pub(crate) fn parallel_pair(old: Voicing, new: Voicing, lower: usize, upper: usize) -> bool {
    let before = old.notes[upper].abs_diff(old.notes[lower]) % 12;
    let after = new.notes[upper].abs_diff(new.notes[lower]) % 12;
    let lower_motion = i16::from(new.notes[lower]) - i16::from(old.notes[lower]);
    let upper_motion = i16::from(new.notes[upper]) - i16::from(old.notes[upper]);
    before == after
        && matches!(before, 0 | 7)
        && lower_motion != 0
        && upper_motion != 0
        && lower_motion.signum() == upper_motion.signum()
}

fn rank(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Solution>,
    preference: VoicingPreference,
    keep_chord: bool,
) -> u32 {
    let mut score = if chord.kind == ChordKind::Seventh {
        12
    } else {
        0
    };
    // Prefer a stable bass; a six-four remains available when its motion wins.
    score += match chord.role(voicing.notes[0]) {
        Some(ToneRole::Third) => 2,
        Some(ToneRole::Fifth) => 8,
        Some(ToneRole::Seventh) => 4,
        _ => 0,
    };
    if let Some(old) = previous {
        if keep_chord
            && old.realized.notes[0] == voicing.notes[0]
            && old.realized.notes[3] == voicing.notes[3]
            && old.chord != chord
        {
            // With unchanged anchors, revoice the current chord whenever it
            // remains legal; register controls should not choose a new harmony.
            score += 4096;
        }
        for voice in 0..4 {
            let from = old.realized.notes[voice];
            let to = voicing.notes[voice];
            if voice == 1 || voice == 2 {
                let distance = u32::from(from.abs_diff(to));
                score += distance * distance;
                if (preference.motion == Motion::Lower && to > from)
                    || (preference.motion == Motion::Higher && to < from)
                {
                    score += distance * 8;
                }
            }
            if key.leading_tone() == Some(from % 12) && key.diatonic_step(from, 1) != Some(to) {
                score += 96;
            }
            if old.chord.seventh == Some(from % 12) && key.diatonic_step(from, -1) != Some(to) {
                score += 96;
            }
        }
    } else {
        score +=
            u32::from(voicing.notes[1].abs_diff(55)) + u32::from(voicing.notes[2].abs_diff(64));
    }
    if preference.motion != Motion::Smooth {
        let (tenor, alto) = match preference.motion {
            Motion::Lower => (48, 55),
            Motion::Higher => (67, 76),
            Motion::Smooth => unreachable!(),
        };
        score += 12
            * (u32::from(voicing.notes[1].abs_diff(tenor))
                + u32::from(voicing.notes[2].abs_diff(alto)));
    }
    let gap = voicing.notes[2] - voicing.notes[1];
    score += if preference.open {
        2 * u32::from(12 - gap).pow(2)
    } else if previous.is_some_and(|old| {
        old.realized.notes[0] == voicing.notes[0] && old.realized.notes[3] == voicing.notes[3]
    }) {
        4 * u32::from(gap.saturating_sub(5)).pow(2)
    } else {
        0
    };
    if chord.kind == ChordKind::Triad
        && voicing
            .notes
            .into_iter()
            .filter(|note| note % 12 == chord.root)
            .count()
            < 2
    {
        score += 4;
    }
    score
}

fn cadence_targets(
    key: Key,
    chord: Chord,
    target: Voicing,
    previous: Option<Solution>,
    intent: HarmonicIntent,
) -> bool {
    let Some(old) = previous else {
        return true;
    };
    if intent != HarmonicIntent::Arrive {
        return true;
    }
    let dominant_to_tonic =
        old.chord.function() == HarmonicFunction::Dominant && chord.degree == Degree::I;
    let six_four_to_dominant =
        cadential_six_four(old.chord, old.realized) && chord.degree == Degree::V;
    for voice in 1..3 {
        let note = old.realized.notes[voice];
        let expected = if dominant_to_tonic && key.leading_tone() == Some(note % 12) {
            key.diatonic_step(note, 1)
        } else if (dominant_to_tonic
            && (old.chord.seventh == Some(note % 12) || note % 12 == key.pitch(Degree::Iv)))
            || (six_four_to_dominant
                && (note % 12 == old.chord.root || note % 12 == old.chord.third))
        {
            key.diatonic_step(note, -1)
        } else {
            continue;
        };
        if expected != Some(target.notes[voice]) {
            return false;
        }
    }
    true
}

fn cadential_six_four(chord: Chord, voicing: Voicing) -> bool {
    chord.degree == Degree::I
        && chord.kind == ChordKind::Triad
        && chord.role(voicing.notes[0]) == Some(ToneRole::Fifth)
}

#[derive(Clone, Copy)]
pub(crate) enum SixFourContext {
    Arpeggiated,
    Cadential,
    Pedal,
    Passing,
    Unprepared,
}

fn six_four_context(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Solution>,
    policy: HarmonyPolicy,
) -> Option<SixFourContext> {
    if chord.kind != ChordKind::Triad || chord.role(voicing.notes[0]) != Some(ToneRole::Fifth) {
        return None;
    }
    Some(classify_six_four_motion(
        key,
        voicing,
        previous.map(|old| old.realized),
        policy.required == Some(chord) && previous.is_some_and(|old| old.chord == chord),
        policy.intent == HarmonicIntent::Lean && cadential_six_four(chord, voicing),
    ))
}

/// Shared motion classification after a caller has identified a six-four chord.
pub(crate) fn classify_six_four_motion(
    key: Key,
    voicing: Voicing,
    previous: Option<Voicing>,
    arpeggiated: bool,
    cadential: bool,
) -> SixFourContext {
    if arpeggiated {
        return SixFourContext::Arpeggiated;
    }
    if cadential {
        return SixFourContext::Cadential;
    }
    if let Some(old) = previous {
        if old.notes[0] == voicing.notes[0] {
            return SixFourContext::Pedal;
        }
        if [-1, 1]
            .into_iter()
            .any(|step| key.diatonic_step(old.notes[0], step) == Some(voicing.notes[0]))
        {
            return SixFourContext::Passing;
        }
    }
    SixFourContext::Unprepared
}

fn phrase_cost(
    key: Key,
    chord: Chord,
    voicing: Voicing,
    previous: Option<Solution>,
    policy: HarmonyPolicy,
) -> u32 {
    use HarmonicFunction::{Dominant, Predominant, Tonic};
    let context = six_four_context(key, chord, voicing, previous, policy);
    let function = if matches!(context, Some(SixFourContext::Cadential)) {
        Dominant
    } else {
        chord.function()
    };
    let mut cost = match policy.intent {
        HarmonicIntent::Continue => match previous {
            // Preserve the established neutral first voicing; phrase direction
            // begins with the next chord, not a guessed function at startup.
            None => 0,
            Some(old) if old.chord == chord => 0,
            Some(old) if (old.chord.degree as u8 + 3) % 7 == chord.degree as u8 => 0,
            Some(old) => match (old.chord.function(), function) {
                (Tonic, Tonic) | (Predominant, Predominant) | (Dominant, Dominant) => 12,
                (Predominant, Dominant) | (Dominant, Tonic) => 32,
                (Tonic, Predominant) => 64,
                (Tonic, Dominant) => 96,
                _ => 160,
            },
        },
        HarmonicIntent::Lean => match previous.map(|old| {
            if cadential_six_four(old.chord, old.realized) {
                Dominant
            } else {
                old.chord.function()
            }
        }) {
            None | Some(Tonic) => match function {
                Predominant => 0,
                Dominant => 64,
                Tonic => 192,
            },
            Some(Predominant) => match function {
                Dominant => 0,
                Predominant => 96,
                Tonic => 192,
            },
            Some(Dominant) => match function {
                Dominant => 0,
                Tonic => 128,
                Predominant => 192,
            },
        },
        HarmonicIntent::Arrive => {
            if previous.is_some_and(|old| cadential_six_four(old.chord, old.realized)) {
                match chord.degree {
                    Degree::V => 0,
                    Degree::I => 96,
                    _ => 192,
                }
            } else {
                match chord.degree {
                    Degree::I => 0,
                    Degree::Vi => 128,
                    Degree::Iii => 192,
                    _ => 256,
                }
            }
        }
    };
    // A deliberate Lean should sustain its harmonic direction before seeking
    // novelty: a fresh degree must not turn dominant pressure into a tonic arrival.
    if policy.variation && policy.intent == HarmonicIntent::Lean {
        cost *= 128;
    }
    if policy.sequence
        && policy.intent == HarmonicIntent::Continue
        && previous.is_some_and(|old| (old.chord.degree as u8 + 3) % 7 != chord.degree as u8)
    {
        cost += 128;
    }
    cost += match context {
        None | Some(SixFourContext::Arpeggiated | SixFourContext::Cadential) => 0,
        Some(SixFourContext::Pedal | SixFourContext::Passing) => 8,
        Some(SixFourContext::Unprepared) => 96,
    };
    if policy.intent == HarmonicIntent::Lean
        && function == Dominant
        && chord.kind == ChordKind::Triad
    {
        cost += 24;
    }
    // A cadential I6/4 points specifically toward V: its fourth and sixth
    // descend to the dominant's third and fifth. This is a preference except
    // during an explicit Arrive command, where cadence_targets enforces it.
    if let Some(old) = previous.filter(|old| cadential_six_four(old.chord, old.realized)) {
        for voice in 1..3 {
            let note = old.realized.notes[voice];
            if (note % 12 == old.chord.root || note % 12 == old.chord.third)
                && key.diatonic_step(note, -1) != Some(voicing.notes[voice])
            {
                cost += 64;
            }
        }
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harmony(notes: [u8; 4], degree: Degree, kind: ChordKind) -> Solution {
        Solution {
            voicing: Voicing { notes },
            realized: Voicing { notes },
            chord: Key::C_MAJOR.chord(degree, kind),
            suspended: [false; 2],
            anchor_conflict: false,
            score: 0,
        }
    }

    fn phrase(intent: HarmonicIntent) -> HarmonyPolicy {
        HarmonyPolicy {
            intent,
            ..HarmonyPolicy::default()
        }
    }

    #[test]
    fn forced_outer_conflicts_preserve_harmony_without_excusing_generated_pairs() {
        let old = harmony([36, 55, 64, 72], Degree::I, ChordKind::Triad);
        let next = harmonize(
            Key::C_MAJOR,
            38,
            74,
            Some(old),
            [None; 2],
            HarmonyPolicy::default(),
        )
        .unwrap();
        assert_eq!((next.realized.notes[0], next.realized.notes[3]), (38, 74));
        assert!(next.anchor_conflict);
        assert!(valid_realization(Key::C_MAJOR, next.realized, true));
        assert!(!forbidden_parallels(old.realized, next.realized, true));
        assert!(parallel_pair(old.realized, next.realized, 0, 3));
        let doubled = harmonize(
            Key::C_MAJOR,
            47,
            71,
            None,
            [None; 2],
            HarmonyPolicy::default(),
        )
        .unwrap();
        assert!(doubled.anchor_conflict);
        assert!(valid_realization(Key::C_MAJOR, doubled.realized, true));
        assert!(!valid_realization(Key::C_MAJOR, doubled.realized, false));
        assert!(!valid_realization(
            Key::C_MAJOR,
            Voicing {
                notes: [47, 59, 65, 71]
            },
            true
        ));
    }

    #[test]
    fn omitting_only_the_fifth_repairs_the_d_to_e_bass_slide() {
        let old = harmony([38, 53, 57, 62], Degree::Ii, ChordKind::Triad);
        let em7 = Key::C_MAJOR.chord(Degree::Iii, ChordKind::Seventh);
        let next = harmonize(
            Key::C_MAJOR,
            40,
            62,
            Some(old),
            [None; 2],
            HarmonyPolicy {
                required: Some(em7),
                ..HarmonyPolicy::default()
            },
        )
        .unwrap();
        assert_eq!(next.chord, em7);
        assert_eq!(
            chord_coverage(em7, next.voicing, FifthOmission::Penalize),
            Some(true)
        );
        assert!(!next
            .voicing
            .notes
            .into_iter()
            .any(|note| note % 12 == em7.fifth));
        for pitch in [em7.root, em7.third, em7.seventh.unwrap()] {
            assert!(next
                .voicing
                .notes
                .into_iter()
                .any(|note| note % 12 == pitch));
        }
        assert!(!next.anchor_conflict);
        assert!(!parallel_perfects(old.realized, next.realized));
        assert_eq!(
            chord_coverage(
                em7,
                Voicing {
                    notes: [40, 52, 55, 64]
                },
                FifthOmission::Penalize
            ),
            None
        );
    }

    #[test]
    fn diminished_chords_keep_the_fifth_that_defines_their_tritone() {
        let diminished = Key::C_MAJOR.chord(Degree::Vii, ChordKind::Triad);
        let half_diminished = Key::C_MAJOR.chord(Degree::Vii, ChordKind::Seventh);
        assert_eq!(
            chord_coverage(
                diminished,
                Voicing {
                    notes: [47, 50, 62, 71]
                },
                FifthOmission::Penalize
            ),
            None
        );
        assert_eq!(
            chord_coverage(
                half_diminished,
                Voicing {
                    notes: [47, 62, 69, 71]
                },
                FifthOmission::Penalize
            ),
            None
        );
        let complete = harmonize(
            Key::C_MAJOR,
            47,
            71,
            None,
            [None; 2],
            HarmonyPolicy {
                required: Some(diminished),
                ..HarmonyPolicy::default()
            },
        )
        .unwrap();
        assert_eq!(
            chord_coverage(diminished, complete.voicing, FifthOmission::Penalize),
            Some(false)
        );
        assert!(complete.anchor_conflict);
    }

    #[test]
    fn phrase_commands_change_harmonic_function_under_compatible_anchors() {
        let tonic = harmonize(
            Key::C_MAJOR,
            36,
            64,
            None,
            [None; 2],
            HarmonyPolicy::default(),
        )
        .unwrap();
        assert_eq!(tonic.chord.degree, Degree::I);
        let approach = harmonize(
            Key::C_MAJOR,
            36,
            64,
            Some(tonic),
            [None; 2],
            phrase(HarmonicIntent::Lean),
        )
        .unwrap();
        assert_eq!(approach.chord.degree, Degree::Iv);
        assert_eq!(approach.chord.kind, ChordKind::Seventh);
        let arrival = harmonize(
            Key::C_MAJOR,
            36,
            64,
            Some(approach),
            [None; 2],
            phrase(HarmonicIntent::Arrive),
        )
        .unwrap();
        assert_eq!(arrival.chord.degree, Degree::I);
        for solution in [approach, arrival] {
            assert_eq!(
                (solution.realized.notes[0], solution.realized.notes[3]),
                (36, 64)
            );
        }
    }

    #[test]
    fn reharmonization_changes_the_chord_and_inversion_keeps_its_identity() {
        let tonic = harmonize(
            Key::C_MAJOR,
            36,
            64,
            None,
            [None; 2],
            HarmonyPolicy::default(),
        )
        .unwrap();
        let alternative = harmonize(
            Key::C_MAJOR,
            36,
            64,
            Some(tonic),
            [None; 2],
            HarmonyPolicy {
                exclude: Some(tonic.chord),
                ..HarmonyPolicy::default()
            },
        )
        .unwrap();
        assert_eq!(alternative.chord.degree, Degree::Vi);
        assert_ne!(alternative.chord, tonic.chord);
        let inversion = harmonize(
            Key::C_MAJOR,
            40,
            64,
            Some(alternative),
            [None; 2],
            HarmonyPolicy {
                required: Some(alternative.chord),
                ..HarmonyPolicy::default()
            },
        )
        .unwrap();
        assert_eq!(inversion.chord, alternative.chord);
        assert_eq!(
            (inversion.voicing.notes[0], inversion.voicing.notes[3]),
            (40, 64)
        );
        assert!(matches!(
            six_four_context(
                Key::C_MAJOR,
                inversion.chord,
                inversion.voicing,
                Some(alternative),
                HarmonyPolicy {
                    required: Some(inversion.chord),
                    ..HarmonyPolicy::default()
                }
            ),
            Some(SixFourContext::Arpeggiated)
        ));
        assert_eq!(
            harmonize(
                Key::C_MAJOR,
                36,
                64,
                Some(tonic),
                [None; 2],
                HarmonyPolicy {
                    required: Some(tonic.chord),
                    exclude: Some(tonic.chord),
                    ..HarmonyPolicy::default()
                }
            ),
            None
        );
    }

    #[test]
    fn an_explicit_dominant_arrival_resolves_inner_tendencies() {
        let dominant = harmony([43, 53, 59, 62], Degree::V, ChordKind::Seventh);
        let arrival = harmonize(
            Key::C_MAJOR,
            36,
            67,
            Some(dominant),
            [None; 2],
            phrase(HarmonicIntent::Arrive),
        )
        .unwrap();
        assert_eq!(arrival.chord.degree, Degree::I);
        assert_eq!(arrival.voicing.notes[1..3], [52, 60]);
        let unresolved = Voicing {
            notes: [36, 55, 64, 67],
        };
        assert!(!cadence_targets(
            Key::C_MAJOR,
            arrival.chord,
            unresolved,
            Some(dominant),
            HarmonicIntent::Arrive
        ));
        let prepared = harmony([41, 57, 65, 72], Degree::Iv, ChordKind::Triad);
        let held = harmonize(
            Key::C_MAJOR,
            36,
            72,
            Some(prepared),
            [None, Some(65)],
            phrase(HarmonicIntent::Arrive),
        )
        .unwrap();
        assert_eq!(held.chord.degree, Degree::I);
        assert_eq!(held.voicing.notes[2], 64);
        assert_eq!(held.realized.notes[2], 65);
        assert!(held.suspended[1]);
    }

    #[test]
    fn six_four_contexts_distinguish_pedal_passing_and_cadential_motion() {
        let tonic = harmony([36, 55, 64, 67], Degree::I, ChordKind::Triad);
        let iv = Key::C_MAJOR.chord(Degree::Iv, ChordKind::Triad);
        let v = Key::C_MAJOR.chord(Degree::V, ChordKind::Triad);
        assert!(matches!(
            six_four_context(
                Key::C_MAJOR,
                iv,
                Voicing {
                    notes: [36, 57, 65, 72]
                },
                Some(tonic),
                HarmonyPolicy::default()
            ),
            Some(SixFourContext::Pedal)
        ));
        assert!(matches!(
            six_four_context(
                Key::C_MAJOR,
                v,
                Voicing {
                    notes: [38, 55, 59, 67]
                },
                Some(tonic),
                HarmonyPolicy::default()
            ),
            Some(SixFourContext::Passing)
        ));
        let cadential = harmony([43, 60, 64, 67], Degree::I, ChordKind::Triad);
        assert!(matches!(
            six_four_context(
                Key::C_MAJOR,
                cadential.chord,
                cadential.voicing,
                Some(tonic),
                phrase(HarmonicIntent::Lean)
            ),
            Some(SixFourContext::Cadential)
        ));
        let dominant = harmonize(
            Key::C_MAJOR,
            43,
            67,
            Some(cadential),
            [None; 2],
            phrase(HarmonicIntent::Arrive),
        )
        .unwrap();
        assert_eq!(dominant.chord.degree, Degree::V);
        assert_eq!(dominant.voicing.notes[1..3], [59, 62]);
    }

    #[test]
    fn independent_hold_realizations_remain_ordered_and_major_policy_is_explicit() {
        assert!(valid_realization(
            Key::C_MAJOR,
            Voicing {
                notes: [36, 62, 64, 72]
            },
            true
        ));
        assert!(!valid_realization(
            Key::C_MAJOR,
            Voicing {
                notes: [36, 62, 62, 72]
            },
            true
        ));
        let minor = Key::new(9, Mode::NaturalMinor).unwrap();
        assert_eq!(
            harmonize(minor, 33, 69, None, [None; 2], HarmonyPolicy::default()),
            None
        );
    }

    #[test]
    fn a_caught_consonance_can_continue_through_a_seventh_into_a_suspension() {
        let subdominant = harmony([41, 57, 65, 72], Degree::Iv, ChordKind::Triad);
        let dominant = harmonize(
            Key::C_MAJOR,
            43,
            71,
            Some(subdominant),
            [None, Some(65)],
            phrase(HarmonicIntent::Lean),
        )
        .unwrap();
        assert_eq!(dominant.chord.degree, Degree::V);
        assert_eq!(dominant.chord.kind, ChordKind::Seventh);
        assert_eq!(dominant.realized.notes[2], 65);
        assert!(!dominant.suspended[1]);
        let arrival = harmonize(
            Key::C_MAJOR,
            48,
            72,
            Some(dominant),
            [None, Some(65)],
            phrase(HarmonicIntent::Arrive),
        )
        .unwrap();
        assert_eq!(arrival.chord.degree, Degree::I);
        assert_eq!(arrival.realized.notes[2], 65);
        assert_eq!(arrival.voicing.notes[2], 64);
        assert!(arrival.suspended[1]);
        assert!(!forbidden_parallels(
            dominant.realized,
            arrival.realized,
            true
        ));
        // The legacy chord solver still requires consonant preparation.
        assert_eq!(
            realize_holds(
                Key::C_MAJOR,
                arrival.chord,
                arrival.voicing,
                Some(dominant),
                [None, Some(65)],
                false
            ),
            None
        );
    }

    #[test]
    fn tonic_is_complete_ordered_and_bounded() {
        let solution = solve(
            Key::C_MAJOR,
            36,
            72,
            None,
            [None; 2],
            VoicingPreference::default(),
        )
        .unwrap();
        assert_eq!(solution.voicing.notes[0], 36);
        assert_eq!(solution.voicing.notes[3], 72);
        assert!(valid_spacing(solution.voicing));
        assert!(complete_chord(solution.chord, solution.voicing));
        assert!(!doubled_leading_tone(Key::C_MAJOR, solution.voicing));
    }

    #[test]
    fn prepared_f_to_c_four_three_suspension_has_real_target() {
        let old = harmony([41, 57, 65, 72], Degree::Iv, ChordKind::Triad);
        let solution = solve(
            Key::C_MAJOR,
            36,
            72,
            Some(old),
            [None, Some(65)],
            VoicingPreference::default(),
        )
        .unwrap();
        assert_eq!(solution.chord.degree, Degree::I);
        assert_eq!(solution.voicing.notes[2], 64);
        assert_eq!(solution.suspended, [false, true]);
        let (realized, _) = realize_holds(
            Key::C_MAJOR,
            solution.chord,
            solution.voicing,
            Some(old),
            [None, Some(65)],
            false,
        )
        .unwrap();
        assert_eq!(realized.notes[2], 65);
        assert!(valid_spacing(realized));
    }

    #[test]
    fn common_tones_stay_fixed_and_unprepared_notes_are_rejected() {
        let old = harmony([36, 55, 64, 72], Degree::I, ChordKind::Triad);
        let common = solve(
            Key::C_MAJOR,
            36,
            72,
            Some(old),
            [None, Some(64)],
            VoicingPreference::default(),
        )
        .unwrap();
        assert_eq!(common.voicing.notes[2], 64);
        assert_eq!(common.realized.notes[2], 64);
        assert!(!common.suspended[1]);
        assert_eq!(
            solve(
                Key::C_MAJOR,
                36,
                72,
                Some(old),
                [None, Some(65)],
                VoicingPreference::default()
            ),
            None
        );
        assert_eq!(
            solve(
                Key::C_MAJOR,
                36,
                72,
                None,
                [None, Some(65)],
                VoicingPreference::default()
            ),
            None
        );
    }

    #[test]
    fn already_prepared_suspension_can_survive_another_harmony_change() {
        let old = harmony([41, 57, 65, 72], Degree::Iv, ChordKind::Triad);
        let first = solve(
            Key::C_MAJOR,
            36,
            72,
            Some(old),
            [None, Some(65)],
            VoicingPreference::default(),
        )
        .unwrap();
        assert!(!first.chord.tones.contains(65));
        let continued = solve(
            Key::C_MAJOR,
            48,
            76,
            Some(first),
            [None, Some(65)],
            VoicingPreference::default(),
        )
        .unwrap();
        assert_eq!(continued.realized.notes[2], 65);
        assert_eq!(continued.voicing.notes[2], 64);
        assert!(continued.suspended[1]);
    }

    #[test]
    fn a_previous_dissonant_chord_tone_does_not_prepare_a_suspension() {
        let old = harmony([36, 57, 65, 72], Degree::Iv, ChordKind::Triad);
        let target = Voicing {
            notes: [36, 55, 64, 72],
        };
        let chord = Key::C_MAJOR.chord(Degree::I, ChordKind::Triad);
        assert!(old.chord.tones.contains(65));
        assert_eq!(
            realize_holds(
                Key::C_MAJOR,
                chord,
                target,
                Some(old),
                [None, Some(65)],
                false
            ),
            None
        );
    }

    #[test]
    fn held_realization_cannot_cross_melody_or_exceed_spacing() {
        let old = harmony([41, 57, 72, 77], Degree::Iv, ChordKind::Triad);
        assert_eq!(
            solve(
                Key::C_MAJOR,
                36,
                72,
                Some(old),
                [None, Some(72)],
                VoicingPreference::default()
            ),
            None
        );
    }

    #[test]
    fn anchors_and_midi_boundaries_do_not_relax_rules() {
        for (bass, melody) in [(23, 72), (49, 72), (36, 59), (36, 89), (36, 128), (37, 72)] {
            assert_eq!(
                solve(
                    Key::C_MAJOR,
                    bass,
                    melody,
                    None,
                    [None; 2],
                    VoicingPreference::default()
                ),
                None
            );
        }
        let boundary = solve(
            Key::C_MAJOR,
            48,
            60,
            None,
            [None; 2],
            VoicingPreference::default(),
        )
        .unwrap();
        assert!(valid_spacing(boundary.voicing));
        let old = harmony([36, 55, 64, 72], Degree::I, ChordKind::Triad);
        // Both played anchors move up an octave class in parallel. No choice
        // of inner voices can repair that, so the solver must report failure.
        assert_eq!(
            solve(
                Key::C_MAJOR,
                38,
                74,
                Some(old),
                [None; 2],
                VoicingPreference::default()
            ),
            None
        );
        let old = harmony([36, 52, 60, 67], Degree::I, ChordKind::Triad);
        assert_eq!(
            solve(
                Key::C_MAJOR,
                38,
                69,
                Some(old),
                [None; 2],
                VoicingPreference::default()
            ),
            None
        );
    }

    #[test]
    fn natural_minor_uses_its_own_chord_vocabulary() {
        let key = Key::new(9, super::super::theory::Mode::NaturalMinor).unwrap();
        let next = solve(key, 33, 69, None, [None; 2], VoicingPreference::default()).unwrap();
        assert_eq!(next.chord.degree, Degree::I);
        assert_eq!(
            next.chord.quality(),
            super::super::theory::ChordQuality::Minor
        );
        assert!(complete_chord(next.chord, next.voicing));
    }

    #[test]
    fn movement_controls_change_inner_register_without_changing_anchors_or_chord() {
        let neutral = solve(
            Key::C_MAJOR,
            36,
            76,
            None,
            [None; 2],
            VoicingPreference::default(),
        )
        .unwrap();
        let mut alternatives = [neutral; 2];
        for (index, motion) in [Motion::Lower, Motion::Higher].into_iter().enumerate() {
            let next = solve(
                Key::C_MAJOR,
                36,
                76,
                Some(neutral),
                [None; 2],
                VoicingPreference {
                    motion,
                    open: false,
                },
            )
            .unwrap();
            assert_eq!(next.voicing.notes[0], 36);
            assert_eq!(next.voicing.notes[3], 76);
            assert_eq!(next.chord, neutral.chord);
            assert!(valid_spacing(next.voicing) && complete_chord(next.chord, next.voicing));
            assert!(!parallel_perfects(neutral.realized, next.realized));
            alternatives[index] = next;
        }
        let lower = alternatives[0].voicing.notes;
        let higher = alternatives[1].voicing.notes;
        assert_ne!(&lower[1..3], &higher[1..3]);
        assert!(
            u16::from(lower[1]) + u16::from(lower[2]) < u16::from(higher[1]) + u16::from(higher[2])
        );
    }

    #[test]
    fn open_control_widens_inner_voices_within_the_same_hard_limits() {
        let neutral = solve(
            Key::C_MAJOR,
            36,
            79,
            None,
            [None; 2],
            VoicingPreference::default(),
        )
        .unwrap();
        let open = solve(
            Key::C_MAJOR,
            36,
            79,
            Some(neutral),
            [None; 2],
            VoicingPreference {
                motion: Motion::Smooth,
                open: true,
            },
        )
        .unwrap();
        let gap = |solution: Solution| solution.voicing.notes[2] - solution.voicing.notes[1];
        assert!(gap(open) > gap(neutral));
        assert_eq!(open.chord, neutral.chord);
        assert_eq!(open.voicing.notes[0], 36);
        assert_eq!(open.voicing.notes[3], 79);
        assert!(valid_spacing(open.voicing) && complete_chord(open.chord, open.voicing));
        assert!(!parallel_perfects(neutral.realized, open.realized));
        let compact = solve(
            Key::C_MAJOR,
            36,
            79,
            Some(open),
            [None; 2],
            VoicingPreference::default(),
        )
        .unwrap();
        assert!(
            gap(compact) < gap(open),
            "open={open:?}, compact={compact:?}"
        );
    }

    #[test]
    fn stationary_voices_are_not_parallel_perfects() {
        let old = Voicing {
            notes: [36, 55, 64, 72],
        };
        assert!(!parallel_perfects(old, old));
        assert!(!parallel_perfects(
            old,
            Voicing {
                notes: [36, 55, 65, 72]
            }
        ));
        assert!(parallel_perfects(
            old,
            Voicing {
                notes: [38, 57, 65, 74]
            }
        ));
        let repeated = harmony(old.notes, Degree::I, ChordKind::Triad);
        assert_eq!(
            solve(
                Key::C_MAJOR,
                36,
                72,
                Some(repeated),
                [None; 2],
                VoicingPreference::default()
            )
            .unwrap()
            .realized,
            old
        );
    }

    #[test]
    fn two_suspensions_cannot_resolve_in_parallel_fifths() {
        let old = harmony([43, 55, 62, 71], Degree::V, ChordKind::Triad);
        let target = Voicing {
            notes: [45, 53, 60, 72],
        };
        let chord = Key::C_MAJOR.chord(Degree::Iv, ChordKind::Triad);
        let (realized, suspended) = realize_holds(
            Key::C_MAJOR,
            chord,
            target,
            Some(old),
            [Some(55), Some(62)],
            false,
        )
        .unwrap();
        assert_eq!(suspended, [true; 2]);
        assert!(valid_spacing(realized) && complete_chord(chord, target));
        assert!(parallel_perfects(realized, target));
        let next = solve(
            Key::C_MAJOR,
            45,
            72,
            Some(old),
            [Some(55), Some(62)],
            VoicingPreference::default(),
        )
        .unwrap();
        // D minor seventh can retain D as a common tone; the forbidden F-major
        // interpretation would move both inner voices down in fifths.
        assert_ne!(next.voicing, target);
        assert_eq!(next.suspended, [true, false]);
        assert!(!parallel_perfects(next.realized, next.voicing));
    }

    #[test]
    fn dominant_seventh_resolves_seventh_down_and_leading_tone_up() {
        let old = harmony([43, 53, 59, 62], Degree::V, ChordKind::Seventh);
        let next = solve(
            Key::C_MAJOR,
            36,
            67,
            Some(old),
            [None; 2],
            VoicingPreference::default(),
        )
        .unwrap();
        assert_eq!(next.chord.degree, Degree::I);
        assert_eq!(next.voicing.notes, [36, 52, 60, 67]);
    }

    #[test]
    fn returned_progressions_always_obey_hard_rules() {
        let old = harmony([36, 55, 64, 72], Degree::I, ChordKind::Triad);
        let mut found = 0;
        for bass in [24, 29, 36, 43, 48] {
            for melody in [60, 65, 67, 72, 79, 84, 88] {
                let Some(next) = solve(
                    Key::C_MAJOR,
                    bass,
                    melody,
                    Some(old),
                    [None; 2],
                    VoicingPreference::default(),
                ) else {
                    continue;
                };
                found += 1;
                assert_eq!(next.voicing.notes[0], bass);
                assert_eq!(next.voicing.notes[3], melody);
                assert!(valid_spacing(next.voicing));
                assert!(complete_chord(next.chord, next.voicing));
                assert!(!doubled_leading_tone(Key::C_MAJOR, next.voicing));
                assert!(!parallel_perfects(old.realized, next.realized));
                assert!(!parallel_perfects(old.realized, next.voicing));
            }
        }
        assert!(found > 10);
    }

    #[test]
    fn seventh_chords_are_complete_and_leading_tones_are_not_doubled() {
        let chord = Key::C_MAJOR.chord(Degree::V, ChordKind::Seventh);
        assert!(complete_chord(
            chord,
            Voicing {
                notes: [43, 53, 59, 74]
            }
        ));
        assert!(!complete_chord(
            chord,
            Voicing {
                notes: [43, 55, 59, 74]
            }
        ));
        assert!(doubled_leading_tone(
            Key::C_MAJOR,
            Voicing {
                notes: [35, 55, 62, 71]
            }
        ));
    }
}
