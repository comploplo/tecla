//! ZMK devicetree parser facade.
//!
//! Implementation lives in the private `parser` module; this facade keeps the
//! public surface shallow and re-exports the parser types and helpers.

mod parser;

pub use parser::*;
