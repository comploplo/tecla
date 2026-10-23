//! RP2040 Asterisk v1.4 pad-mapping diagnostic.
//!
//! The board wiring and keymap live under
//! [`keyboards/tecla/asterisk_v1_4_keytest`](../profiles/asterisk_v1_4_keytest).

#![no_std]
#![no_main]
#![allow(unsafe_code)]
#![expect(clippy::unwrap_used)]

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/profiles/asterisk_v1_4_keytest/examples/default.rs"
));
