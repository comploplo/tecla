use tecla::action::{Action, MidiAction};
use tecla::pipeline::keymap::{Layer, LayerStack};

/// Number of capacitive pads, in the existing Asterisk scanner's GPIO order.
pub const KEYS: usize = 26;
/// Final pad (GPIO13) stops sounding notes; lift held note pads before replaying.
pub const STOP_COL: u8 = 25;
/// Zero-based MIDI channel (channel 1 in a host's interface).
pub const CHANNEL: u8 = 0;
/// Capacitive scanning supplies touch edges, so velocity is fixed.
pub const VELOCITY: u8 = 100;
const _: () = assert!(VELOCITY > 0 && VELOCITY <= 127);
/// Notes for scanner indices 0 through 24; physical musical order is unverified.
pub const NOTES: [u8; 25] = [
    48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71,
    72,
];
/// Fixed pad actions; the board handles the stop pad independently.
pub const fn actions() -> [Action; KEYS] {
    let mut result = [Action::NoOp; KEYS];
    let mut index = 0;
    while index < NOTES.len() {
        result[index] = Action::Midi(MidiAction::NoteOn {
            channel: CHANNEL,
            note: NOTES[index],
            velocity: VELOCITY,
        });
        index += 1;
    }
    result
}

/// Constructs the same keymap used by the firmware and host integration tests.
pub fn keymap() -> LayerStack<1, KEYS> {
    let mut layers = heapless::Vec::new();
    assert!(layers.push(Layer::new(&actions())).is_ok());
    LayerStack::new(layers)
}
