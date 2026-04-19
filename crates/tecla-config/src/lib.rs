//! QMK config parser and keymap transpiler for tecla.
//!
//! This crate parses QMK's `info.json` and `keymap.c` formats and produces
//! typed Rust keymap source suitable for use with `tecla_macros::keymap!`.
//!
//! It is a `std` library intended for build scripts and CLI tools — not for
//! firmware.

pub mod board;
pub mod proximity;
pub mod qmk;
pub mod steno_dictionary;
pub mod trie;
pub mod zmk;

#[cfg(feature = "dictionary")]
pub mod dictionary;
