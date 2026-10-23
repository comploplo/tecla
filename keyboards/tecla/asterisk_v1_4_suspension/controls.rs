// A contact-shaped harmonic body with prepared, independently released tensions.

use tecla::matrix::MatrixState;
use tecla::music::harmony::{Chord, Shape};
use tecla::music::realization::{
    Colour, Direction, Foundation, Motion, Preferences, Realization, Realizer, Reharm,
    SUPPORT_VOICES,
};
use tecla::music::{ChordKind, ChordQuality, Key};
use tecla::pipeline::report::MidiEvent;

/// Hardware-verified Asterisk capacitive scanner width.
pub const KEYS: usize = 26;
/// MIDI channel 1, numbered from zero in the wire representation.
pub const CHANNEL: u8 = 0;
const VELOCITY: u8 = 88;
const TRANSFER_MS: u64 = 24;
const LEFT: [usize; 8] = [2, 3, 4, 5, 13, 14, 15, 16]; // S T P H / S K W R
const SHAPE: [usize; 5] = [8, 9, 10, 11, 12]; // F P L T D
const RETAIN: [usize; 5] = [17, 18, 19, 20, 21]; // R B G S Z
const MODIFIERS: [usize; 8] = [0, 1, 6, 7, 22, 23, 24, 25];

/// Both number bars stop the phrase and require fresh playing contacts.
pub fn is_stop(stable: &MatrixState<1, KEYS>) -> bool {
    stable.get(0, 0).unwrap_or(false) && stable.get(0, 1).unwrap_or(false)
}

/// No contact owns a note: gestures transform a shared, chronologically heard body.
pub struct Controls {
    player: Realizer,
    previous: u32,
    accepted: u32,
    retained: [u8; 5],
    sounding: u128,
    playing: bool,
    quiet_since: Option<u64>,
}

impl Controls {
    /// Starts a silent phrase in C major.
    pub const fn new() -> Self {
        Self {
            player: Realizer::new(Key::C_MAJOR),
            previous: 0,
            accepted: 0,
            retained: [0; 5],
            sounding: 0,
            playing: false,
            quiet_since: None,
        }
    }

    /// Processes a coherent touch frame using the shared Asterisk MIDI runtime.
    pub fn update_at(
        &mut self,
        stable: &MatrixState<1, KEYS>,
        now_ms: u64,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) {
        let physical = (0..KEYS).fold(0, |bits, pad| {
            bits | (u32::from(stable.get(0, pad).unwrap_or(false)) << pad)
        });
        if !ready || is_stop(stable) {
            self.deliver([None; SUPPORT_VOICES], emit);
            self.player.clear();
            self.previous = physical;
            self.accepted = 0;
            self.retained = [0; 5];
            self.playing = false;
            self.quiet_since = None;
            return;
        }

        let pressed = physical & !self.previous;
        self.previous = physical;
        self.accepted = (self.accepted | pressed) & physical;
        let down = self.accepted;
        let before_retain = self.retain_mask();
        for (index, pad) in RETAIN.into_iter().enumerate() {
            if !has(down, pad) {
                self.retained[index] = 0;
            } else if has(pressed, pad) {
                self.retained[index] = retention_group(self.player.current(), index);
            }
        }

        let fingers = LEFT.into_iter().chain(SHAPE).chain(RETAIN);
        if !fingers.into_iter().any(|pad| has(down, pad)) {
            if self.playing {
                let since = *self.quiet_since.get_or_insert(now_ms);
                // A brief gap while transferring fingers is not a new phrase.
                if now_ms.saturating_sub(since) >= TRANSFER_MS {
                    self.deliver([None; SUPPORT_VOICES], emit);
                    self.player.plan(&[], Preferences::default());
                    self.playing = false;
                    self.quiet_since = None;
                }
            }
            return;
        }
        self.quiet_since = None;

        let event = !self.playing
            || LEFT
                .into_iter()
                .chain(SHAPE)
                .chain(MODIFIERS)
                .any(|pad| has(pressed, pad));
        self.playing = true;
        let retain = self.retain_mask();
        if event {
            let (reference, preferences) = self.request(down, pressed);
            let body = self.player.plan_harmony(reference, preferences, retain);
            self.deliver(body.notes, emit);
        } else if before_retain & !retain != 0 {
            // Lifting a catch resolves its voice, not the whole harmonic sentence.
            let body = self.player.resolve_suspensions(retain);
            self.deliver(body.notes, emit);
        }

        // A catch present before the first sound arms only after that sound exists.
        for (index, pad) in RETAIN.into_iter().enumerate() {
            if has(down, pad) && self.retained[index] == 0 {
                self.retained[index] = retention_group(self.player.current(), index);
            }
        }
    }

