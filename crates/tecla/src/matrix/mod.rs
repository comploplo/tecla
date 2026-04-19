//! Input scanning, debounce, and split keyboard infrastructure.
//!
//! Hardware-agnostic traits for matrix scanning and debouncing.
//! Board crates provide GPIO-level implementations.

pub mod debounce;
pub mod scanner;
pub mod split;

pub use scanner::*;
