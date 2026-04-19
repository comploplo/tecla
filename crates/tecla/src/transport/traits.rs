//! Split keyboard transport abstraction.
//!
//! [`SplitTransport`] defines the boundary between the two halves of a split
//! keyboard. The secondary half sends [`KeyEvent`]s over a physical link
//! (UART, I2C, SPI, RF) and the primary half receives them.
//!
//! Implementors handle framing, serialization, and error recovery — this
//! trait only moves [`KeyEvent`] values.

use crate::pipeline::event::KeyEvent;

/// Async transport for split keyboard halves.
///
/// The primary half calls [`recv`](SplitTransport::recv) in a
/// `remote_event_task` loop. The secondary
/// half calls [`send`](SplitTransport::send) after each debounced scan.
///
/// `async fn` in traits is used intentionally — tecla targets single-threaded
/// Embassy executors where `Send` bounds on futures are not required.
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait SplitTransport {
    /// The error type produced by the transport.
    type Error;

    /// Sends a key event to the other half.
    async fn send(&mut self, event: KeyEvent) -> Result<(), Self::Error>;

    /// Waits for and returns the next key event from the other half.
    async fn recv(&mut self) -> Result<KeyEvent, Self::Error>;
}
