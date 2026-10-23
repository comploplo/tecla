//! RP2040 Uni V4 steno example.
//!
//! The board wiring and keymap live under
//! [`keyboards/tecla/steno_uni_v4`](../profiles/steno_uni_v4).

#![no_std]
#![no_main]
#![allow(unsafe_code)]
#![expect(clippy::unwrap_used)]

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/profiles/steno_uni_v4/examples/default.rs"
));
