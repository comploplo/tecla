use tecla::matrix::MatrixState;
use tecla::music::tonnetz::{Transformation, Triad};
use tecla::music::{ChordQuality, PitchClassSet};
use tecla::pipeline::report::MidiEvent;
use tecla::rgb::Rgb;

/// Optional USB diagnostic transport context shared with profile serializers.
#[path = "diagnostics.rs"]
#[allow(
    dead_code,
    reason = "snapshot context is used only by profiles with USB diagnostics"
)]
pub mod diagnostics;

/// Shared pitch and control palette for Voyager music profiles.
#[path = "colours.rs"]
pub mod colours;
use colours::NoteState;

/// Number of physical switches and per-key LEDs on a Voyager.
pub const LEDS: usize = 52;
/// MIDI channel 1, using MIDI's zero-based channel field.
pub const CHANNEL: u8 = 0;
/// One scan can release at most 80 pitches and start at most 47 new notes.
pub const MAX_EVENTS: usize = 128;
/// Top-left Esc key: stop all sound and require fresh presses.
pub const STOP: usize = 0;
/// Left outer/front thumb, one octave down while held.
pub const OCTAVE_DOWN: usize = 24;
/// Left inner/back thumb, one octave up while held.
pub const OCTAVE_UP: usize = 25;
/// Right inner/back thumb, software damper toggle.
pub const DAMPER: usize = 50;
/// Right outer/front thumb, accent new attacks while held.
pub const ACCENT: usize = 51;

/// Electrical positions in physical LED order: each half's four rows, then thumbs.
///
/// The two split bottom rows deliberately follow physical order rather than
/// electrical row order. In particular, right-hand N is LED 44, not LED 49.
#[rustfmt::skip]
pub const POSITIONS: [(usize, usize); LEDS] = [
    (0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (0, 6),
    (1, 1), (1, 2), (1, 3), (1, 4), (1, 5), (1, 6),
    (2, 1), (2, 2), (2, 3), (2, 4), (2, 5), (2, 6),
    (3, 1), (3, 2), (3, 3), (3, 4), (3, 5), (4, 4),
    (5, 0), (5, 1),
    (6, 0), (6, 1), (6, 2), (6, 3), (6, 4), (6, 5),
    (7, 0), (7, 1), (7, 2), (7, 3), (7, 4), (7, 5),
    (8, 0), (8, 1), (8, 2), (8, 3), (8, 4), (8, 5),
    (10, 2), (9, 1), (9, 2), (9, 3), (9, 4), (9, 5),
    (11, 5), (11, 6),
];

/// Fixed audition maps sharing the same articulation and controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "each fixed profile constructs only its own orientation"
)]
pub enum Orientation {
    /// Right hand mirrors the left hand's spatial intervals, one octave above it.
    Mirrored,
    /// Both hands ascend left to right, with completely separated registers.
    Parallel,
    /// Semitone columns and major-third rows, with separated hand registers.
    FourFinger,
    /// Minor-third columns and semitone rows, with separated hand registers.
    Accordion,
}

/// Unshifted MIDI pitch for a physical LED index; controls have no pitch.
pub const fn base_note(orientation: Orientation, led: usize) -> Option<u8> {
    if led == STOP || led == OCTAVE_DOWN || led == OCTAVE_UP || led >= DAMPER {
        return None;
    }
    let (left_base, right_base, across, up) = match orientation {
        Orientation::Mirrored => (35, 47, 3, 4),
        Orientation::Parallel => (35, 63, 3, 4),
        Orientation::FourFinger => (47, 65, 1, 4),
        Orientation::Accordion => (45, 64, 3, 1),
    };
    let right_hand = led > OCTAVE_UP;
    let local = if right_hand { led - 26 } else { led };
    let base = if right_hand { right_base } else { left_base };
    // A half has exactly four physical rows of six keys.
    let row = (local / 6) as u8;
    let col = (local % 6) as u8;
    let horizontal = match (orientation, right_hand) {
        (Orientation::Mirrored, true) => 5 - col,
        _ => col,
    };
    Some(base + across * horizontal + up * (3 - row))
}

const CONTROLS: [usize; 4] = [OCTAVE_DOWN, OCTAVE_UP, DAMPER, ACCENT];

/// Manual polyphony with attack-time pitch capture and visual-only Tonnetz hints.
pub struct Instrument {
    orientation: Orientation,
    previous: [bool; LEDS],
    accepted_controls: [bool; 4],
    captured: [Option<u8>; LEDS],
    // Each LED can show its most recent retained release. The independent
    // per-pitch set keeps older damper notes alive when that pad is played again.
    retained: [Option<u8>; LEDS],
    sustained: [bool; 128],
    sounding: [bool; 128],
    damper: bool,
}

impl Instrument {
    /// Starts silent, with no octave shift, accent or damper.
    pub const fn new(orientation: Orientation) -> Self {
        Self {
            orientation,
            previous: [false; LEDS],
            accepted_controls: [false; 4],
            captured: [None; LEDS],
            retained: [None; LEDS],
            sustained: [false; 128],
            sounding: [false; 128],
            damper: false,
        }
    }

