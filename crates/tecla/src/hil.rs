//! Hardware-in-the-loop test fixtures.
//!
//! This module re-exports the shared [`hilt`] crate, which provides the Renode
//! container runner, ELF introspection, and the matrix-test framework. It is
//! gated behind the `hil` feature.
//!
//! # Usage
//!
//! ```rust,ignore
//! use tecla::hil::{HilConfig, Platform, RenodeRunner};
//!
//! let runner = RenodeRunner::new(HilConfig::new(
//!     Platform::rp2040(),
//!     "target/thumbv6m-none-eabi/release/examples/hil_keyboard",
//! ));
//! let output = runner.run();
//! assert!(output.passed(), "HIL test failed: {}", output.stderr());
//! ```

pub use hilt::*;
