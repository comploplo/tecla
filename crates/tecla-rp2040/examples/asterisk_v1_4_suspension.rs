//! Asterisk v1.4 suspension organ with gesture-driven harmonic movement.

#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "bind_interrupts! emits unsafe impl blocks")]
#![expect(
    clippy::unwrap_used,
    reason = "fixed firmware configuration and task allocation"
)]

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/profiles/asterisk_v1_4_suspension/examples/default.rs"
));