    /// Consumes a debounced physical snapshot and emits only manual note events.
    ///
    /// `ready=false` releases the instrument and consumes suppressed input. A
    /// held key or modifier must then be released and pressed afresh, preventing
    /// USB recovery or a failed right-hand scan from resurrecting old gestures.
    pub fn update(
        &mut self,
        stable: &MatrixState<12, 7>,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) {
        let pressed = POSITIONS.map(|(row, col)| stable.get(row, col).unwrap_or(false));
        if !ready || pressed[STOP] {
            self.release_all(emit);
            self.previous = pressed;
            return;
        }

        for (index, led) in CONTROLS.into_iter().enumerate() {
            if !pressed[led] {
                self.accepted_controls[index] = false;
            } else if !self.previous[led] {
                self.accepted_controls[index] = true;
            }
        }
        if self.accepted_controls[2] && pressed[DAMPER] && !self.previous[DAMPER] {
            self.damper = !self.damper;
            if !self.damper {
                self.sustained.fill(false);
                self.retained.fill(None);
            }
        }

        for (led, captured) in self.captured.iter_mut().enumerate() {
            if !pressed[led] {
                if let Some(note) = captured.take() {
                    if self.damper {
                        self.sustained[usize::from(note)] = true;
                        self.retained[led] = Some(note);
                    }
                }
            }
        }

        let mut attacks = [false; 128];
        let offset = self.octave_offset();
        for (led, &held) in pressed.iter().enumerate() {
            if held && !self.previous[led] {
                if let Some(base) = base_note(self.orientation, led) {
                    // All fixed maps and their +/- octave shifts fit 23..102.
                    let note = (i16::from(base) + i16::from(offset)) as u8;
                    self.captured[led] = Some(note);
                    self.retained[led] = None;
                    attacks[usize::from(note)] = true;
                }
            }
        }

        let mut desired = self.sustained;
        for note in self.captured.iter().flatten() {
            desired[usize::from(*note)] = true;
        }
        // Shared pitches have one MIDI owner. A fresh press deliberately
        // rearticulates once, including simultaneous duplicate physical presses.
        for (note, &was_sounding) in self.sounding.iter().enumerate() {
            if was_sounding && (!desired[note] || attacks[note]) {
                emit(MidiEvent::NoteOff {
                    channel: CHANNEL,
                    note: note as u8,
                });
            }
        }
        let velocity = if self.accepted_controls[3] { 120 } else { 88 };
        for (note, &wanted) in desired.iter().enumerate() {
            if wanted && (!self.sounding[note] || attacks[note]) {
                emit(MidiEvent::NoteOn {
                    channel: CHANNEL,
                    note: note as u8,
                    velocity,
                });
            }
        }
        self.sounding = desired;
        self.previous = pressed;
    }

    /// State-driven colours in the Voyager driver's physical LED order.
    pub fn frame(&self) -> [Rgb; LEDS] {
        let hints = self.hints();
        let offset = self.octave_offset();
        let mut held = [false; 128];
        let mut held_classes = PitchClassSet::default();
        let mut sustained_classes = PitchClassSet::default();
        for note in self.captured.iter().flatten() {
            held[usize::from(*note)] = true;
            held_classes.insert(*note);
        }
        for (note, sustained) in self.sustained.iter().enumerate() {
            if *sustained {
                sustained_classes.insert(note as u8);
            }
        }
        let mut frame = [Rgb::BLACK; LEDS];
        for (led, colour) in frame.iter_mut().enumerate() {
            if let Some(base) = base_note(self.orientation, led) {
                let (pitch, state) = if let Some(note) = self.captured[led] {
                    (note, NoteState::Held)
                } else if let Some(note) =
                    self.retained[led].filter(|note| self.sustained[usize::from(*note)])
                {
                    (note, NoteState::Retained)
                } else {
                    let note = (i16::from(base) + i16::from(offset)) as u8;
                    let state = if held[usize::from(note)] {
                        NoteState::ExactHeld
                    } else if held_classes.contains(note) {
                        NoteState::OctaveHeld
                    } else if sustained_classes.contains(note) {
                        NoteState::Sustained
                    } else if hints.contains(note) {
                        NoteState::Hint
                    } else {
                        NoteState::Idle
                    };
                    (note, state)
                };
                *colour = colours::note(pitch, state);
            }
        }
        frame[STOP] = colours::stop(self.previous[STOP]);
        for (led, hue, active) in [
            (OCTAVE_DOWN, 128, self.accepted_controls[0]),
            (OCTAVE_UP, 192, self.accepted_controls[1]),
            (DAMPER, 85, self.damper),
            (ACCENT, 16, self.accepted_controls[3]),
        ] {
            frame[led] = colours::control(hue, active);
        }
        frame
    }

    fn octave_offset(&self) -> i8 {
        match (self.accepted_controls[0], self.accepted_controls[1]) {
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
        self.retained.fill(None);
        self.sustained.fill(false);
        self.accepted_controls.fill(false);
        self.damper = false;
    }

    fn hints(&self) -> PitchClassSet {
        let mut held = PitchClassSet::default();
        for note in self.captured.iter().flatten() {
            held.insert(*note);
        }
        if held.len() != 2 && held.len() != 3 {
            return PitchClassSet::default();
        }
        let mut hints = 0;
        for root in 0..12 {
            for quality in [ChordQuality::Major, ChordQuality::Minor] {
                // Both qualities and the bounded root are valid Tonnetz triads.
                let Some(triad) = Triad::new(root, quality) else {
                    continue;
                };
                if held.len() == 2 && held.bits() & triad.tones().bits() == held.bits() {
                    hints |= triad.tones().bits();
                } else if held == triad.tones() {
                    for transformation in [
                        Transformation::Parallel,
                        Transformation::LeadingToneExchange,
                        Transformation::Relative,
                    ] {
                        hints |= triad.transform(transformation).tones().bits();
                    }
                }
            }
        }
        PitchClassSet::from_bits(hints & !held.bits())
    }
}
