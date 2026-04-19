//! Async task facade.
//!
//! Implementation lives in [`runtime`]; this module keeps the public surface
//! shallow and re-exports the task API.

mod runtime;

pub use runtime::*;
