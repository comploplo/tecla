// NOS75 production matrix order, 6 × 17. Empty electrical positions are NoOp.
// Conventional F1–F12 base, Caps Lock → Left Control, hold Fn for media/RGB/WPM.
use tecla::action::Action::{Consumer, Key, NoOp, Transparent};
use tecla::action::KeyCode::*;
use tecla::action::{Action, ConsumerCode, LayerOp};
use tecla::pipeline::keymap::{Layer, LayerStack};
use tecla::rgb::RgbCommand;

pub const ROWS: usize = 6;
pub const COLS: usize = 17;
pub const KEYS: usize = ROWS * COLS;
pub const LAYERS: usize = 2;

#[rustfmt::skip]
pub const BASE: [Action; KEYS] = [
    Key(Escape), Key(F1), Key(F2), Key(F3), Key(F4), Key(F5), Key(F6), Key(F7), Key(F8), Key(F9), Key(F10), Key(F11), Key(F12), NoOp, NoOp, NoOp, NoOp,
    Key(Grave), Key(N1), Key(N2), Key(N3), Key(N4), Key(N5), Key(N6), Key(N7), Key(N8), Key(N9), Key(N0), Key(Minus), Key(Equal), Key(Backspace), NoOp, Key(Delete), Key(PageDown),
    Key(Tab), Key(Q), Key(W), Key(E), Key(R), Key(T), Key(Y), Key(U), Key(I), Key(O), Key(P), Key(LeftBrace), Key(RightBrace), Key(Backslash), Key(PageUp), NoOp, Key(Home),
    Key(LCtrl), Key(A), Key(S), Key(D), Key(F), Key(G), Key(H), Key(J), Key(K), Key(L), Key(Semicolon), Key(Quote), NoOp, Key(Enter), NoOp, NoOp, NoOp,
    Key(LShift), NoOp, Key(Z), Key(X), Key(C), Key(V), Key(B), Key(N), Key(M), Key(Comma), Key(Dot), Key(Slash), NoOp, Key(RShift), Key(Up), NoOp, NoOp,
    Key(LCtrl), Key(LAlt), Key(LGui), NoOp, NoOp, NoOp, Key(Space), NoOp, NoOp, Key(RAlt), Action::Layer(LayerOp::Momentary(1)), NoOp, Key(RCtrl), Key(Left), Key(Down), Key(Right), NoOp
];

#[rustfmt::skip]
pub const FUNCTION: [Action; KEYS] = [
    Transparent, Consumer(ConsumerCode::BrightnessDown), Consumer(ConsumerCode::BrightnessUp), Transparent, Transparent, Transparent, Transparent, Consumer(ConsumerCode::PrevTrack), Consumer(ConsumerCode::Play), Consumer(ConsumerCode::NextTrack), Consumer(ConsumerCode::Mute), Consumer(ConsumerCode::VolumeDown), Consumer(ConsumerCode::VolumeUp), NoOp, NoOp, NoOp, NoOp,
    Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, NoOp, Key(Insert), Transparent,
    Transparent, Transparent, Action::TypeWpm, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Action::Rgb(RgbCommand::HueDown), Action::Rgb(RgbCommand::HueUp), Action::Rgb(RgbCommand::Toggle), Transparent, NoOp, Key(End),
    Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Action::Rgb(RgbCommand::SatDown), Action::Rgb(RgbCommand::SatUp), NoOp, Transparent, NoOp, NoOp, NoOp,
    Transparent, NoOp, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Transparent, Action::Rgb(RgbCommand::SpeedDown), Action::Rgb(RgbCommand::SpeedUp), Transparent, NoOp, Transparent, Action::Rgb(RgbCommand::BrightUp), NoOp, NoOp,
    Transparent, Transparent, Transparent, NoOp, NoOp, NoOp, Transparent, NoOp, NoOp, Transparent, Transparent, NoOp, Transparent, Action::Rgb(RgbCommand::Prev), Action::Rgb(RgbCommand::BrightDown), Action::Rgb(RgbCommand::Next), NoOp
];

pub const fn layers() -> LayerStack<LAYERS, KEYS> {
    LayerStack::new(heapless::Vec::from_array([
        Layer::from_array(BASE),
        Layer::from_array(FUNCTION),
    ]))
}
