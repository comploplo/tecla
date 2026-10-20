use tecla::matrix::MatrixState;
use tecla::music::realization::{
    Colour, Diagnostics, Direction, Foundation, Motion, Preferences, Realizer, Reharm,
};
use tecla::music::{Key, PitchClassSet};
use tecla::pipeline::report::MidiEvent;
use tecla::rgb::{hsv_to_rgb, Hsv, Rgb};

use super::instrument::colours::{self, NoteState};
use super::instrument::{
    base_note, Orientation, CHANNEL, LEDS, OCTAVE_DOWN, OCTAVE_UP, POSITIONS, STOP,
};

/// Right inner/back thumb: articulate the current realization without replanning.
pub const RESTRIKE: usize = 50;
/// Right outer/front thumb: realize the prepared destination.
pub const ARRIVE: usize = 51;

const CONTROL_ORDER: [u8; 28] = [
    24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
    48, 49, 50, 51,
];

/// Stable musical responsibilities in the right-hand physical grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Characteristic added tones, independent of register and harmony.
    Colour(Colour),
    /// Chord-member role of the lowest literal left-hand pitch.
    Foundation(Foundation),
    /// Directed movement of generated inner voices.
    Motion(Motion),
    /// Relationship between the source harmony and its new interpretation.
    Reharm(Reharm),
    /// Establish a predominant approach.
    Prepare,
    /// Establish a directed harmonic destination.
    Lean,
    /// Retain a prepared inner voice until released.
    Delay,
    /// Fulfil the saved destination and release delayed resolutions.
    Resolve,
    /// Request fresh inner motion while continuing the current musical thought.
    Continue,
    /// Permit a more adventurous substitution for this gesture.
    Substitute,
    /// Retain the current generated bass while other voices and harmony change.
    Pedal,
    /// Divert the expected arrival toward its related deceptive destination.
    Deceive,
}

/// Musical control at a right-hand LED position; all other positions return None.
pub const fn control(led: usize) -> Option<Control> {
    if led < 26 || led >= 50 {
        return None;
    }
    let local = led - 26;
    let row = local / 6;
    match local % 6 {
        1 => Some(Control::Colour(
            [
                Colour::Extended,
                Colour::Seventh,
                Colour::Sixth,
                Colour::SecondsFourths,
            ][row],
        )),
        2 => Some(Control::Foundation(
            [
                Foundation::Seventh,
                Foundation::Root,
                Foundation::Fifth,
                Foundation::Third,
            ][row],
        )),
        3 => Some(Control::Motion(
            [
                Motion::Outward,
                Motion::Inward,
                Motion::Rising,
                Motion::Falling,
            ][row],
        )),
        4 => Some(Control::Reharm(
            [
                Reharm::AppliedDominant,
                Reharm::Borrowed,
                Reharm::CommonTone,
                Reharm::Relative,
            ][row],
        )),
        0 => Some(match row {
            0 => Control::Prepare,
            1 => Control::Delay,
            2 => Control::Continue,
            _ => Control::Pedal,
        }),
        _ => Some(match row {
            0 => Control::Lean,
            1 => Control::Resolve,
            2 => Control::Substitute,
            _ => Control::Deceive,
        }),
    }
}

/// Flags describing the last physical or output-readiness change.
pub mod cause {
    /// At least one left-hand note key was physically pressed.
    pub const MANUAL_ATTACK: u16 = 1 << 0;
    /// At least one left-hand note key was physically released.
    pub const MANUAL_RELEASE: u16 = 1 << 1;
    /// A right-hand control or octave thumb was physically pressed.
    pub const RHS_ATTACK: u16 = 1 << 2;
    /// A right-hand control or octave thumb was physically released.
    pub const RHS_RELEASE: u16 = 1 << 3;
    /// An accepted Delay gesture ended.
    pub const DELAY_RELEASE: u16 = 1 << 4;
    /// An accepted Restrike attack articulated the current realization.
    pub const RESTRIKE: u16 = 1 << 5;
    /// The generated bass pedal was pressed or released.
    pub const PEDAL: u16 = 1 << 6;
    /// A deceptive arrival was requested.
    pub const DECEIVE: u16 = 1 << 7;
    /// Output or scanner readiness changed, including recovery.
    pub const RECOVERY: u16 = 1 << 8;
    /// The harmonic planner ran during this event, including a silent breath.
    pub const REPLAN: u16 = 1 << 9;
    /// Stop was held while this change was consumed.
    pub const STOP: u16 = 1 << 10;
}

