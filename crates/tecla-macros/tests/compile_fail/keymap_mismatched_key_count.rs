use tecla_macros::keymap;

fn main() {
    let _ = keymap! {
        layer!(Base, A, B, C),
        layer!(Fn,   F1, F2),
    };
}
