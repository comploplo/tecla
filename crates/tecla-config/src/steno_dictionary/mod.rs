//! Steno dictionary facade.
//!
//! Implementation lives in [`parser`]; this module keeps the public surface
//! shallow and re-exports the dictionary helpers.

mod parser;

pub use parser::*;