/// One allocation-free observation of the complete keyed musical state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// Advances once per physical-key or readiness change; unchanged scans keep it.
    pub sequence: u32,
    /// Firmware time of that change, in milliseconds.
    pub time_ms: u64,
    /// Whether the runtime marked the output and scanner ready for this scan.
    pub ready: bool,
    /// Combination of the flags in [`cause`].
    pub cause: u16,
    /// Physical keys in LED order, low 52 bits only.
    pub physical: u64,
    /// Accepted momentary controls in LED order; suppressed controls stay clear.
    pub accepted: u64,
    /// Left-hand attack-time pitches indexed by physical LED; Stop has no pitch.
    pub captured: [Option<u8>; 24],
    /// Actual supporting pitches, including prepared delayed notes.
    pub support: [Option<u8>; 6],
    /// Preferences requested by the currently accepted physical controls.
    pub requested: Preferences,
    /// Planner state; `applied` can differ after a quiet control release.
    pub diagnostics: Diagnostics,
}

/// A literal polyphonic Tonnetz with explicitly played harmonic intentions.
pub struct HarmonyInstrument {
    previous: [bool; LEDS],
    accepted: [bool; LEDS],
    captured: [Option<u8>; 24],
    sounding: [bool; 128],
    support: [Option<u8>; 6],
    realizer: Realizer,
    sequence: u32,
    event_ms: u64,
    event_cause: u16,
    ready: bool,
    control_order: [u8; 28],
    event_direction: Direction,
}

impl HarmonyInstrument {
    /// Starts in C major with restrained, faithful, grounded preferences.
    pub const fn new() -> Self {
        Self {
            previous: [false; LEDS],
            accepted: [false; LEDS],
            captured: [None; 24],
            sounding: [false; 128],
            support: [None; 6],
            realizer: Realizer::new(Key::C_MAJOR),
            sequence: 0,
            event_ms: 0,
            event_cause: 0,
            ready: false,
            control_order: CONTROL_ORDER,
            event_direction: Direction::Continue,
        }
    }

