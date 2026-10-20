//! Asterisk v1.4 chromatic harmonica with finger-controlled breath and slides.

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! emits unsafe impl blocks")]
#![expect(
    clippy::unwrap_used,
    reason = "fixed firmware configuration and task allocation"
)]

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../keyboards/tecla/asterisk_v1_4_chromatica/examples/default.rs"
));
