//! Device modality modules.
//!
//! Each subdirectory bundles the complete implementation for one input/output
//! modality: action types, report types, processing logic, and telemetry.
//! Study any device module as a self-contained template for building your own.

pub mod encoder;
pub mod gamepad;
pub mod keyboard;
pub mod midi;
pub mod mouse;
pub mod steno;
