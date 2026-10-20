//! Four MIDI voices around literal bass and melody, with expressive harmony controls.

use crate::pipeline::report::MidiEvent;

use super::harmony::{self, Plan};
use super::{Key, PitchClassSet};

pub use super::harmony::{AnchorVoice, Gesture as HarmonyCommand, Steering};

const INNER_GATES: u8 = 0b0110;
const MELODY_FLOW_MS: u64 = 450;

#[derive(Clone, Copy)]
struct MelodyAttack {
    pitch: u8,
    at_ms: Option<u64>,
}

/// A coherent input frame, with voices ordered bass, tenor, alto, melody.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AutoChordInput {
    /// Exact played bass pitch; harmonic gestures never change it.
    pub bass: Option<u8>,
    /// Exact played melody pitch; harmonic gestures never change it.
    pub melody: Option<u8>,
    /// Extra held-key pitch classes guide harmony without directly sounding notes.
    pub extra_tones: PitchClassSet,
    /// Sounding voice bits, 0 through 3 in voice order.
    pub gates: u8,
    /// Independently catch tenor and alto; release approaches their saved targets.
    pub hold: [bool; 2],
    /// A new physical press, including an already selected pitch.
    pub revoice: bool,
    /// Rearticulate these voice bits once, except pitches protected by catches.
    pub retrigger: u8,
    /// One musical gesture; articulates uncaught inner voices, including common tones.
    pub command: Option<HarmonyCommand>,
    /// Current physical thumb steering; release leaves the heard harmony intact.
    pub steering: Steering,
    /// Held bass/melody reinterpretation bar; release ends only its preference.
    pub interpretation: Option<AnchorVoice>,
    /// Permit borrowed harmony and chromatic functional substitutions.
    pub borrowing: bool,
    /// Permit suspended, extended, and altered chord colours.
    pub extensions: bool,
    /// Monotonic performance time; read only at musical events, never a scheduler.
    pub now_ms: Option<u64>,
}

/// An invalid MIDI output configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// MIDI channels are numbered 0 through 15 internally.
    Channel(u8),
    /// Attack velocity must be between 1 and 127.
    Velocity(u8),
}

/// Invalid input; rejection first releases all owned MIDI notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// Pitch outside the MIDI range.
    Note(u8),
    /// Played pitch outside the configured keyboard scale.
    OutsideKey(u8),
    /// Only the low four voice-gate bits are valid.
    Gates(u8),
    /// Only the low four rearticulation bits are valid.
    Retrigger(u8),
}

/// An allocation-free instrument sharing the existing MIDI event representation.
///
/// Played outer pitches always win. The harmony search chooses the two inner
/// voices, including chromatic colours and their directed resolutions. Explicit
/// catches hold any sounding inner pitch, including a seventh or chromatic tone.
/// No failed search steals a catch, changes a keyboard note, or erases history.
pub struct AutoChord {
    key: Key,
    channel: u8,
    velocity: u8,
    input: AutoChordInput,
    solution: Option<Plan>,
    history: Option<Plan>,
    caught: [Option<u8>; 2],
    sounding: [Option<u8>; 4],
    melody_attacks: [Option<MelodyAttack>; 2],
}

impl AutoChord {
    /// Creates a silent instrument with validated output configuration.
    pub fn new(key: Key, channel: u8, velocity: u8) -> Result<Self, ConfigError> {
        if channel > 15 {
            return Err(ConfigError::Channel(channel));
        }
        if velocity == 0 || velocity > 127 {
            return Err(ConfigError::Velocity(velocity));
        }
        Ok(Self {
            key,
            channel,
            velocity,
            input: AutoChordInput::default(),
            solution: None,
            history: None,
            caught: [None; 2],
            sounding: [None; 4],
            melody_attacks: [None; 2],
        })
    }

    /// Current harmony, actual pitches, and saved inner-voice resolution targets.
    #[must_use]
    pub const fn solution(&self) -> Option<Plan> {
        self.solution
    }

    /// Last successfully sounded harmony, retained through rests and failed searches.
    #[must_use]
    pub const fn history(&self) -> Option<Plan> {
        self.history
    }

    /// Actual assigned pitches before MIDI pitch deduplication.
    #[must_use]
    pub const fn sounding(&self) -> [Option<u8>; 4] {
        self.sounding
    }

    /// Releases owned notes and clears phrase history, pending resolutions, and catches.
    pub fn reset(&mut self, emit: &mut impl FnMut(MidiEvent)) {
        self.deliver([None; 4], 0, emit);
        self.input = AutoChordInput::default();
        self.solution = None;
        self.history = None;
        self.caught = [None; 2];
        self.melody_attacks = [None; 2];
    }

