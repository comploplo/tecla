use tecla_macros::layer;

fn main() {
    let _: tecla::keymap::Layer<()> = layer!(Base, A, LT!(Fn, SPC));
}
