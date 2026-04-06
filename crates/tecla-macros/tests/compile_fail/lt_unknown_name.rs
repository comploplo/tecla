use tecla_macros::keymap;

fn main() {
    let _ = keymap! {
        layer!(Base, A, B, LT!(Typo, SPC)),
        layer!(Fn,   F1, F2, TRNS),
    };
}
