//! ZMK keyboard configuration parser.
//!
//! Parses ZMK's Devicetree overlay (`.dtsi`/`.overlay`) and keymap (`.keymap`)
//! formats. [`dts`] contains the nom-based devicetree parser. [`parse_shield`]
//! reads a shield directory and produces a [`BoardConfig`](crate::board::BoardConfig).

pub mod dts;
pub mod keymap;
pub mod shield;