    /// Processes one stable frame without an allocator, scheduler, or transport.
    pub fn update(
        &mut self,
        input: AutoChordInput,
        emit: &mut impl FnMut(MidiEvent),
    ) -> Result<(), InputError> {
        if let Err(error) = self.validate(input) {
            self.reset(emit);
            return Err(error);
        }
        if (AutoChordInput {
            now_ms: None,
            ..input
        }) == self.input
        {
            return Ok(());
        }
        let melody_attack = input.gates & 8 != 0
            && (input.retrigger & 8 != 0
                || input.melody != self.input.melody
                || self.input.gates & 8 == 0);
        let event = self.harmonic_event(input, melody_attack);
        // Retain the phrase across rests, but not physical ties to released notes.
        let physical_history = |mut plan: Plan| {
            if input.gates & 1 == 0 || input.bass != Some(plan.voicing.notes[0]) {
                plan.pedal_prepared = None;
            }
            for index in 0..2 {
                if input.gates & (1 << (index + 1)) == 0 {
                    plan.inner_ornaments[index] = None;
                }
            }
            harmony::with_steering(plan, input.steering)
        };
        self.solution = self.solution.map(physical_history);
        self.history = self.history.map(physical_history);
        let released_voices = core::array::from_fn(|index| {
            let bit = 1 << (index + 1);
            (self.caught[index].is_some() && (!input.hold[index] || input.gates & bit == 0))
                || (self.input.gates & bit != 0 && input.gates & bit == 0)
        });
        self.capture(input);
        self.plan(input, released_voices, event, melody_attack);
        let mut desired = [None; 4];
        if input.gates & 1 != 0 {
            desired[0] = input.bass;
        }
        if input.gates & 8 != 0 {
            desired[3] = input.melody;
        }
        for index in 0..2 {
            if input.gates & (1 << (index + 1)) != 0 {
                // An incompatible new keyboard note cannot steal a voice held by the player.
                desired[index + 1] = self.caught[index]
                    .or_else(|| self.solution.map(|plan| plan.realized.notes[index + 1]));
            }
        }
        let retrigger = if input.command.is_some() {
            input.retrigger | (input.gates & INNER_GATES)
        } else {
            input.retrigger
        };
        self.deliver(desired, retrigger, emit);
        self.input = AutoChordInput {
            revoice: false,
            retrigger: 0,
            command: None,
            now_ms: None,
            ..input
        };
        if melody_attack {
            if let Some(pitch) = input.melody {
                self.melody_attacks.rotate_right(1);
                self.melody_attacks[0] = Some(MelodyAttack {
                    pitch,
                    at_ms: input.now_ms,
                });
            }
        }
        // This also arms a catch held before the first two-hand harmony appears.
        let before_capture = self.caught;
        self.capture(input);
        if self.caught != before_capture {
            self.solution = self
                .solution
                .map(|plan| harmony::refresh_resolution(self.key, plan, self.caught));
        }
        self.solution = self
            .solution
            .map(|plan| harmony::with_steering(plan, input.steering));
        if let Some(plan) = self.solution {
            self.history = Some(plan);
        }
        Ok(())
    }

    fn harmonic_event(&self, input: AutoChordInput, melody_attack: bool) -> harmony::HarmonicEvent {
        use harmony::HarmonicEvent::{Arrive, Continue, Reconsider, Structural};
        if input.command.is_some()
            || (input.bass == self.input.bass
                && input.melody == self.input.melody
                && input.extra_tones != self.input.extra_tones)
        {
            return Reconsider;
        }
        let Some(old) = self.solution.or(self.history) else {
            return Reconsider;
        };
        let (Some(bass), Some(melody)) = (input.bass, input.melody) else {
            return Reconsider;
        };
        if bass != old.realized.notes[0] || input.retrigger & 1 != 0 {
            return Reconsider;
        }
        if !melody_attack {
            return Continue;
        }
        let previous = self.melody_attacks[0];
        let from = previous.map_or(old.realized.notes[3], |attack| attack.pitch);
        if old
            .ornament
            .is_some_and(|ornament| ornament.origin == melody)
            || old.tendency.is_some_and(|goal| {
                goal.tones().contains(bass)
                    && goal.tones().contains(melody)
                    && (from == melody || !goal.tones().contains(from))
            })
        {
            return Arrive;
        }
        if from == melody {
            return Structural;
        }
        let gap = input
            .now_ms
            .zip(previous.and_then(|attack| attack.at_ms))
            .and_then(|(now, before)| now.checked_sub(before));
        let previous_gap = self.melody_attacks[0]
            .zip(self.melody_attacks[1])
            .and_then(|(last, before)| last.at_ms.zip(before.at_ms))
            .and_then(|(last, before)| last.checked_sub(before));
        let flowing = gap.is_none_or(|gap| {
            gap <= MELODY_FLOW_MS
                || previous_gap
                    .is_some_and(|before| before != 0 && gap <= before.saturating_mul(3) / 2)
        });
        let step = [-1, 1]
            .into_iter()
            .any(|direction| self.key.diatonic_step(from, direction) == Some(melody));
        if step && flowing {
            Continue
        } else {
            Structural
        }
    }

    fn plan(
        &mut self,
        input: AutoChordInput,
        released_voices: [bool; 2],
        event: harmony::HarmonicEvent,
        melody_attack: bool,
    ) {
        if input.gates & INNER_GATES == 0 {
            self.solution = None;
            return;
        }
        let (Some(bass), Some(melody)) = (input.bass, input.melody) else {
            self.solution = None;
            return;
        };
        let changed = self.solution.is_none_or(|old| {
            old.voicing.notes[0] != bass
                || old.voicing.notes[3] != melody
                || (old.root_implied && old.pedal_prepared != Some(bass))
        });
        if changed
            || input.command.is_some()
            || melody_attack
            || input.extra_tones != self.input.extra_tones
        {
            let previous = self.solution.or(self.history);
            let gesture = input.command.unwrap_or(HarmonyCommand::Follow);
            let guidance = harmony::Guidance {
                steering: input.steering,
                interpretation: input.interpretation,
                borrowing: input.borrowing,
                extensions: input.extensions,
                event,
                extra_tones: input.extra_tones,
            };
            if let Some(next) = harmony::select_steered(
                self.key,
                bass,
                melody,
                previous,
                self.caught,
                gesture,
                guidance,
            ) {
                self.solution = Some(next);
            } else if changed {
                self.solution = if gesture == HarmonyCommand::Follow {
                    None
                } else {
                    harmony::select_steered(
                        self.key,
                        bass,
                        melody,
                        previous,
                        self.caught,
                        HarmonyCommand::Follow,
                        guidance,
                    )
                };
            }
        }
        let Some(old) = self.solution else {
            return;
        };
        let owed_release = (0..2).any(|index| {
            self.caught[index].is_none()
                && (old.inner_ornaments[index].is_none() || released_voices[index])
                && (released_voices[index]
                    || old.realized.notes[index + 1] != old.voicing.notes[index + 1])
                && old.tendencies[index].is_some_and(|debt| {
                    old.realized.notes[index + 1] == debt.from
                        && old.voicing.notes[index + 1] != debt.to
                })
        });
        let mut released = old;
        for (index, was_released) in released_voices.into_iter().enumerate() {
            if was_released && self.caught[index].is_none() {
                released.inner_ornaments[index] = None;
            }
            released.realized.notes[index + 1] = self.caught[index]
                .or_else(|| released.inner_ornaments[index].map(|ornament| ornament.from))
                .unwrap_or(released.voicing.notes[index + 1]);
            released.suspended[index] =
                self.caught[index].is_some_and(|pitch| pitch != released.voicing.notes[index + 1]);
        }
        if !owed_release
            && harmony::valid_caught_realization(
                self.key,
                released.chord,
                released.realized,
                self.caught,
            )
        {
            self.solution = Some(
                if released.realized != old.realized
                    || released.inner_ornaments != old.inner_ornaments
                    || input.hold != self.input.hold
                {
                    harmony::refresh_resolution(self.key, released, self.caught)
                } else {
                    released
                },
            );
        } else {
            self.solution = Some(
                harmony::release_voices(self.key, old, self.caught, released_voices)
                    .unwrap_or_else(|| harmony::refresh_resolution(self.key, old, self.caught)),
            );
        }
        // If a partial release would cross the remaining catch, retain the
        // sounding pitch until a compatible gesture or the other catch releases.
    }

