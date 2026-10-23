use embassy_executor::Spawner;
use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::keyboard::KeyboardProcessor;
use tecla::matrix::MatrixState;
use tecla::pipeline::event::KeyEvent;
use tecla::pipeline::report::HidReport;
use tecla_macros::keyboard_main;

mod pads {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/asterisk_v1_4_midi/keymaps/midi.rs"
    ));
}

mod firmware {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/asterisk_v1_4/midi.rs"
    ));
}

#[keyboard_main]
async fn main(spawner: Spawner) {
    let mut previous = MatrixState::<1, { pads::KEYS }>::new();
    let mut processor = KeyboardProcessor::<1, { pads::KEYS }, 1, { pads::KEYS }>::new(
        pads::keymap(),
        TapHoldConfig::default(),
    );
    firmware::run(
        spawner,
        "Asterisk v1.4 MIDI",
        "AST014-MIDI",
        pads::CHANNEL,
        pads::NOTES,
        |stable| stable.get(0, usize::from(pads::STOP_COL)).unwrap_or(false),
        move |stable, now_ms, _ready, emit| {
            for (pos, pressed) in stable.iter_pos() {
                if pressed == previous.get_pos(pos).unwrap_or(false) {
                    continue;
                }
                let event = if pressed {
                    KeyEvent::press(pos, now_ms)
                } else {
                    KeyEvent::release(pos, now_ms)
                };
                processor.process_event(event, now_ms, &mut |report| {
                    if let HidReport::Midi(event) = report {
                        emit(event);
                    }
                });
            }
            previous = stable.clone();
        },
    )
    .await;
}
