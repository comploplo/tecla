//! Tecla — embedded Rust keyboard firmware core.
#![cfg_attr(not(feature = "std"), no_std)]

pub mod action;
pub mod combo;
pub mod debounce;
pub mod event;
pub mod hid;
pub mod keyboard;
pub mod keymap;
pub mod leader;
pub mod matrix;
pub mod modifier;
pub mod recognizer;
pub mod report;
pub mod steno;
pub mod tap_dance;
pub mod tapping;

#[cfg(feature = "embassy")]
pub mod task;

pub mod prelude {
    //! Convenience re-exports — `use tecla::prelude::*` gives you everything a
    //! board crate needs.

    pub use crate::action::{Action, ConsumerCode, KeyCode, LayerOp, ModifierKey};
    pub use crate::debounce::Debouncer;
    pub use crate::event::{KeyEvent, KeyPos};
    pub use crate::hid::Reporter;
    pub use crate::keyboard::{KeyboardProcessor, RolloverMode};
    pub use crate::keymap::{Layer, LayerStack};
    pub use crate::matrix::MatrixScanner;
    pub use crate::modifier::ModifierState;
    pub use crate::recognizer::{Recognition, Recognizer};
    pub use crate::report::HidReport;
    pub use crate::tapping::{TapHoldConfig, TapHoldDecision, TapHoldRecognizer};

    #[cfg(feature = "embassy")]
    pub use crate::hid::AsyncReporter;
    #[cfg(feature = "embassy")]
    pub use crate::matrix::AsyncMatrixScanner;
    #[cfg(feature = "embassy")]
    pub use crate::task::{EventChannel, ReportChannel};
}
