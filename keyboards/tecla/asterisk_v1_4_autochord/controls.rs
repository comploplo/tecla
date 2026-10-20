use tecla::matrix::MatrixState;
use tecla::music::autochord::{
    AnchorVoice, AutoChord, AutoChordInput, HarmonyCommand, InputError, Steering,
};
use tecla::music::PitchClassSet;
use tecla::pipeline::report::MidiEvent;

/// Pads in the Asterisk scanner's hardware-verified GPIO order.
pub const KEYS: usize = 26;
/// Zero-based channel; channel 1 in a MIDI host.
pub const CHANNEL: u8 = 0;
/// Binary capacitive contacts cannot measure attack velocity.
pub const VELOCITY: u8 = 100;
/// Left/right number directly reinterprets bass/melody; both together stop notes.
pub const NUMBER_COLS: [usize; 2] = [0, 1];
// Javelin config/asterisk.h matches our GPIO order: numbers at x=0/8, stars x=4/7.
/// Left star permits borrowing and more adventurous substitutions.
pub const BORROW_COL: usize = 6;
/// Right star permits richer chord extensions.
pub const EXTEND_COL: usize = 7;
/// A prepares the next harmonic movement.
pub const PREPARE_COL: usize = 22;
/// O intensifies the harmonic pull.
pub const TENSION_COL: usize = 23;
/// E arrives at the pending harmonic destination.
pub const RESOLVE_COL: usize = 24;
/// U substitutes another harmony serving the current direction.
pub const SUBSTITUTE_COL: usize = 25;

const CONTROL_COLS: [usize; 8] = [
    0,
    1,
    PREPARE_COL,
    TENSION_COL,
    RESOLVE_COL,
    SUBSTITUTE_COL,
    BORROW_COL,
    EXTEND_COL,
];
// Only the highest currently held thumb can issue a new thumb command.
const THUMB_PRIORITY: [(usize, Steering, HarmonyCommand); 4] = [
    (RESOLVE_COL, Steering::Arrive, HarmonyCommand::Resolve),
    (TENSION_COL, Steering::Lean, HarmonyCommand::Tension),
    (PREPARE_COL, Steering::Prepare, HarmonyCommand::Prepare),
    (
        SUBSTITUTE_COL,
        Steering::Substitute,
        HarmonyCommand::Substitute,
    ),
];

/// Both number pads stop output and require fresh selector presses.
pub fn is_stop(stable: &MatrixState<1, KEYS>) -> bool {
    NUMBER_COLS
        .into_iter()
        .all(|col| stable.get(0, col).unwrap_or(false))
}

/// Left top G2 A2 B2 C3, then left bottom C2 D2 E2 F2.
pub const BASSES: [(usize, u8); 8] = [
    (2, 43),
    (3, 45),
    (4, 47),
    (5, 48),
    (13, 36),
    (14, 38),
    (15, 40),
    (16, 41),
];

/// Right top A4 B4 C5 D5 E5, then right bottom C4 D4 E4 F4 G4.
pub const MELODIES: [(usize, u8); 10] = [
    (8, 69),
    (9, 71),
    (10, 72),
    (11, 74),
    (12, 76),
    (17, 60),
    (18, 62),
    (19, 64),
    (20, 65),
    (21, 67),
];

/// Physical state and captured pitches for this one Asterisk instrument.
pub struct Controls {
    program: AutoChord,
    previous: MatrixState<1, KEYS>,
    basses: heapless::Vec<(usize, u8), 8>,
    melodies: heapless::Vec<(usize, u8), 10>,
    accepted_controls: u8,
}

impl Controls {
    /// Starts silent until the player presses a selector.
    pub fn new(program: AutoChord) -> Self {
        Self {
            program,
            previous: MatrixState::new(),
            basses: heapless::Vec::new(),
            melodies: heapless::Vec::new(),
            accepted_controls: 0,
        }
    }

    /// Consumes every physical snapshot, including scans suppressed by recovery.
    #[cfg(test)]
    pub fn update(
        &mut self,
        stable: &MatrixState<1, KEYS>,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) -> Result<(), InputError> {
        self.update_frame(stable, None, ready, emit)
    }