    fn validate(&self, input: AutoChordInput) -> Result<(), InputError> {
        if input.gates & !0b1111 != 0 {
            return Err(InputError::Gates(input.gates));
        }
        if input.retrigger & !0b1111 != 0 {
            return Err(InputError::Retrigger(input.retrigger));
        }
        for note in [input.bass, input.melody].into_iter().flatten() {
            if note > 127 {
                return Err(InputError::Note(note));
            }
            if !self.key.contains(note) {
                return Err(InputError::OutsideKey(note));
            }
        }
        Ok(())
    }

    fn capture(&mut self, input: AutoChordInput) {
        for index in 0..2 {
            if !input.hold[index]
                || input.gates & (1 << (index + 1)) == 0
                || self.sounding[index + 1].is_none()
            {
                self.caught[index] = None;
            } else if self.caught[index].is_none() {
                self.caught[index] = self.sounding[index + 1];
            }
        }
    }

    fn deliver(
        &mut self,
        desired: [Option<u8>; 4],
        retrigger: u8,
        emit: &mut impl FnMut(MidiEvent),
    ) {
        let forced = |note| {
            // A harmony attack cannot steal a literal voice's sustained MIDI pitch.
            if [0, 3].into_iter().any(|voice| {
                self.sounding[voice] == Some(note)
                    && desired[voice] == Some(note)
                    && retrigger & (1 << voice) == 0
            }) {
                return false;
            }
            // A literal keyboard note can share a caught pitch after an incompatible slide.
            let caught = (0..2)
                .any(|index| self.caught[index] == Some(note) && desired[index + 1] == Some(note));
            !caught
                && desired
                    .iter()
                    .enumerate()
                    .any(|(voice, pitch)| retrigger & (1 << voice) != 0 && *pitch == Some(note))
        };
        for (index, &note) in self.sounding.iter().enumerate() {
            if let Some(note) = note {
                if !self.sounding[..index].contains(&Some(note))
                    && (!desired.contains(&Some(note)) || forced(note))
                {
                    emit(MidiEvent::NoteOff {
                        channel: self.channel,
                        note,
                    });
                }
            }
        }
        for (index, &note) in desired.iter().enumerate() {
            if let Some(note) = note {
                if !desired[..index].contains(&Some(note))
                    && (!self.sounding.contains(&Some(note)) || forced(note))
                {
                    emit(MidiEvent::NoteOn {
                        channel: self.channel,
                        note,
                        velocity: self.velocity,
                    });
                }
            }
        }
        self.sounding = desired;
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test assertions")]
mod tests {
    extern crate std;
    use super::*;
    use std::{collections::BTreeSet, vec::Vec};

    fn program() -> AutoChord {
        AutoChord::new(Key::C_MAJOR, 0, 100).unwrap()
    }
    fn frame(bass: u8, melody: u8) -> AutoChordInput {
        AutoChordInput {
            bass: Some(bass),
            melody: Some(melody),
            gates: 15,
            ..AutoChordInput::default()
        }
    }
    fn update(program: &mut AutoChord, input: AutoChordInput) -> Vec<MidiEvent> {
        let mut events = Vec::new();
        program
            .update(input, &mut |event| events.push(event))
            .unwrap();
        events
    }

    #[test]
    fn validates_configuration_and_invalid_input_resets_every_owned_note() {
        assert_eq!(
            AutoChord::new(Key::C_MAJOR, 16, 100).err(),
            Some(ConfigError::Channel(16))
        );
        for velocity in [0, 128, 255] {
            assert_eq!(
                AutoChord::new(Key::C_MAJOR, 0, velocity).err(),
                Some(ConfigError::Velocity(velocity))
            );
        }
        for (invalid, error) in [
            (
                AutoChordInput {
                    bass: Some(128),
                    ..frame(36, 72)
                },
                InputError::Note(128),
            ),
            (
                AutoChordInput {
                    bass: Some(37),
                    ..frame(36, 72)
                },
                InputError::OutsideKey(37),
            ),
            (
                AutoChordInput {
                    gates: 16,
                    ..frame(36, 72)
                },
                InputError::Gates(16),
            ),
            (
                AutoChordInput {
                    retrigger: 16,
                    ..frame(36, 72)
                },
                InputError::Retrigger(16),
            ),
        ] {
            let mut p = program();
            update(&mut p, frame(36, 72));
            let mut events = Vec::new();
            assert_eq!(
                p.update(invalid, &mut |event| events.push(event)),
                Err(error)
            );
            assert!(events
                .iter()
                .all(|e| matches!(e, MidiEvent::NoteOff { .. })));
            assert_eq!(p.sounding(), [None; 4]);
            assert_eq!(p.history(), None);
        }
    }

    #[test]
    fn repeated_impulses_are_consumed_but_holds_and_releases_do_not_repeat_them() {
        let mut p = program();
        let input = frame(36, 64);
        update(&mut p, input);
        for _ in 0..8 {
            let before = p.sounding();
            assert!(!update(
                &mut p,
                AutoChordInput {
                    command: Some(HarmonyCommand::Reharm),
                    ..input
                }
            )
            .is_empty());
            assert_ne!(p.sounding(), before);
            assert!(update(&mut p, input).is_empty());
            assert!(update(&mut p, input).is_empty());
            assert_eq!(p.sounding()[0], Some(36));
            assert_eq!(p.sounding()[3], Some(64));
        }
    }

    #[test]
    fn shared_pitch_has_one_attack_and_one_release_across_voice_gates() {
        let mut p = program();
        let input = AutoChordInput {
            bass: Some(60),
            melody: Some(60),
            gates: 9,
            ..AutoChordInput::default()
        };
        assert_eq!(update(&mut p, input).len(), 1);
        assert!(update(&mut p, AutoChordInput { gates: 8, ..input }).is_empty());
        assert_eq!(update(&mut p, AutoChordInput::default()).len(), 1);
    }

    #[test]
    fn rests_keep_history_and_stop_clears_it_and_catches() {
        let mut p = program();
        let input = frame(36, 64);
        update(&mut p, input);
        let mut old = p.history().unwrap();
        update(&mut p, AutoChordInput::default());
        old.pedal_prepared = None;
        assert_eq!(p.history(), Some(old));
        update(
            &mut p,
            AutoChordInput {
                hold: [true; 2],
                ..input
            },
        );
        p.reset(&mut |_| {});
        assert_eq!(p.sounding(), [None; 4]);
        assert_eq!(p.history(), None);
        assert_eq!(p.caught, [None; 2]);
    }

    #[test]
    fn incompatible_note_slide_preserves_only_caught_inner_notes_and_history() {
        let mut p = program();
        let input = frame(41, 72);
        update(&mut p, input);
        update(
            &mut p,
            AutoChordInput {
                hold: [true, false],
                ..input
            },
        );
        let history = p.history();
        let tenor = p.sounding()[1];
        assert_eq!(tenor, Some(57));
        update(
            &mut p,
            AutoChordInput {
                hold: [true, false],
                ..frame(47, 60)
            },
        );
        assert_eq!(p.solution(), None);
        assert_eq!(p.history(), history);
        assert_eq!(p.sounding(), [Some(47), tenor, None, Some(60)]);
    }

    #[test]
    fn caught_pitch_shared_with_a_keyboard_note_is_never_rearticulated() {
        let mut p = program();
        update(&mut p, frame(36, 64));
        assert_eq!(p.sounding()[2], Some(60));
        update(
            &mut p,
            AutoChordInput {
                hold: [false, true],
                ..frame(36, 64)
            },
        );
        for _ in 0..8 {
            let events = update(
                &mut p,
                AutoChordInput {
                    hold: [false, true],
                    command: Some(HarmonyCommand::Reharm),
                    retrigger: 8,
                    ..frame(36, 60)
                },
            );
            assert!(events.iter().all(|event| match event {
                MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } => *note != 36,
                _ => false,
            }));
            assert!(events.iter().all(|event| match event {
                MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } => *note != 60,
                _ => false,
            }));
            assert_eq!(p.sounding()[2..], [Some(60), Some(60)]);
        }
    }

