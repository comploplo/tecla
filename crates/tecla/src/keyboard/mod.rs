//! Keyboard pipeline facade.
//!
//! This module intentionally stays shallow: implementation details live in
//! sibling modules and are re-exported here.

mod processor;
pub mod steno_playback;

pub use processor::{KeyboardProcessor, ProcessorTelemetry, RolloverMode};