    /// Uses performance time as evidence at the next physical musical gesture.
    pub fn update_at(
        &mut self,
        stable: &MatrixState<1, KEYS>,
        now_ms: u64,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) -> Result<(), InputError> {
        self.update_frame(stable, Some(now_ms), ready, emit)
    }

    fn update_frame(
        &mut self,
        stable: &MatrixState<1, KEYS>,
        now_ms: Option<u64>,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) -> Result<(), InputError> {
        let held = |col| stable.get(0, col).unwrap_or(false);
        let rising = |col| held(col) && !self.previous.get(0, col).unwrap_or(false);
        let stopped = is_stop(stable);
        let ready = ready && !stopped;
        for (index, col) in CONTROL_COLS.into_iter().enumerate() {
            if !ready || !held(col) {
                self.accepted_controls &= !(1 << index);
            } else if rising(col) {
                self.accepted_controls |= 1 << index;
            }
        }
        let interpretation = self.interpretation();
        let command = if self.accepted(NUMBER_COLS[0]) && rising(NUMBER_COLS[0]) {
            Some(HarmonyCommand::Reinterpret(AnchorVoice::Bass))
        } else if self.accepted(NUMBER_COLS[1]) && rising(NUMBER_COLS[1]) {
            Some(HarmonyCommand::Reinterpret(AnchorVoice::Melody))
        } else {
            self.thumb()
                .and_then(|(col, _, command)| rising(col).then_some(command))
        };
        let revoice = command.is_some()
            || stable
                .iter_pos()
                .any(|(pos, pressed)| pressed && !self.previous.get_pos(pos).unwrap_or(false));
        if ready {
            self.basses.retain(|(col, _)| held(*col));
            self.melodies.retain(|(col, _)| held(*col));
        } else {
            self.basses.clear();
            self.melodies.clear();
        }

        if ready {
            for (pos, pressed) in stable.iter_pos() {
                if !pressed || self.previous.get_pos(pos).unwrap_or(false) {
                    continue;
                }
                let col = usize::from(pos.col);
                if let Some((_, note)) = BASSES.iter().find(|(index, _)| *index == col) {
                    // One entry per physical pad; capacity equals the bank size.
                    assert!(self.basses.push((col, *note)).is_ok());
                }
                if let Some((_, note)) = MELODIES.iter().find(|(index, _)| *index == col) {
                    assert!(self.melodies.push((col, *note)).is_ok());
                }
            }
        }
        // Outside notes sound literally; the remaining fingers guide the inner voices.
        let bass = self.basses.iter().min_by_key(|(_, note)| note).copied();
        let melody = self.melodies.iter().max_by_key(|(_, note)| note).copied();
        let retrigger = u8::from(bass.is_some_and(|(col, _)| rising(col)))
            | (u8::from(melody.is_some_and(|(col, _)| rising(col))) << 3);
        self.previous = stable.clone();
        if !ready {
            if stopped {
                self.program.reset(&mut |event| emit(event));
            } else {
                // USB recovery owns the sweep; old held pads stay unarmed.
                self.program.reset(&mut |_| {});
            }
            return Ok(());
        }

        let bass = bass.map(|(_, note)| note);
        let melody = melody.map(|(_, note)| note);
        let extra_tones = self
            .basses
            .iter()
            .chain(&self.melodies)
            .map(|(_, note)| *note)
            .filter(|note| Some(*note) != bass && Some(*note) != melody)
            .fold(PitchClassSet::default(), |mut tones, note| {
                tones.insert(note);
                tones
            });
        let gates = match (bass.is_some(), melody.is_some()) {
            (true, true) => 0b1111,
            (true, false) => 1,
            (false, true) => 8,
            (false, false) => 0,
        };
        self.program.update(
            AutoChordInput {
                bass,
                melody,
                extra_tones,
                gates,
                hold: [false; 2],
                revoice,
                retrigger,
                command,
                steering: self.steering(),
                interpretation,
                borrowing: self.accepted(BORROW_COL),
                extensions: self.accepted(EXTEND_COL),
                now_ms,
            },
            &mut |event| emit(event),
        )
    }

    fn accepted(&self, col: usize) -> bool {
        CONTROL_COLS
            .iter()
            .position(|candidate| *candidate == col)
            .is_some_and(|index| self.accepted_controls & (1 << index) != 0)
    }

