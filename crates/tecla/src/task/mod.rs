//! Async task facade.
//!
//! Implementation lives in the private `runtime` module; this facade keeps the
//! public surface shallow and re-exports the task API.

mod runtime;

pub use runtime::*;
