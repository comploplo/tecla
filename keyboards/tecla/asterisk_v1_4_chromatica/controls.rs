use tecla::matrix::MatrixState;
use tecla::pipeline::report::MidiEvent;

/// Number of pads in the hardware-verified Asterisk scanner order.
pub const KEYS: usize = 26;
/// Zero-based MIDI channel; channel 1 in a host.
pub const CHANNEL: u8 = 0;
/// Fixed attack velocity for binary capacitive contacts.
pub const VELOCITY: u8 = 100;

// Each pad chooses a solo-tuned hole: scanner index, exhale, inhale.
// Upper pads repeat their lower partners one octave higher.
const HOLES: [(usize, u8, u8); 18] = [
    (2, 60, 62),  // Left upper S: C4 / D4
    (3, 64, 65),  // T: E4 / F4
    (4, 67, 69),  // P: G4 / A4
    (5, 72, 71),  // H: C5 / B4
    (13, 48, 50), // Left lower S: C3 / D3
    (14, 52, 53), // K: E3 / F3
    (15, 55, 57), // W: G3 / A3
    (16, 60, 59), // R: C4 / B3
    (8, 72, 74),  // Right upper F: C5 / D5
    (9, 76, 77),  // P: E5 / F5
    (10, 79, 81), // L: G5 / A5
    (11, 84, 83), // T: C6 / B5
    (12, 84, 86), // D: C6 / D6
    (17, 60, 62), // Right lower R: C4 / D4
    (18, 64, 65), // B: E4 / F4
    (19, 67, 69), // G: G4 / A4
    (20, 72, 71), // S: C5 / B4
    (21, 72, 74), // Z: C5 / D5
];

/// Both number bars stop output and require fresh touches.
pub fn is_stop(stable: &MatrixState<1, KEYS>) -> bool {
    stable.get(0, 0).unwrap_or(false) && stable.get(0, 1).unwrap_or(false)
}

/// Polyphonic harmonica holes with momentary breath, slides, and octave bars.
pub struct Controls {
    previous: u32,
    accepted: u32,
    sounding: u128,
}

impl Controls {
    /// Starts silent with exhale selected and all modifiers released.
    pub const fn new() -> Self {
        Self {
            previous: 0,
            accepted: 0,
            sounding: 0,
        }
    }

    /// Processes a coherent touch snapshot, including scans suppressed by USB.
    pub fn update(
        &mut self,
        stable: &MatrixState<1, KEYS>,
        ready: bool,
        emit: &mut dyn FnMut(MidiEvent),
    ) {
        let mut pressed = 0_u32;
        for col in 0..KEYS {
            if stable.get(0, col).unwrap_or(false) {
                pressed |= 1 << col;
            }
        }
        if !ready || is_stop(stable) {
            self.deliver(0, 0, emit);
            self.accepted = 0;
            self.previous = pressed;
            return;
        }

        let rising = pressed & !self.previous;
        // Contacts held through Stop or recovery stay silent until re-pressed.
        self.accepted = (self.accepted | rising) & pressed;
        let held = |col: usize| self.accepted & (1_u32 << col) != 0;
        let inhale = held(6) || held(7);
        let slide = i16::from(held(23) || held(24)) - i16::from(held(22) || held(25));
        let octave = 12 * (i16::from(held(1)) - i16::from(held(0)));

        let mut desired = 0_u128;
        let mut retrigger = 0_u128;
        for (col, exhale, draw) in HOLES {
            if held(col) {
                // The fixed map and modifiers keep every pitch in 35..=99.
                let note = (i16::from(if inhale { draw } else { exhale }) + slide + octave) as u8;
                desired |= 1_u128 << note;
                if rising & (1 << col) != 0 {
                    retrigger |= 1_u128 << note;
                }
            }
        }
        self.previous = pressed;
        self.deliver(desired, retrigger, emit);
    }

    fn deliver(&mut self, desired: u128, retrigger: u128, emit: &mut dyn FnMut(MidiEvent)) {
        // Recompute pitch ownership from all held holes, including duplicates.
        let mut releases = self.sounding & (!desired | retrigger);
        while releases != 0 {
            let note = releases.trailing_zeros() as u8;
            releases &= releases - 1;
            emit(MidiEvent::NoteOff {
                channel: CHANNEL,
                note,
            });
        }
        let mut attacks = desired & (!self.sounding | retrigger);
        while attacks != 0 {
            let note = attacks.trailing_zeros() as u8;
            attacks &= attacks - 1;
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
