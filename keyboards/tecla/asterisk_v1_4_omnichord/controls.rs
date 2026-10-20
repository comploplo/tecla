use tecla::matrix::MatrixState;
use tecla::music::harmony::{Chord, Shape};
use tecla::music::PitchClassSet;
use tecla::pipeline::report::MidiEvent;

/// Number of pads in the hardware-verified Asterisk scanner order.
pub const KEYS: usize = 26;
/// Zero-based MIDI channel; channel 1 in a host.
pub const CHANNEL: u8 = 0;
/// A pluck rings this long unless sustained or explicitly damped.
pub const PLUCK_MS: u64 = 500;

const BED_VELOCITY: u8 = 64;
const HARP_VELOCITY: u8 = 100;
const BED_OWNERS: usize = 6;
const OWNERS: usize = BED_OWNERS + 10;
const ROOTS: [(usize, u8); 8] = [
    (2, 43),  // S- top: G2
    (3, 45),  // T-: A2
    (4, 47),  // P-: B2
    (5, 48),  // H-: C3
    (13, 36), // S- bottom: C2
    (14, 38), // K-: D2
    (15, 40), // W-: E2
    (16, 41), // R-: F2
];
// Ascend along the bottom row, then return along the top: R B G S Z D T L P F.
const STRINGS: [usize; 10] = [17, 18, 19, 20, 21, 12, 11, 10, 9, 8];

/// Both number bars stop the entire instrument and require fresh presses.
pub fn is_stop(stable: &MatrixState<1, KEYS>) -> bool {
    stable.get(0, 0).unwrap_or(false) && stable.get(0, 1).unwrap_or(false)
}

#[derive(Clone, Copy)]
struct Pluck {
    note: Option<u8>,
    until_ms: u64,
}

impl Pluck {
    const SILENT: Self = Self {
        note: None,
        until_ms: 0,
    };
}

/// Chord buttons, momentary colours, and ten independently ringing harp strings.
pub struct Controls {
    previous: MatrixState<1, KEYS>,
    accepted: u32,
    roots: heapless::Vec<(usize, u8), 8>,
    selected: Option<u8>,
    major_seventh_recent: bool,
    harmony: Option<(Chord, PitchClassSet)>,
    plucks: [Pluck; 10],
    sounding: [Option<u8>; OWNERS],
}

impl Controls {
    /// Starts without a selected chord or sounding notes.
    pub fn new() -> Self {
        Self {
            previous: MatrixState::new(),
            accepted: 0,
            roots: heapless::Vec::new(),
            selected: None,
            major_seventh_recent: true,
            harmony: None,
            plucks: [Pluck::SILENT; 10],
            sounding: [None; OWNERS],
        }
    }