    #[cfg(test)]
    pub fn diagnostics(&self) -> tecla::music::realization::Diagnostics {
        self.player.diagnostics()
    }

    fn retain_mask(&self) -> u8 {
        self.retained
            .into_iter()
            .fold(0, |mask, group| mask | group)
    }

    fn request(&self, down: u32, pressed: u32) -> (Chord, Preferences) {
        let [prepare, lean, root_up, sequence, arrive, deceive, root_down, substitute] = LEFT;
        let [rise, open, reinterpret, close, fall] = SHAPE;
        let [third, fifth, borrow, extend, thumb_prepare, thumb_lean, thumb_arrive, thumb_turn] =
            MODIFIERS;
        let mut reference = self.player.current().chord.unwrap_or(Chord {
            root: 0,
            shape: Shape::Major,
        });
        let mut direction = if has(pressed, arrive) {
            Direction::Arrive
        } else if has(pressed, lean) {
            Direction::Lean
        } else if has(pressed, prepare) {
            Direction::Prepare
        } else if has(pressed, deceive) {
            Direction::Deceive
        } else if has(pressed, substitute) || has(pressed, reinterpret) {
            Direction::Substitute
        } else {
            Direction::Continue
        };
        let step = i8::from(has(pressed, root_up)) - i8::from(has(pressed, root_down));
        if step != 0 {
            reference = shifted(reference, step);
        } else if has(pressed, sequence) {
            // A descending-fifths sequence belongs to the heard harmony, not a key.
            reference = shifted(reference, 3);
        }
        let applied = self.player.diagnostics().applied;
        let thumbs = [
            (thumb_arrive, Direction::Arrive),
            (thumb_lean, Direction::Lean),
            (thumb_prepare, Direction::Prepare),
            (thumb_turn, Direction::Substitute),
        ];
        // A new contact wins an overlapping transfer; releases remain quiet.
        if let Some((_, intent)) = thumbs
            .iter()
            .find(|(pad, _)| has(pressed, *pad))
            .or_else(|| {
                thumbs
                    .iter()
                    .find(|(pad, intent)| has(down, *pad) && *intent == applied.direction)
            })
            .or_else(|| thumbs.iter().find(|(pad, _)| has(down, *pad)))
        {
            direction = *intent;
        }

        let outward = i8::from(has(down, open)) - i8::from(has(down, close));
        let vertical = i8::from(has(down, rise)) - i8::from(has(down, fall));
        // Rocking between paired pads must move immediately, including overlap.
        let spacing_attack = has(pressed, open) || has(pressed, close);
        let vertical_attack = has(pressed, rise) || has(pressed, fall);
        let axes = if vertical_attack && !spacing_attack {
            (
                0,
                i8::from(has(pressed, rise)) - i8::from(has(pressed, fall)),
            )
        } else if spacing_attack {
            (
                i8::from(has(pressed, open)) - i8::from(has(pressed, close)),
                0,
            )
        } else if [
            (open, Motion::Outward),
            (close, Motion::Inward),
            (rise, Motion::Rising),
            (fall, Motion::Falling),
        ]
        .into_iter()
        .any(|(pad, motion)| has(down, pad) && applied.motion == motion)
        {
            match applied.motion {
                Motion::Outward => (1, 0),
                Motion::Inward => (-1, 0),
                Motion::Rising => (0, 1),
                Motion::Falling => (0, -1),
                Motion::Nearest => (0, 0),
            }
        } else {
            (outward, vertical)
        };
        let motion = match axes {
            (1, _) => Motion::Outward,
            (-1, _) => Motion::Inward,
            (_, 1) => Motion::Rising,
            (_, -1) => Motion::Falling,
            _ => Motion::Nearest,
        };
        if motion != Motion::Nearest
            && direction == Direction::Continue
            && self.player.current().chord == Some(reference)
        {
            direction = Direction::Explore;
        }
        let reharm = if has(down, borrow) {
            Reharm::Borrowed
        } else if has(down, reinterpret) {
            Reharm::CommonTone
        } else {
            Reharm::Faithful
        };
        let foundation = if has(down, third) {
            Foundation::Third
        } else if has(down, fifth) {
            Foundation::Fifth
        } else {
            Foundation::Natural
        };
        (
            reference,
            Preferences {
                colour: if has(down, extend) {
                    Colour::Extended
                } else {
                    Colour::Natural
                },
                direction,
                motion,
                reharm,
                foundation,
                ..Preferences::default()
            },
        )
    }

