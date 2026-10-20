use embassy_executor::Spawner;
use tecla::music::autochord::AutoChord;
use tecla::music::Key;
use tecla_macros::keyboard_main;

mod controls {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../keyboards/tecla/asterisk_v1_4_autochord/controls.rs"
    ));
}

mod firmware {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../keyboards/tecla/asterisk_v1_4/midi.rs"
    ));
}

#[keyboard_main]
async fn main(spawner: Spawner) {
    let mut controls = controls::Controls::new(
        AutoChord::new(Key::C_MAJOR, controls::CHANNEL, controls::VELOCITY).unwrap(),
    );
    let notes: [u8; 128] = core::array::from_fn(|note| u8::try_from(note).unwrap());
    firmware::run(
        spawner,
        "Asterisk v1.4 AutoChord",
        "AST014-CHORD",
        controls::CHANNEL,
        notes,
        controls::is_stop,
        move |stable, now_ms, ready, emit| {
            if let Err(error) = controls.update_at(stable, now_ms, ready, emit) {
                defmt::warn!(
                    "autochord: rejected input: {:?}",
                    defmt::Debug2Format(&error)
                );
            }
        },
    )
    .await;
}
