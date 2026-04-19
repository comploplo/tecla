//! ZMK devicetree parser facade.
//!
//! Implementation lives in [`parser`]; this module keeps the public surface
//! shallow and re-exports the parser types and helpers.

mod parser;

pub use parser::*;
