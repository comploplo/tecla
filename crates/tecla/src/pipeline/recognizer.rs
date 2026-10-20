//! The [`Recognizer`] trait — the unifying primitive for all input patterns.
//!
//! A time-sensitive input behaviour that must disambiguate an incomplete
//! pattern against a deadline implements [`Recognizer`]. Today that is
//! [`TapHoldRecognizer`](crate::devices::keyboard::tapping::TapHoldRecognizer);
//! the trait exists so board crates and downstream consumers can add their own
//! and drive them from the same event stream.
//!
//! # Design
//!
//! Recognizers are **pure logic** — no async, no Embassy, no hardware.
//! Timing is injected as a `now_ms: u64` parameter on every call, making
//! recognizers fully deterministic and trivially testable on the host.
//!
//! The orchestrator in `keyboard.rs` is responsible for:
//! - Calling [`Recognizer::observe`] on every [`KeyEvent`].
//! - Calling [`Recognizer::tick`] when [`Recognizer::next_deadline_ms`]
//!   elapses (via an Embassy timer).
//! - Draining any buffered events after a [`Recognition::Matched`] or
//!   [`Recognition::Failed`] result.
//!
//! # Implementing a custom recognizer
//!
//! A recognizer should:
//! 1. Return [`Recognition::Pending`] while still accumulating input.
//! 2. Return [`Recognition::Matched`] (with the output value) as soon as the
//!    pattern is unambiguously complete.
//! 3. Return [`Recognition::Failed`] and call [`reset`](Recognizer::reset)
//!    internally when the pattern can no longer match.
//! 4. Expose a deadline via [`next_deadline_ms`](Recognizer::next_deadline_ms)
//!    if the pattern can time out; return `None` when no timer is needed.
//! 5. Buffer any events that arrive while `Pending` so they can be replayed by
//!    the orchestrator after a `Failed` or `Matched` result.
//!
//! ```rust
//! use tecla::pipeline::event::{KeyEvent, KeyPos};
//! use tecla::pipeline::recognizer::{Recognition, Recognizer};
//!
//! struct DoubleTap {
//!     first_press_ms: Option<u64>,
//!     window_ms: u64,
//! }
//!
//! impl Recognizer for DoubleTap {
//!     type Output = ();
//!
//!     fn observe(&mut self, event: KeyEvent, now_ms: u64) -> Recognition<()> {
//!         if !event.pressed {
//!             return Recognition::Pending;
//!         }
//!         match self.first_press_ms {
//!             None => {
//!                 self.first_press_ms = Some(now_ms);
//!                 Recognition::Pending
//!             }
//!             Some(t) if now_ms - t <= self.window_ms => {
//!                 self.reset();
//!                 Recognition::Matched(())
//!             }
//!             _ => {
//!                 self.reset();
//!                 Recognition::Failed
//!             }
//!         }
//!     }
//!
//!     fn tick(&mut self, _now_ms: u64) -> Recognition<()> {
//!         self.reset();
//!         Recognition::Failed
//!     }
//!
//!     fn next_deadline_ms(&self) -> Option<u64> {
//!         self.first_press_ms.map(|t| t + self.window_ms)
//!     }
//!
//!     fn reset(&mut self) {
//!         self.first_press_ms = None;
//!     }
//! }
//!
//! let key = KeyPos::new(0, 0);
//! let mut rec = DoubleTap { first_press_ms: None, window_ms: 200 };
//! assert!(rec.observe(KeyEvent::press(key, 0), 0).is_pending());
//! assert!(matches!(rec.observe(KeyEvent::press(key, 50), 50), Recognition::Matched(())));
//! ```

use super::event::KeyEvent;

/// The result of feeding an event or tick to a [`Recognizer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recognition<T> {
    /// The pattern matched. Contains the resolved output.
    Matched(T),
    /// Still accumulating — more events or a timeout needed.
    Pending,
    /// No match possible. The recognizer has reset; buffered events should
    /// be replayed into the pipeline.
    Failed,
}

impl<T> Recognition<T> {
    /// Returns `true` if this is [`Recognition::Matched`].
    #[must_use]
    pub const fn is_matched(&self) -> bool {
        matches!(self, Self::Matched(_))
    }

    /// Returns `true` if this is [`Recognition::Pending`].
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Returns `true` if this is [`Recognition::Failed`].
    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed)
    }

    /// Returns the inner value if matched, otherwise `None`.
    #[must_use]
    pub fn matched(self) -> Option<T> {
        match self {
            Self::Matched(v) => Some(v),
            _ => None,
        }
    }
}

/// An input pattern recognizer.
///
/// Implementations observe a stream of [`KeyEvent`]s and resolve to an
/// output value when the pattern is detected.
pub trait Recognizer {
    /// The value produced when the pattern matches.
    type Output;