    #[test]
    fn a_catch_arms_before_notes_and_never_filters_out_a_chromatic_pitch() {
        let mut p = program();
        update(
            &mut p,
            AutoChordInput {
                hold: [true; 2],
                ..AutoChordInput::default()
            },
        );
        let input = AutoChordInput {
            hold: [true; 2],
            ..frame(36, 64)
        };
        update(&mut p, input);
        assert_eq!(p.caught, [p.sounding()[1], p.sounding()[2]]);
        p.reset(&mut |_| {});
        let input = frame(36, 64);
        update(&mut p, input);
        for _ in 0..4 {
            let notes = p.sounding();
            if notes[1..3]
                .iter()
                .flatten()
                .any(|n| !Key::C_MAJOR.contains(*n))
            {
                break;
            }
            update(
                &mut p,
                AutoChordInput {
                    command: Some(HarmonyCommand::Tension),
                    ..input
                },
            );
        }
        let notes = p.sounding();
        assert!(notes[1..3]
            .iter()
            .flatten()
            .any(|n| !Key::C_MAJOR.contains(*n)));
        update(
            &mut p,
            AutoChordInput {
                hold: [true; 2],
                ..input
            },
        );
        assert_eq!(p.caught, [notes[1], notes[2]]);
    }

    #[test]
    fn arming_catches_rechecks_the_saved_resolution_against_actual_catches() {
        for bass in [36, 48] {
            for hold in [[true, false], [false, true], [true, true]] {
                let mut p = program();
                let input = frame(bass, 64);
                update(&mut p, input);
                update(
                    &mut p,
                    AutoChordInput {
                        command: Some(HarmonyCommand::Tension),
                        steering: Steering::Lean,
                        ..input
                    },
                );
                assert!(p.solution().unwrap().resolution.is_some());
                let before = p.sounding();
                assert!(update(&mut p, AutoChordInput { hold, ..input }).is_empty());
                assert_eq!(p.sounding(), before);
                let plan = p.solution().unwrap();
                assert_eq!(p.history(), Some(plan));
                if let Some(destination) = plan.resolution {
                    let resolved = harmony::select(
                        Key::C_MAJOR,
                        bass,
                        64,
                        Some(plan),
                        p.caught,
                        HarmonyCommand::Resolve,
                    )
                    .unwrap();
                    assert_eq!(resolved.chord, destination);
                }

                // A catch armed before the next harmony must also be reflected in history.
                update(&mut p, AutoChordInput::default());
                update(&mut p, AutoChordInput { hold, ..input });
                let plan = p.solution().unwrap();
                assert_eq!(p.history(), Some(plan));
                if let Some(destination) = plan.resolution {
                    assert_eq!(
                        harmony::select(
                            Key::C_MAJOR,
                            bass,
                            64,
                            Some(plan),
                            p.caught,
                            HarmonyCommand::Resolve,
                        )
                        .unwrap()
                        .chord,
                        destination
                    );
                }
            }
        }
    }

