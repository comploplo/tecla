//! RP2040 Asterisk v1.4 steno example.
//!
//! The board wiring and keymap live under
//! [`keyboards/tecla/asterisk_v1_4`](../profiles/asterisk_v1_4).

#![no_std]
#![no_main]
#![allow(unsafe_code)]
#![expect(clippy::unwrap_used)]

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/profiles/asterisk_v1_4/examples/default.rs"
));