    fn thumb(&self) -> Option<(usize, Steering, HarmonyCommand)> {
        THUMB_PRIORITY
            .into_iter()
            .find(|(col, _, _)| self.accepted(*col))
    }

    fn steering(&self) -> Steering {
        self.thumb()
            .map_or(Steering::Neutral, |(_, steering, _)| steering)
    }

    fn interpretation(&self) -> Option<AnchorVoice> {
        if self.accepted(NUMBER_COLS[0]) {
            Some(AnchorVoice::Bass)
        } else if self.accepted(NUMBER_COLS[1]) {
            Some(AnchorVoice::Melody)
        } else {
            None
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test assertions")]
mod tests {
    use super::*;
    use std::vec::Vec;
    use tecla::music::Key;

    fn controller() -> Controls {
        Controls::new(AutoChord::new(Key::C_MAJOR, CHANNEL, VELOCITY).unwrap())
    }

    fn frame(c: &mut Controls, pads: &[usize], ready: bool) -> Vec<MidiEvent> {
        let mut stable = MatrixState::new();
        for &pad in pads {
            assert!(stable.set(0, pad, true));
        }
        let mut events = Vec::new();
        c.update(&stable, ready, &mut |event| events.push(event))
            .unwrap();
        events
    }

    #[test]
    fn bars_and_thumbs_keep_independent_meanings_without_a_role_menu() {
        for (number, voice) in [(0, AnchorVoice::Bass), (1, AnchorVoice::Melody)] {
            let mut c = controller();
            frame(&mut c, &[13, 19], true);
            frame(&mut c, &[13, 19, number], true);
            assert_eq!(c.interpretation(), Some(voice));
            assert_eq!(c.steering(), Steering::Neutral);
            frame(&mut c, &[13, 19, number, PREPARE_COL], true);
            assert_eq!(c.interpretation(), Some(voice));
            assert_eq!(c.steering(), Steering::Prepare);
            assert!(frame(&mut c, &[13, 19, PREPARE_COL], true).is_empty());
            assert_eq!(c.interpretation(), None);
            assert_eq!(c.steering(), Steering::Prepare);
            assert!(frame(&mut c, &[13, 19], true).is_empty());
            assert_eq!(c.steering(), Steering::Neutral);
        }
    }

    #[test]
    fn the_highest_held_thumb_has_priority_and_releases_are_quiet() {
        for (high, high_steering, low, low_steering) in [
            (
                PREPARE_COL,
                Steering::Prepare,
                SUBSTITUTE_COL,
                Steering::Substitute,
            ),
            (TENSION_COL, Steering::Lean, PREPARE_COL, Steering::Prepare),
            (RESOLVE_COL, Steering::Arrive, TENSION_COL, Steering::Lean),
        ] {
            let mut c = controller();
            frame(&mut c, &[13, 19], true);
            frame(&mut c, &[13, 19, high], true);
            let sounding = c.program.sounding();
            assert!(frame(&mut c, &[13, 19, high, low], true).is_empty());
            assert_eq!(c.program.sounding(), sounding);
            assert_eq!(c.steering(), high_steering);
            assert!(frame(&mut c, &[13, 19, low], true).is_empty());
            assert_eq!(c.program.sounding(), sounding);
            assert_eq!(c.steering(), low_steering);
            assert!(frame(&mut c, &[13, 19], true).is_empty());
            assert_eq!(c.steering(), Steering::Neutral);
        }
    }

    #[test]
    fn stop_and_offline_recovery_require_fresh_control_presses() {
        for col in CONTROL_COLS {
            for stop in [false, true] {
                let mut c = controller();
                frame(&mut c, &[13, 19, col], true);
                assert!(c.accepted(col));
                if stop {
                    frame(&mut c, &[13, 19, col, 0, 1], true);
                } else {
                    frame(&mut c, &[13, 19, col], false);
                }
                assert_eq!(c.accepted_controls, 0);
                assert!(frame(&mut c, &[13, 19, col], true).is_empty());
                assert!(!c.accepted(col));
                frame(&mut c, &[col], true);
                frame(&mut c, &[13, 19, col], true);
                assert!(!c.accepted(col));
                assert!(frame(&mut c, &[13, 19], true).is_empty());
                frame(&mut c, &[13, 19, col], true);
                assert!(c.accepted(col));
            }
        }
    }
}
