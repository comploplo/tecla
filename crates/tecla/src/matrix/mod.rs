//! Input scanning, debounce, and split keyboard infrastructure.
//!
//! Hardware-agnostic traits for matrix scanning and debouncing.
//! Board crates provide GPIO-level implementations.

pub mod analog;
pub mod capacitive;
pub mod debounce;
pub mod scanner;
pub mod split;
mod state;

pub use analog::*;
pub use scanner::*;
