//! Steno dictionary facade.
//!
//! Implementation lives in the private `parser` module; this facade keeps the
//! public surface shallow and re-exports the dictionary helpers.

mod parser;

pub use parser::*;
