use tecla_macros::layer;

fn main() {
    let _: tecla::pipeline::keymap::Layer<(), 2> = layer!(Base, A, LT!(Fn, SPC));
}