    /// Processes one coherent touch snapshot; time alone may only end plucks.
    pub fn update_at(
        &mut self,
        stable: &MatrixState<1, KEYS>,
        now_ms: u64,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) {
        if !ready || is_stop(stable) {
            self.deliver([None; OWNERS], 0, emit);
            self.accepted = 0;
            self.roots.clear();
            self.selected = None;
            self.major_seventh_recent = true;
            self.harmony = None;
            self.plucks = [Pluck::SILENT; 10];
            self.previous = stable.clone();
            return;
        }

        let held = |col| stable.get(0, col).unwrap_or(false);
        let rising = |col| held(col) && !self.previous.get(0, col).unwrap_or(false);
        for col in 0..KEYS {
            if !held(col) {
                self.accepted &= !(1 << col);
            } else if rising(col) {
                self.accepted |= 1 << col;
            }
        }
        self.roots
            .retain(|(col, _)| self.accepted & (1 << col) != 0);
        let mut bed_attack = false;
        for (col, note) in ROOTS {
            if rising(col) {
                // Each accepted physical pad owns at most one stack entry.
                assert!(self.roots.push((col, note)).is_ok());
                bed_attack = true;
            }
        }
        if let Some((_, note)) = self.roots.last() {
            self.selected = Some(*note);
        }
        if rising(23) {
            self.major_seventh_recent = false;
        }
        if rising(24) {
            self.major_seventh_recent = true;
        }

        let held_control = |col: usize| self.accepted & (1_u32 << col) != 0;
        let sustain = held_control(6);
        let damp = held_control(7);
        let accidental = i16::from(held_control(1)) - i16::from(held_control(0));
        let bass = self
            .selected
            .map(|note| (i16::from(note) + accidental) as u8);
        let harmony = bass.map(|bass| {
            let chord = Chord {
                root: bass % 12,
                shape: shape(
                    held_control(22),
                    held_control(23),
                    held_control(24) && (!held_control(23) || self.major_seventh_recent),
                    held_control(25),
                ),
            };
            let mut tones = chord.tones();
            if held_control(25) {
                // The catalogue's minor-major seventh still gains a real ninth.
                tones.insert(chord.root + 2);
            }
            (chord, tones)
        });
        if damp
            || (!sustain && harmony.map(|(_, tones)| tones) != self.harmony.map(|(_, tones)| tones))
        {
            self.plucks = [Pluck::SILENT; 10];
        } else if !sustain {
            for pluck in &mut self.plucks {
                if now_ms >= pluck.until_ms {
                    *pluck = Pluck::SILENT;
                }
            }
        }

        let mut retrigger = 0_u128;
        if !damp {
            if let Some((chord, tones)) = harmony {
                for (rank, col) in STRINGS.into_iter().enumerate() {
                    if rising(col) {
                        if let Some(note) = string_note(chord.root, tones, rank) {
                            self.plucks[rank] = Pluck {
                                note: Some(note),
                                until_ms: now_ms.saturating_add(PLUCK_MS),
                            };
                            retrigger |= 1_u128 << note;
                        }
                    }
                }
            }
        }

        let mut desired = [None; OWNERS];
        if !self.roots.is_empty() {
            desired[0] = bass;
            if let Some((_, tones)) = harmony {
                let mut owner = 1;
                for note in 48..60 {
                    if tones.contains(note) {
                        desired[owner] = Some(note);
                        owner += 1;
                    }
                }
            }
            if bed_attack {
                for note in desired[..BED_OWNERS].iter().flatten() {
                    retrigger |= 1_u128 << note;
                }
            }
        }
        for (rank, pluck) in self.plucks.iter().enumerate() {
            desired[BED_OWNERS + rank] = pluck.note;
        }
        self.harmony = harmony;
        self.previous = stable.clone();
        self.deliver(desired, retrigger, emit);
    }

    fn deliver(
        &mut self,
        desired: [Option<u8>; OWNERS],
        retrigger: u128,
        emit: &mut dyn FnMut(MidiEvent),
    ) {
        for (owner, note) in self.sounding.iter().copied().enumerate() {
            if let Some(note) = note {
                if !self.sounding[..owner].contains(&Some(note))
                    && (!desired.contains(&Some(note)) || retrigger & (1_u128 << note) != 0)
                {
                    emit(MidiEvent::NoteOff {
                        channel: CHANNEL,
                        note,
                    });
                }
            }
        }
        for (owner, note) in desired.iter().copied().enumerate() {
            if let Some(note) = note {
                if !desired[..owner].contains(&Some(note))
                    && (!self.sounding.contains(&Some(note)) || retrigger & (1_u128 << note) != 0)
                {
                    emit(MidiEvent::NoteOn {
                        channel: CHANNEL,
                        note,
                        velocity: if owner < BED_OWNERS {
                            BED_VELOCITY
                        } else {
                            HARP_VELOCITY
                        },
                    });
                }
            }
        }
        self.sounding = desired;
    }
}

impl Default for Controls {
    fn default() -> Self {
        Self::new()
    }
}

fn shape(minor: bool, seventh: bool, major_seventh: bool, ninth: bool) -> Shape {
    match (minor, seventh, major_seventh, ninth) {
        (true, _, true, _) => Shape::MinorMajor7,
        (false, _, true, true) => Shape::Major9,
        (false, _, true, false) => Shape::Major7,
        (true, true, false, true) => Shape::Minor9,
        (false, true, false, true) => Shape::Dominant9,
        (true, true, false, false) => Shape::Minor7,
        (false, true, false, false) => Shape::Dominant7,
        (true, false, false, true) => Shape::MinorAdd9,
        (false, false, false, true) => Shape::Add9,
        (true, false, false, false) => Shape::Minor,
        (false, false, false, false) => Shape::Major,
    }
}

fn string_note(root: u8, tones: PitchClassSet, rank: usize) -> Option<u8> {
    (60 + root..=127)
        .filter(|note| tones.contains(*note))
        .nth(rank)
}
