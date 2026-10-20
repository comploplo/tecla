//! Telemetry facade.
//!
//! Implementation lives in `core`; this module keeps the public surface
//! shallow and re-exports the telemetry types.

mod core;

pub mod accessibility;
pub mod encoder;
pub mod gamepad;
pub mod midi;
pub mod pointing;
pub mod steno;
pub mod transport;

pub use core::{BigramStat, CoreSnapshot, CoreTelemetry, TelemetryExt};