    /// Copies the current observation without planning, emitting MIDI, or advancing music.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            sequence: self.sequence,
            time_ms: self.event_ms,
            ready: self.ready,
            cause: self.event_cause,
            physical: key_mask(&self.previous),
            accepted: key_mask(&self.accepted),
            captured: self.captured,
            support: self.support,
            requested: Preferences {
                direction: self.event_direction,
                ..self.preferences()
            },
            diagnostics: self.realizer.diagnostics(),
        }
    }

    /// Consumes a debounced scan. Time alone never advances music.
    ///
    /// Stop and unavailable output consume held gestures, so every note and
    /// modifier needs a fresh physical press after recovery. Normal steering
    /// never changes or rearticulates a held literal note. Only the explicitly
    /// labelled Restrike thumb rearticulates the whole current realization.
    pub fn update_at(
        &mut self,
        stable: &MatrixState<12, 7>,
        now_ms: u64,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) {
        let pressed = POSITIONS.map(|(row, col)| stable.get(row, col).unwrap_or(false));
        let changed = pressed != self.previous || ready != self.ready;
        if changed {
            self.sequence = self.sequence.wrapping_add(1);
            self.event_ms = now_ms;
            self.event_cause = 0;
            self.event_direction = Direction::Continue;
            if ready != self.ready {
                self.event_cause |= cause::RECOVERY;
            }
            for (led, (&held, &previous)) in pressed.iter().zip(&self.previous).enumerate() {
                if held == previous {
                    continue;
                }
                if (1..24).contains(&led) {
                    self.event_cause |= if held {
                        cause::MANUAL_ATTACK
                    } else {
                        cause::MANUAL_RELEASE
                    };
                } else if led >= 24 {
                    self.event_cause |= if held {
                        cause::RHS_ATTACK
                    } else {
                        cause::RHS_RELEASE
                    };
                }
            }
            if pressed[STOP] {
                self.event_cause |= cause::STOP;
            }
        }
        self.ready = ready;
        if !ready || pressed[STOP] {
            self.release_all(emit);
            self.previous = pressed;
            return;
        }

        let previous_preferences = self.preferences();
        let mut rising = [false; LEDS];
        for led in 24..LEDS {
            if !pressed[led] {
                self.accepted[led] = false;
            } else if !self.previous[led] {
                self.accepted[led] = true;
                rising[led] = true;
            }
        }
        // Fresh gestures override older held gestures within their own family.
        // Simultaneous alternatives choose the physically uppermost pad.
        for led in (24..LEDS).rev().filter(|&led| rising[led]) {
            if let Some(position) = self
                .control_order
                .iter()
                .position(|&key| usize::from(key) == led)
            {
                self.control_order[position..].rotate_left(1);
            }
        }

        if previous_preferences.pedal != self.accepted[44] {
            self.event_cause |= cause::PEDAL;
        }
        if rising[49] {
            self.event_cause |= cause::DECEIVE;
        }

        let mut attacks = [false; 128];
        let mut manual_changed = false;
        let offset = self.octave_offset();
        for (led, &held) in pressed.iter().enumerate().take(24).skip(1) {
            if !held {
                manual_changed |= self.captured[led].take().is_some();
            } else if !self.previous[led] {
                if let Some(base) = base_note(Orientation::Mirrored, led) {
                    let note = (i16::from(base) + i16::from(offset)) as u8;
                    self.captured[led] = Some(note);
                    attacks[usize::from(note)] = true;
                    manual_changed = true;
                }
            }
        }

        let mut held = [0; 23];
        let mut count = 0;
        for note in self.captured.iter().flatten() {
            held[count] = *note;
            count += 1;
        }
        // Pitch order, not electrical scan order, defines bass and upper voices.
        held[..count].sort_unstable();
        let mut preferences = self.preferences();
        preferences.direction = if rising[ARRIVE] || rising[37] {
            Direction::Arrive
        } else if rising[49] {
            Direction::Deceive
        } else if rising[31] {
            Direction::Lean
        } else if rising[26] {
            Direction::Prepare
        } else if rising[43] {
            Direction::Substitute
        } else if rising[38] {
            Direction::Explore
        } else {
            Direction::Continue
        };
        if changed {
            self.event_direction = preferences.direction;
        }
        let steering_attack = rising[26..50].iter().any(|pressed| *pressed) || rising[ARRIVE];
        if count == 0 {
            self.support.fill(None);
            if manual_changed
                || previous_preferences.pedal != preferences.pedal
                || previous_preferences.delay && !preferences.delay
            {
                self.realizer.plan(&[], preferences);
                self.event_cause |= cause::REPLAN;
            }
        } else if manual_changed
            || steering_attack
            || previous_preferences.delay && !preferences.delay
            || previous_preferences.pedal && !preferences.pedal
        {
            self.support = self.realizer.plan(&held[..count], preferences).notes;
            self.event_cause |= cause::REPLAN;
        }
        if previous_preferences.delay && !preferences.delay {
            self.event_cause |= cause::DELAY_RELEASE;
        }

        let mut desired = [false; 128];
        for note in self
            .captured
            .iter()
            .chain(self.support.iter())
            .flatten()
            .copied()
        {
            desired[usize::from(note)] = true;
        }
        // A harmonic tap is an attack, even when good voice leading keeps a pitch.
        if steering_attack {
            let suspended = self.realizer.current().suspended;
            for (voice, note) in self.support.iter().enumerate() {
                if let Some(note) = note {
                    if !suspended[voice]
                        && !(voice == 0 && preferences.pedal && previous_preferences.pedal)
                        && !self.captured.contains(&Some(*note))
                    {
                        attacks[usize::from(*note)] = true;
                    }
                }
            }
        }
        if rising[RESTRIKE] {
            attacks = desired;
            self.event_cause |= cause::RESTRIKE;
        }
        for (note, &was_sounding) in self.sounding.iter().enumerate() {
            if was_sounding && (!desired[note] || attacks[note]) {
                emit(MidiEvent::NoteOff {
                    channel: CHANNEL,
                    note: note as u8,
                });
            }
        }
        for (note, &wanted) in desired.iter().enumerate() {
            if wanted && (!self.sounding[note] || attacks[note]) {
                emit(MidiEvent::NoteOn {
                    channel: CHANNEL,
                    note: note as u8,
                    velocity: 88,
                });
            }
        }
        self.sounding = desired;
        self.previous = pressed;
    }

    /// Colours expose literal pitches, sounding support, destination and gestures.
    pub fn frame(&self) -> [Rgb; LEDS] {
        let current = self.realizer.current();
        let mut held = [false; 128];
        let mut held_classes = PitchClassSet::default();
        for note in self.captured.iter().flatten().copied() {
            held[usize::from(note)] = true;
            held_classes.insert(note);
        }
        let mut support = [false; 128];
        let mut support_classes = PitchClassSet::default();
        for note in self.support.iter().flatten() {
            support[usize::from(*note)] = true;
            support_classes.insert(*note);
        }
        let destination = current.destination.map(|chord| chord.tones());
        let active = self.captured.iter().any(Option::is_some);
        let suspended = current.suspended.into_iter().any(|held| held);
        let can_resolve = active && (destination.is_some() || suspended);
        let mut frame = [Rgb::BLACK; LEDS];
        let offset = self.octave_offset();
        for (led, colour) in frame.iter_mut().enumerate().take(24).skip(1) {
            if let Some(base) = base_note(Orientation::Mirrored, led) {
                if let Some(note) = self.captured[led] {
                    *colour = colours::note(note, NoteState::Held);
                } else {
                    let note = (i16::from(base) + i16::from(offset)) as u8;
                    let state = if held[usize::from(note)] {
                        NoteState::ExactHeld
                    } else if held_classes.contains(note) {
                        NoteState::OctaveHeld
                    } else if support[usize::from(note)] {
                        NoteState::GeneratedExact
                    } else if support_classes.contains(note) {
                        NoteState::GeneratedOctave
                    } else if active && destination.is_some_and(|pcs| pcs.contains(note)) {
                        NoteState::Hint
                    } else {
                        NoteState::Idle
                    };
                    *colour = colours::note(note, state);
                }
            }
        }
        let prefs = self.preferences();
        let diagnostics = self.realizer.diagnostics();
        for (led, colour) in frame.iter_mut().enumerate().take(50).skip(26) {
            if let Some(control) = control(led) {
                let (hue, selected, applied, family) = match control {
                    Control::Colour(choice) => (
                        16,
                        prefs.colour == choice,
                        diagnostics.applied.colour == choice,
                        1,
                    ),
                    Control::Foundation(choice) => (
                        128,
                        prefs.foundation == choice,
                        diagnostics.applied.foundation == choice,
                        2,
                    ),
                    Control::Motion(choice) => (
                        85,
                        prefs.motion == choice,
                        diagnostics.applied.motion == choice,
                        4,
                    ),
                    Control::Reharm(choice) => (
                        192,
                        prefs.reharm == choice,
                        diagnostics.applied.reharm == choice,
                        8,
                    ),
                    Control::Prepare | Control::Continue => (85, true, false, 0),
                    Control::Delay => (128, true, suspended, 0),
                    Control::Lean => (16, true, false, 0),
                    Control::Resolve => (85, true, can_resolve, 0),
                    Control::Substitute | Control::Deceive => (192, true, false, 0),
                    Control::Pedal => (128, true, false, 0),
                };
                let limited = active && applied && diagnostics.limited & family != 0;
                let value = if self.accepted[led] && selected {
                    144
                } else if self.accepted[led] {
                    64
                } else if active && applied {
                    80
                } else {
                    32
                };
                let saturation = if limited {
                    210
                } else if matches!(control, Control::Delay) && suspended {
                    48
                } else if self.accepted[led] && selected {
                    120
                } else {
                    210
                };
                *colour = hsv_to_rgb(Hsv::new(if limited { 24 } else { hue }, saturation, value));
            }
        }
        frame[STOP] = colours::stop(self.previous[STOP]);
        for (led, hue) in [
            (OCTAVE_DOWN, 128),
            (OCTAVE_UP, 192),
            (RESTRIKE, 32),
            (ARRIVE, 85),
        ] {
            frame[led] = if led == ARRIVE && !self.accepted[led] && can_resolve {
                hsv_to_rgb(Hsv::new(hue, 210, 80))
            } else {
                colours::control(hue, self.accepted[led])
            };
        }
        frame
    }

    fn preferences(&self) -> Preferences {
        let mut prefs = Preferences::default();
        for &key in &self.control_order {
            let led = usize::from(key);
            if !self.accepted[led] {
                continue;
            }
            match control(led) {
                Some(Control::Colour(choice)) => prefs.colour = choice,
                Some(Control::Foundation(choice)) => prefs.foundation = choice,
                Some(Control::Motion(choice)) => prefs.motion = choice,
                Some(Control::Reharm(choice)) => prefs.reharm = choice,
                Some(Control::Delay) => prefs.delay = true,
                Some(Control::Pedal) => prefs.pedal = true,
                _ => {}
            }
        }
        prefs
    }

    fn octave_offset(&self) -> i8 {
        match (self.accepted[OCTAVE_DOWN], self.accepted[OCTAVE_UP]) {
            (true, false) => -12,
            (false, true) => 12,
            _ => 0,
        }
    }

    fn release_all(&mut self, emit: &mut dyn FnMut(MidiEvent)) {
        for (note, sounding) in self.sounding.iter_mut().enumerate() {
            if *sounding {
                emit(MidiEvent::NoteOff {
                    channel: CHANNEL,
                    note: note as u8,
                });
                *sounding = false;
            }
        }
        self.captured.fill(None);
        self.support.fill(None);
        self.accepted.fill(false);
        self.control_order = CONTROL_ORDER;
        self.event_direction = Direction::Continue;
        self.realizer.clear();
    }
}

fn key_mask(keys: &[bool; LEDS]) -> u64 {
    keys.iter().enumerate().fold(0, |mask, (led, &pressed)| {
        mask | (u64::from(pressed) << led)
    })
}