    #[test]
    fn keyboard_motion_resolves_applied_tendencies_without_restriking_the_held_anchor() {
        for (bass, melody, unchanged) in [(45, 64, 64), (36, 69, 36)] {
            let mut p = program();
            let input = frame(36, 64);
            update(&mut p, input);
            update(
                &mut p,
                AutoChordInput {
                    command: Some(HarmonyCommand::Tension),
                    steering: Steering::Lean,
                    ..input
                },
            );
            assert_eq!(p.sounding()[1..3], [Some(56), Some(62)]);
            let events = update(&mut p, frame(bass, melody));
            let plan = p.solution().unwrap();
            assert_eq!(plan.chord.root, 9);
            assert_eq!(plan.chord.shape, harmony::Shape::Minor);
            assert_eq!(p.sounding()[1..3], [Some(57), Some(60)]);
            assert!(events.iter().all(|event| match event {
                MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } =>
                    *note != unchanged,
                _ => false,
            }));
            assert!(update(&mut p, frame(bass, melody)).is_empty());
        }
    }

    #[test]
    fn automatic_plagal_arrival_waits_for_each_catch_and_finishes_its_guidance() {
        for first in 0..2 {
            let mut p = program();
            let input = frame(41, 72);
            update(&mut p, input);
            assert_eq!(p.sounding()[1..3], [Some(57), Some(65)]);
            let mut held = [true; 2];
            update(
                &mut p,
                AutoChordInput {
                    hold: held,
                    ..input
                },
            );
            let arrival = frame(36, 72);
            update(
                &mut p,
                AutoChordInput {
                    hold: held,
                    ..arrival
                },
            );
            let pending = p.solution().unwrap();
            assert_eq!(pending.chord.root, 0);
            assert_eq!(pending.voicing.notes, [36, 55, 64, 72]);
            assert_eq!(pending.realized.notes, [36, 57, 65, 72]);
            assert_eq!(pending.suspended, [true; 2]);
            held[first] = false;
            let events = update(
                &mut p,
                AutoChordInput {
                    hold: held,
                    ..arrival
                },
            );
            let protected = pending.realized.notes[2 - first];
            assert!(events.iter().all(|event| match event {
                MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } =>
                    *note != protected,
                _ => false,
            }));
            assert_eq!(
                p.sounding()[first + 1],
                Some(pending.voicing.notes[first + 1])
            );
            update(&mut p, arrival);
            let settled = p.solution().unwrap();
            assert_eq!(settled.realized, pending.voicing);
            assert_eq!(settled.suspended, [false; 2]);
            assert_eq!(settled.guidance, super::super::HarmonicIntent::Continue);
            assert_eq!(p.history(), Some(settled));
        }
    }

    #[test]
    fn released_suspension_retains_its_new_leading_tone_resolution() {
        let mut p = program();
        update(&mut p, frame(36, 64));
        update(&mut p, frame(41, 65));
        assert_eq!(p.sounding()[2], Some(60));
        update(
            &mut p,
            AutoChordInput {
                hold: [false, true],
                ..frame(41, 65)
            },
        );
        update(
            &mut p,
            AutoChordInput {
                hold: [false, true],
                ..frame(43, 62)
            },
        );
        assert_eq!(p.sounding()[2], Some(60));
        update(&mut p, frame(43, 62));
        let dominant = p.solution().unwrap();
        assert_eq!(dominant.realized.notes[2], 59);
        assert_eq!(
            dominant.tendencies[1],
            Some(harmony::VoiceTendency { from: 59, to: 60 })
        );
        update(&mut p, frame(36, 72));
        let tonic = p.solution().unwrap();
        assert_eq!(tonic.chord.root, 0);
        assert_eq!(tonic.chord.shape, harmony::Shape::Major);
        assert_eq!(tonic.realized.notes[2], 60);
        assert!(tonic.tendencies.iter().all(Option::is_none));
    }

    #[test]
    fn releasing_thumb_steering_keeps_the_sound_and_its_resolution_debts() {
        let mut p = program();
        let input = frame(36, 67);
        update(&mut p, input);
        update(
            &mut p,
            AutoChordInput {
                command: Some(HarmonyCommand::Tension),
                steering: Steering::Lean,
                ..input
            },
        );
        let tension = p.solution().unwrap();
        assert_eq!(tension.chord.root, 7);
        assert_eq!(tension.chord.shape, harmony::Shape::Dominant7);
        assert_eq!(tension.pedal, Some(36));
        let sounding = p.sounding();
        assert!(update(&mut p, input).is_empty());
        assert_eq!(p.sounding(), sounding);
        let released = p.solution().unwrap();
        assert_eq!(released.guidance, super::super::HarmonicIntent::Continue);
        assert_eq!(released.tendency, tension.tendency);
        assert_eq!(released.tendencies, tension.tendencies);
        assert_eq!(p.history(), Some(released));
        update(&mut p, AutoChordInput::default());
        assert_eq!(p.history().unwrap().guidance, released.guidance);
        p.reset(&mut |_| {});
        assert_eq!(p.history(), None);
    }

    #[test]
    fn pending_inner_step_survives_idle_frames_and_an_unrelated_catch_release() {
        let mut p = program();
        update(&mut p, frame(36, 67));
        let input = frame(36, 72);
        update(&mut p, input);
        assert_eq!(p.sounding(), [Some(36), Some(53), Some(64), Some(72)]);
        let ornament = p.solution().unwrap().inner_ornaments[0].unwrap();
        assert_eq!((ornament.from, ornament.to), (53, 52));
        for _ in 0..8 {
            assert!(update(&mut p, input).is_empty());
            assert!(update(
                &mut p,
                AutoChordInput {
                    revoice: true,
                    ..input
                }
            )
            .is_empty());
        }
        for state in [
            AutoChordInput {
                hold: [false, true],
                ..input
            },
            input,
            AutoChordInput {
                steering: Steering::Lean,
                ..input
            },
            input,
        ] {
            assert!(update(&mut p, state).is_empty());
            assert_eq!(p.sounding()[1], Some(53));
            assert_eq!(p.solution().unwrap().inner_ornaments[0], Some(ornament));
        }
        update(
            &mut p,
            AutoChordInput {
                hold: [true, false],
                ..input
            },
        );
        update(
            &mut p,
            AutoChordInput {
                hold: [true, false],
                ..frame(36, 67)
            },
        );
        assert_eq!(p.sounding()[1], Some(53));
        let events = update(&mut p, frame(36, 67));
        assert_eq!(p.sounding()[1], Some(52));
        assert!(events.contains(&MidiEvent::NoteOff {
            channel: 0,
            note: 53,
        }));
        assert!(events.contains(&MidiEvent::NoteOn {
            channel: 0,
            note: 52,
            velocity: 100,
        }));
        assert!(events.iter().all(|event| match event {
            MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } => *note != 64,
            _ => false,
        }));
    }

    #[test]
    fn bass_pedal_preparation_requires_continuous_playing_across_melody_rests() {
        let mut p = program();
        let input = frame(36, 67);
        update(&mut p, input);
        assert_eq!(p.solution().unwrap().pedal_prepared, Some(36));
        update(
            &mut p,
            AutoChordInput {
                bass: Some(36),
                gates: 1,
                ..AutoChordInput::default()
            },
        );
        assert_eq!(p.history().unwrap().pedal_prepared, Some(36));
        update(&mut p, input);
        update(
            &mut p,
            AutoChordInput {
                command: Some(HarmonyCommand::Tension),
                steering: Steering::Lean,
                ..input
            },
        );
        assert_eq!(p.solution().unwrap().pedal, Some(36));
        update(&mut p, AutoChordInput::default());
        assert_eq!(p.history().unwrap().pedal_prepared, None);
        update(
            &mut p,
            AutoChordInput {
                command: Some(HarmonyCommand::Tension),
                steering: Steering::Lean,
                ..input
            },
        );
        assert_eq!(p.solution().unwrap().pedal, None);
        assert_eq!([p.sounding()[0], p.sounding()[3]], [Some(36), Some(67)]);
    }

    #[test]
    fn a_rest_releases_an_ornament_instance_but_keeps_its_musical_endpoint() {
        let mut p = program();
        update(&mut p, frame(36, 67));
        let input = frame(36, 72);
        update(&mut p, input);
        assert_eq!(p.sounding()[1], Some(53));
        let debt = p.solution().unwrap().tendencies[0];
        assert_eq!(debt, Some(harmony::VoiceTendency { from: 53, to: 52 }));
        update(&mut p, AutoChordInput::default());
        assert_eq!(p.sounding(), [None; 4]);
        let history = p.history().unwrap();
        assert_eq!(history.inner_ornaments, [None; 2]);
        assert_eq!(history.tendencies[0], debt);
        update(&mut p, input);
        assert_eq!(p.sounding()[1], Some(52));
    }

    #[test]
    fn direct_bars_change_anchor_interpretation_and_release_without_another_attack() {
        for voice in [AnchorVoice::Bass, AnchorVoice::Melody] {
            let mut p = program();
            let input = frame(36, 64);
            update(&mut p, input);
            let first = p.solution().unwrap();
            update(
                &mut p,
                AutoChordInput {
                    command: Some(HarmonyCommand::Reinterpret(voice)),
                    interpretation: Some(voice),
                    ..input
                },
            );
            let reinterpreted = p.solution().unwrap();
            assert_ne!(reinterpreted.chord.root, first.chord.root);
            assert_ne!(reinterpreted.realized, first.realized);
            assert_eq!([p.sounding()[0], p.sounding()[3]], [Some(36), Some(64)]);
            assert!(update(&mut p, input).is_empty());
            assert_eq!(p.solution().unwrap(), reinterpreted);
        }
    }

    #[test]
    fn harmonic_permissions_have_quiet_edges_and_do_not_latch_after_release() {
        use harmony::Shape;
        for (borrowing, extensions) in [(true, false), (false, true), (true, true)] {
            let mut p = program();
            let input = frame(36, 64);
            update(&mut p, input);
            let first = p.solution().unwrap();
            let permitted = AutoChordInput {
                borrowing,
                extensions,
                ..input
            };
            assert!(update(&mut p, permitted).is_empty());
            assert_eq!(p.solution(), Some(first));
            for _ in 0..4 {
                update(
                    &mut p,
                    AutoChordInput {
                        command: Some(HarmonyCommand::Substitute),
                        steering: Steering::Substitute,
                        ..permitted
                    },
                );
            }
            let rich = p.solution().unwrap();
            let sounding = p.sounding();
            assert!(update(&mut p, input).is_empty());
            assert_eq!(p.sounding(), sounding);
            let released = p.solution().unwrap();
            assert_eq!(released.chord, rich.chord);
            assert_eq!(released.realized, rich.realized);
            assert_eq!(released.tendencies, rich.tendencies);
            assert_eq!(released.tendency, rich.tendency);
            update(
                &mut p,
                AutoChordInput {
                    command: Some(HarmonyCommand::Substitute),
                    steering: Steering::Substitute,
                    ..input
                },
            );
            assert!(!matches!(
                p.solution().unwrap().chord.shape,
                Shape::Sus2
                    | Shape::Sus4
                    | Shape::DominantSus4
                    | Shape::Dominant13
                    | Shape::Major13
                    | Shape::DominantFlat9
                    | Shape::DominantSharp9
            ));
        }
    }

    #[test]
    fn harmony_commands_articulate_only_the_inner_voices_of_a_held_line() {
        for command in [
            HarmonyCommand::Prepare,
            HarmonyCommand::Tension,
            HarmonyCommand::Resolve,
            HarmonyCommand::Substitute,
            HarmonyCommand::Reinterpret(AnchorVoice::Bass),
            HarmonyCommand::Reinterpret(AnchorVoice::Melody),
        ] {
            let mut p = program();
            update(&mut p, frame(36, 64));
            update(&mut p, frame(36, 65));
            let events = update(
                &mut p,
                AutoChordInput {
                    command: Some(command),
                    ..frame(36, 65)
                },
            );
            assert!(
                events.iter().all(|event| match event {
                    MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } =>
                        ![36, 65].contains(note),
                    _ => false,
                }),
                "outer attack from {command:?}: {events:?}"
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, MidiEvent::NoteOn { .. }))
                    .count(),
                2
            );
            assert_eq!([p.sounding()[0], p.sounding()[3]], [Some(36), Some(65)]);
            assert!(update(&mut p, frame(36, 65)).is_empty());
        }
    }

    #[test]
    fn established_guides_support_a_literal_thirteenth_and_a_repeated_arrival() {
        let mut p = program();
        update(&mut p, frame(36, 67));
        update(
            &mut p,
            AutoChordInput {
                command: Some(HarmonyCommand::Tension),
                steering: Steering::Lean,
                ..frame(36, 67)
            },
        );
        assert!(update(&mut p, frame(36, 67)).is_empty());
        let dominant = p.solution().unwrap();
        assert_eq!(dominant.realized.notes, [36, 59, 65, 67]);
        let moved = update(&mut p, frame(36, 76));
        let extended = p.solution().unwrap();
        assert_eq!(extended.realized.notes, [36, 59, 65, 76]);
        assert!(extended.root_implied);
        assert_eq!(extended.tendency, dominant.tendency);
        assert_eq!(extended.tendencies, dominant.tendencies);
        assert_eq!(
            moved,
            [
                MidiEvent::NoteOff {
                    channel: 0,
                    note: 67
                },
                MidiEvent::NoteOn {
                    channel: 0,
                    note: 76,
                    velocity: 100
                },
            ]
        );
        let repeated = update(
            &mut p,
            AutoChordInput {
                retrigger: 8,
                ..frame(36, 76)
            },
        );
        assert_eq!(p.solution().unwrap().realized.notes, [36, 60, 64, 76]);
        assert!(!p.solution().unwrap().root_implied);
        assert!(repeated.contains(&MidiEvent::NoteOn {
            channel: 0,
            note: 76,
            velocity: 100
        }));
        assert!(repeated.iter().all(|event| match event {
            MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } => *note != 36,
            _ => false,
        }));
    }

    #[test]
    fn timed_flow_and_emphasis_change_harmonic_authority_only_at_a_musical_event() {
        use harmony::HarmonicEvent;
        let timed = |melody, now_ms| AutoChordInput {
            now_ms: Some(now_ms),
            ..frame(36, melody)
        };
        let mut flowing = program();
        update(&mut flowing, timed(64, 0));
        let home = flowing.solution().unwrap();
        assert_eq!(
            flowing.harmonic_event(timed(65, 100), true),
            HarmonicEvent::Continue
        );
        update(&mut flowing, timed(65, 100));
        assert_eq!(flowing.solution().unwrap().chord, home.chord);
        let heard = flowing.solution();
        assert!(update(&mut flowing, timed(65, 2000)).is_empty());
        assert_eq!(flowing.solution(), heard);
        assert_eq!(
            flowing.harmonic_event(timed(64, 2100), true),
            HarmonicEvent::Arrive
        );
        update(&mut flowing, timed(64, 2100));
        assert_eq!(flowing.solution().unwrap().chord, home.chord);

        let mut spaced = program();
        update(&mut spaced, timed(64, 0));
        assert_eq!(
            spaced.harmonic_event(timed(65, 700), true),
            HarmonicEvent::Structural
        );
        update(&mut spaced, timed(65, 700));
        let structural = spaced.solution().unwrap();
        assert!(structural.ornament.is_none());
        assert!(structural.chord.tones().contains(65));
        assert_ne!(structural.chord, home.chord);

        let repeated = AutoChordInput {
            retrigger: 8,
            ..timed(65, 2300)
        };
        update(&mut flowing, timed(65, 2200));
        assert_eq!(
            flowing.harmonic_event(repeated, true),
            HarmonicEvent::Structural
        );
        update(&mut flowing, repeated);
        assert!(flowing.solution().unwrap().ornament.is_none());
        assert_ne!(flowing.solution().unwrap().chord, home.chord);
    }

    #[test]
    fn timing_regression_is_safe_and_stop_discards_performance_evidence() {
        let mut p = program();
        update(
            &mut p,
            AutoChordInput {
                now_ms: Some(u64::MAX),
                ..frame(36, 64)
            },
        );
        update(
            &mut p,
            AutoChordInput {
                now_ms: Some(0),
                ..frame(36, 65)
            },
        );
        assert!(p.solution().unwrap().ornament.is_some());
        p.reset(&mut |_| {});
        assert!(p.melody_attacks.iter().all(Option::is_none));
        update(
            &mut p,
            AutoChordInput {
                now_ms: Some(1),
                ..frame(36, 65)
            },
        );
        assert!(p.solution().unwrap().ornament.is_none());
    }

    #[test]
    fn melodic_neighbours_keep_the_lower_voices_and_only_rearticulate_the_melody() {
        let mut p = program();
        update(&mut p, frame(36, 64));
        let initial = p.sounding();
        let chord = p.solution().unwrap().chord;
        let mut previous = 64;
        for melody in [65, 64, 62, 64] {
            let input = frame(36, melody);
            assert_eq!(
                update(&mut p, input),
                [
                    MidiEvent::NoteOff {
                        channel: 0,
                        note: previous,
                    },
                    MidiEvent::NoteOn {
                        channel: 0,
                        note: melody,
                        velocity: 100,
                    },
                ]
            );
            assert_eq!(&p.sounding()[..3], &initial[..3]);
            assert_eq!(p.solution().unwrap().chord, chord);
            assert_eq!(p.sounding()[3], Some(melody));
            assert!(update(&mut p, input).is_empty());
            previous = melody;
        }
    }

    #[test]
    fn gesture_stream_preserves_exact_keyboards_catches_and_midi_ownership() {
        let mut p = program();
        let mut active = BTreeSet::new();
        let mut seed = 0x8c54_1a39u32;
        let mut random = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let basses = [36, 38, 40, 41, 43, 45, 47, 48];
        let melodies = [60, 62, 64, 65, 67, 69, 71, 72, 74, 76];
        let commands = [
            None,
            Some(HarmonyCommand::Relax),
            Some(HarmonyCommand::Tension),
            Some(HarmonyCommand::Resolve),
            Some(HarmonyCommand::Reharm),
            Some(HarmonyCommand::Prepare),
            Some(HarmonyCommand::Substitute),
            Some(HarmonyCommand::Reinterpret(AnchorVoice::Bass)),
            Some(HarmonyCommand::Reinterpret(AnchorVoice::Melody)),
            Some(HarmonyCommand::Role {
                voice: AnchorVoice::Bass,
                role: super::super::ToneRole::Third,
            }),
        ];
        for i in 0..4000 {
            let input = AutoChordInput {
                bass: Some(basses[random() as usize % basses.len()]),
                melody: Some(melodies[random() as usize % melodies.len()]),
                extra_tones: PitchClassSet::from_bits(random() as u16),
                gates: (random() as u8) & 15,
                hold: [random() & 1 != 0, random() & 1 != 0],
                revoice: true,
                retrigger: (random() as u8) & 15,
                command: commands[random() as usize % commands.len()],
                steering: [
                    Steering::Neutral,
                    Steering::Ease,
                    Steering::Lean,
                    Steering::Arrive,
                    Steering::Prepare,
                    Steering::Substitute,
                ][random() as usize % 6],
                interpretation: [None, Some(AnchorVoice::Bass), Some(AnchorVoice::Melody)]
                    [random() as usize % 3],
                borrowing: random() & 1 != 0,
                extensions: random() & 1 != 0,
                now_ms: Some(i * 73),
            };
            let catches = p.caught;
            let previous = p.solution();
            let events = if i % 97 == 0 {
                let mut e = Vec::new();
                p.reset(&mut |event| e.push(event));
                e
            } else {
                update(&mut p, input)
            };
            let mut attacked = false;
            for event in events {
                match event {
                    MidiEvent::NoteOff { note, .. } => {
                        assert!(!attacked);
                        assert!(active.remove(&note));
                    }
                    MidiEvent::NoteOn { note, .. } => {
                        attacked = true;
                        assert!(active.insert(note));
                    }
                    _ => panic!("unexpected MIDI event"),
                }
            }
            assert_eq!(active, p.sounding().into_iter().flatten().collect());
            assert!(active.len() <= 4);
            if i % 97 != 0 {
                if input.gates & 1 != 0 {
                    assert_eq!(p.sounding()[0], input.bass);
                }
                if input.gates & 8 != 0 {
                    assert_eq!(p.sounding()[3], input.melody);
                }
                for (index, catch) in catches.into_iter().enumerate() {
                    if input.hold[index] && input.gates & (1 << (index + 1)) != 0 && catch.is_some()
                    {
                        assert_eq!(p.sounding()[index + 1], catch, "stolen catch at {i}");
                    }
                }
            }
            if let Some(plan) = p.solution() {
                assert_eq!(Some(plan.voicing.notes[0]), input.bass);
                assert_eq!(Some(plan.voicing.notes[3]), input.melody);
                assert!(
                    harmony::valid_caught_realization(
                        Key::C_MAJOR,
                        plan.chord,
                        plan.realized,
                        p.caught,
                    ),
                    "invalid realization at {i}: {input:?}, catches {:?}, previous {previous:?}, plan {plan:?}",
                    p.caught,
                );
            }
        }
        let mut events = Vec::new();
        p.reset(&mut |e| events.push(e));
        for event in events {
            if let MidiEvent::NoteOff { note, .. } = event {
                assert!(active.remove(&note));
            }
        }
        assert!(active.is_empty());
    }
}
