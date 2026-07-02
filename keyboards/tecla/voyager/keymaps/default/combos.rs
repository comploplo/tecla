{
    use tecla::action::{Action, KeyCode};
    use tecla::devices::keyboard::combo::{Combo, ComboConfig};

    // Combos are defined by keycode (the key names from the keymap), not matrix
    // positions. Chords fire the instant the set is fully held — the action is
    // held and repeats like any other key, releasing on the first key-up. No
    // timing window, so a fast same-hand roll can misfire; the chords here are
    // cross-hand. `THE` is an example macro (modifier-free).
    const THE: &[KeyCode] = &[KeyCode::T, KeyCode::H, KeyCode::E];

    ComboConfig::new(&[
        Combo::key(&[KeyCode::D, KeyCode::F], KeyCode::Backspace),
        Combo::key(&[KeyCode::F, KeyCode::J], KeyCode::Enter),
        Combo::key(&[KeyCode::D, KeyCode::K], KeyCode::Escape),
        Combo::new(&[KeyCode::S, KeyCode::L], Action::SendString(THE)),
    ])
}
