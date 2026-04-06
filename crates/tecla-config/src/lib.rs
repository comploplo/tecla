//! QMK config parser and keymap transpiler for tecla.
//!
//! This crate parses QMK's `info.json` and `keymap.c` formats and produces
//! typed Rust keymap source suitable for use with `tecla_macros::keymap!`.
//!
//! It is a `std` library intended for build scripts and CLI tools — not for
//! firmware.

pub mod info_json;
pub mod keymap;
pub mod proximity;
pub mod trie;
pub mod via_json;

#[cfg(feature = "dictionary")]
pub mod dictionary;
