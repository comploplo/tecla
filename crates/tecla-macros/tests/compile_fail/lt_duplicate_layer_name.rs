use tecla_macros::keymap;

fn main() {
    let _ = keymap! {
        layer!(Base, A, B),
        layer!(Base, C, D),
    };
}
