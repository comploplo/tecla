use embassy_executor::Spawner;
use tecla_macros::keyboard_main;

mod controls {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../keyboards/tecla/asterisk_v1_4_omnichord/controls.rs"
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
    let mut controls = controls::Controls::new();
    let notes: [u8; 128] = core::array::from_fn(|note| u8::try_from(note).unwrap());
    firmware::run(
        spawner,
        "Asterisk v1.4 Omnichord",
        "AST014-OMNI",
        controls::CHANNEL,
        notes,
        controls::is_stop,
        move |stable, now_ms, ready, emit| controls.update_at(stable, now_ms, ready, emit),
    )
    .await;
}
