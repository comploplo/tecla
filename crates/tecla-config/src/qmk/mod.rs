//! QMK keyboard configuration parser.
//!
//! Parses QMK's `keyboard.json` / `info.json` and `keymap.c` formats into
//! typed Rust structures. Use [`info_json::parse`] as the entry point, then
//! convert to a [`BoardConfig`](crate::board::BoardConfig) via
//! [`QmkConfig::to_board_config`](info_json::QmkConfig::to_board_config).

pub mod info_json;
pub mod keymap;
pub mod via_json;