    /// Feeds a key event into the recognizer.
    ///
    /// Returns [`Recognition::Matched`] if the event completes the pattern,
    /// [`Recognition::Pending`] if more input is needed, or
    /// [`Recognition::Failed`] if the pattern cannot match.
    fn observe(&mut self, event: KeyEvent, now_ms: u64) -> Recognition<Self::Output>;

    /// Called when [`next_deadline_ms`](Recognizer::next_deadline_ms) elapses.
    ///
    /// Allows timeout-based resolution (e.g. tap-hold after tapping term).
    fn tick(&mut self, now_ms: u64) -> Recognition<Self::Output>;

    /// Returns the timestamp at which [`tick`](Recognizer::tick) should next
    /// be called, or `None` if no timer is required.
    fn next_deadline_ms(&self) -> Option<u64>;

    /// Resets the recognizer to its initial idle state.
    fn reset(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Recognition ──────────────────────────────────────────────────────────

    #[test]
    fn matched_is_matched() {
        assert!(Recognition::<u8>::Matched(1).is_matched());
        assert!(!Recognition::<u8>::Matched(1).is_pending());
        assert!(!Recognition::<u8>::Matched(1).is_failed());
    }

    #[test]
    fn pending_is_pending() {
        assert!(Recognition::<u8>::Pending.is_pending());
        assert!(!Recognition::<u8>::Pending.is_matched());
        assert!(!Recognition::<u8>::Pending.is_failed());
    }

    #[test]
    fn failed_is_failed() {
        assert!(Recognition::<u8>::Failed.is_failed());
        assert!(!Recognition::<u8>::Failed.is_matched());
        assert!(!Recognition::<u8>::Failed.is_pending());
    }

    #[test]
    fn matched_extracts_inner_value() {
        assert_eq!(Recognition::<u8>::Matched(42).matched(), Some(42));
    }

    #[test]
    fn pending_matched_returns_none() {
        assert_eq!(Recognition::<u8>::Pending.matched(), None);
    }

    #[test]
    fn failed_matched_returns_none() {
        assert_eq!(Recognition::<u8>::Failed.matched(), None);
    }

    // ── Recognizer (tested via a minimal mock implementation) ────────────────

    struct AlwaysMatch;

    impl Recognizer for AlwaysMatch {
        type Output = ();
        fn observe(&mut self, _event: KeyEvent, _now_ms: u64) -> Recognition<()> {
            Recognition::Matched(())
        }
        fn tick(&mut self, _now_ms: u64) -> Recognition<()> {
            Recognition::Pending
        }
        fn next_deadline_ms(&self) -> Option<u64> {
            None
        }
        fn reset(&mut self) {}
    }

    struct NeverMatch {
        deadline: Option<u64>,
    }

    impl Recognizer for NeverMatch {
        type Output = ();
        fn observe(&mut self, _event: KeyEvent, _now_ms: u64) -> Recognition<()> {
            Recognition::Failed
        }
        fn tick(&mut self, _now_ms: u64) -> Recognition<()> {
            Recognition::Failed
        }
        fn next_deadline_ms(&self) -> Option<u64> {
            self.deadline
        }
        fn reset(&mut self) {
            self.deadline = None;
        }
    }

    #[test]
    fn always_match_returns_matched_on_observe() {
        let pos = crate::pipeline::event::KeyPos::new(0, 0);
        let event = KeyEvent::press(pos, 0);
        let result = AlwaysMatch.observe(event, 0);
        assert!(result.is_matched());
    }

    #[test]
    fn always_match_returns_pending_on_tick() {
        assert!(AlwaysMatch.tick(0).is_pending());
    }

    #[test]
    fn always_match_has_no_deadline() {
        assert_eq!(AlwaysMatch.next_deadline_ms(), None);
    }

    #[test]
    fn never_match_returns_failed_on_observe() {
        let pos = crate::pipeline::event::KeyPos::new(0, 0);
        let event = KeyEvent::press(pos, 0);
        let result = NeverMatch {
            deadline: Some(100),
        }
        .observe(event, 0);
        assert!(result.is_failed());
    }

    #[test]
    fn never_match_exposes_deadline() {
        let r = NeverMatch {
            deadline: Some(500),
        };
        assert_eq!(r.next_deadline_ms(), Some(500));
    }

    #[test]
    fn reset_clears_deadline() {
        let mut r = NeverMatch {
            deadline: Some(500),
        };
        r.reset();
        assert_eq!(r.next_deadline_ms(), None);
    }

    #[test]
    fn never_match_tick_returns_failed() {
        let result = NeverMatch { deadline: None }.tick(0);
        assert!(result.is_failed());
    }
}
