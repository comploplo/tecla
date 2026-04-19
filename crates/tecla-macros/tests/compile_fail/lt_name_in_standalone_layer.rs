use tecla_macros::layer;

fn main() {
    let _: tecla::pipeline::keymap::Layer<()> = layer!(Base, A, LT!(Fn, SPC));
}