    fn deliver(&mut self, notes: [Option<u8>; SUPPORT_VOICES], emit: &mut dyn FnMut(MidiEvent)) {
        let desired = notes
            .into_iter()
            .flatten()
            .fold(0u128, |set, note| set | (1u128 << note));
        let mut released = self.sounding & !desired;
        while released != 0 {
            let note = released.trailing_zeros() as u8;
            released &= released - 1;
            emit(MidiEvent::NoteOff {
                channel: CHANNEL,
                note,
            });
        }
        let mut attacked = desired & !self.sounding;
        while attacked != 0 {
            let note = attacked.trailing_zeros() as u8;
            attacked &= attacked - 1;
            emit(MidiEvent::NoteOn {
                channel: CHANNEL,
                note,
                velocity: VELOCITY,
            });
        }
        self.sounding = desired;
    }
}

impl Default for Controls {
    fn default() -> Self {
        Self::new()
    }
}

fn has(bits: u32, pad: usize) -> bool {
    bits & (1 << pad) != 0
}

fn shifted(reference: Chord, steps: i8) -> Chord {
    let key = Key::C_MAJOR;
    let chromatic_steps = if steps == 3 { 5 } else { 2 * steps };
    let fallback = (i16::from(reference.root) + i16::from(chromatic_steps)).rem_euclid(12) as u8;
    let note = key
        .diatonic_step(60 + reference.root, steps)
        .unwrap_or(60 + fallback);
    let shape = key.degree(note).map_or(Shape::Major, |degree| {
        match key.chord(degree, ChordKind::Triad).quality() {
            ChordQuality::Minor => Shape::Minor,
            ChordQuality::Diminished => Shape::Diminished,
            _ => Shape::Major,
        }
    });
    Chord {
        root: note % 12,
        shape,
    }
}

fn retention_group(body: Realization, column: usize) -> u8 {
    let mut voices: heapless::Vec<(u8, usize), SUPPORT_VOICES> = body
        .notes
        .into_iter()
        .enumerate()
        .skip(1)
        .filter_map(|(voice, note)| note.map(|pitch| (pitch, voice)))
        .collect();
    voices.sort_unstable();
    if voices.is_empty() {
        return 0;
    }
    if column == 2 {
        return voices
            .iter()
            .fold(0, |mask, (_, voice)| mask | (1 << voice));
    }
    let first = (column * voices.len().saturating_sub(2) + 2) / 4;
    voices
        .iter()
        .skip(first)
        .take(2)
        .fold(0, |mask, (_, voice)| mask | (1 << voice))
}
