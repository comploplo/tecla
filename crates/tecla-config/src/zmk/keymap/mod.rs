//! ZMK keymap transpiler.
//!
//! Converts parsed ZMK keymap bindings into tecla `keymap!` macro invocations.

/// ZMK key name to tecla key name mapping.
pub mod keycodes;
/// Binding-to-tecla transpilation.
pub mod transpiler;

pub use transpiler::{binding_to_tecla, transpile, TranspileError};
