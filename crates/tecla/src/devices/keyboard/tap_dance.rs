//! Tap-dance keys — different actions for different tap counts.
//!
//! A [`TapDanceRecognizer`] counts rapid taps of a single key within a
//! configurable window. After the window expires since the last release,
//! it resolves with the tap count and the configured action for that count.

use crate::action::KeyCode;
use crate::pipeline::event::{KeyEvent, KeyPos};
use crate::pipeline::recognizer::{Recognition, Recognizer};

/// The resolved output of a tap-dance sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapDanceOutcome {
    /// How many times the key was tapped.
    pub count: usize,
    /// The action mapped to that tap count, or `None` if count exceeded the
    /// configured maximum.
    pub keycode: Option<KeyCode>,
}

/// Recognizes repeated taps of a single key and maps tap count to an action.
///
/// # Lifecycle
///
/// 1. Construct with [`TapDanceRecognizer::new`].
/// 2. Feed events via [`observe`](Recognizer::observe). Any key other than
///    `pos` causes immediate [`Recognition::Failed`].
/// 3. After the key is released, the inter-tap timer starts. A subsequent
///    press within `inter_tap_ms` increments the count. When the timer
///    expires, resolves with the tap count and associated keycode.
pub struct TapDanceRecognizer<const MAX_TAPS: usize> {
    pos: KeyPos,
    actions: [Option<KeyCode>; MAX_TAPS],
    inter_tap_ms: u64,
    count: usize,
    held: bool,
    last_release_ms: Option<u64>,
    done: bool,
}

impl<const MAX_TAPS: usize> TapDanceRecognizer<MAX_TAPS> {
    /// Creates a new tap-dance recognizer.
    ///
    /// `actions[0]` maps to a single tap, `actions[1]` to a double-tap, etc.
    #[must_use]
    pub fn new(pos: KeyPos, actions: [Option<KeyCode>; MAX_TAPS], inter_tap_ms: u64) -> Self {
        Self {
            pos,
            actions,
            inter_tap_ms,
            count: 0,
            held: false,
            last_release_ms: None,
            done: false,
        }
    }

    fn resolve(&mut self) -> Recognition<TapDanceOutcome> {
        self.done = true;
        let keycode = self
            .count
            .checked_sub(1)
            .and_then(|i| self.actions.get(i).copied().flatten());
        Recognition::Matched(TapDanceOutcome {
            count: self.count,
            keycode,
        })
    }
}

impl<const MAX_TAPS: usize> Recognizer for TapDanceRecognizer<MAX_TAPS> {
    type Output = TapDanceOutcome;

    fn observe(&mut self, event: KeyEvent, _now_ms: u64) -> Recognition<TapDanceOutcome> {
        if self.done {
            return Recognition::Pending;
        }

        if event.pos != self.pos {
            self.done = true;
            return Recognition::Failed;
        }

        if event.pressed {
            self.held = true;
            self.last_release_ms = None;
            self.count += 1;
            if self.count >= MAX_TAPS {
                return self.resolve();
            }
        } else {
            self.held = false;
            self.last_release_ms = Some(_now_ms);
        }

        Recognition::Pending
    }

    fn tick(&mut self, now_ms: u64) -> Recognition<TapDanceOutcome> {
        if self.done || self.held {
            return Recognition::Pending;
        }
        if let Some(release_ms) = self.last_release_ms {
            if now_ms.wrapping_sub(release_ms) >= self.inter_tap_ms {
                return self.resolve();
            }
        }
        Recognition::Pending
    }

    fn next_deadline_ms(&self) -> Option<u64> {
        if self.done || self.held {
            return None;
        }
        self.last_release_ms
            .map(|r| r.wrapping_add(self.inter_tap_ms))
    }

    fn reset(&mut self) {
        self.count = 0;
        self.held = false;
        self.last_release_ms = None;
        self.done = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(pos: KeyPos, ms: u64) -> KeyEvent {
        KeyEvent::press(pos, ms)
    }

    fn release(pos: KeyPos, ms: u64) -> KeyEvent {
        KeyEvent::release(pos, ms)
    }

    const KEY: KeyPos = KeyPos::new(0, 0);
    const OTHER: KeyPos = KeyPos::new(0, 1);
    const TERM: u64 = 200;

    fn make_rec() -> TapDanceRecognizer<3> {
        TapDanceRecognizer::new(
            KEY,
            [Some(KeyCode::A), Some(KeyCode::B), Some(KeyCode::C)],
            TERM,
        )
    }

    #[test]
    fn single_tap_resolves_after_timeout() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        let result = rec.tick(250);
        assert_eq!(
            result,
            Recognition::Matched(TapDanceOutcome {
                count: 1,
                keycode: Some(KeyCode::A),
            })
        );
    }

    #[test]
    fn double_tap_resolves_after_timeout() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        rec.observe(press(KEY, 100), 100);
        rec.observe(release(KEY, 150), 150);
        let result = rec.tick(350);
        assert_eq!(
            result,
            Recognition::Matched(TapDanceOutcome {
                count: 2,
                keycode: Some(KeyCode::B),
            })
        );
    }

    #[test]
    fn max_taps_resolves_on_third_press() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 10), 10);
        rec.observe(press(KEY, 20), 20);
        rec.observe(release(KEY, 30), 30);
        let result = rec.observe(press(KEY, 40), 40);
        assert_eq!(
            result,
            Recognition::Matched(TapDanceOutcome {
                count: 3,
                keycode: Some(KeyCode::C),
            })
        );
    }

    #[test]
    fn pending_after_press() {
        let mut rec = make_rec();
        let result = rec.observe(press(KEY, 0), 0);
        assert!(result.is_pending());
    }

    #[test]
    fn pending_after_release_before_timeout() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        assert!(rec.tick(TERM - 1).is_pending());
    }

    #[test]
    fn fails_on_different_key_press() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        let result = rec.observe(press(OTHER, 100), 100);
        assert!(result.is_failed());
    }

    #[test]
    fn next_deadline_none_before_first_release() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        assert!(rec.next_deadline_ms().is_none());
    }

    #[test]
    fn next_deadline_set_after_release() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        assert_eq!(rec.next_deadline_ms(), Some(50 + TERM));
    }

    #[test]
    fn next_deadline_none_while_held() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        rec.observe(press(KEY, 100), 100);
        assert!(rec.next_deadline_ms().is_none());
    }

    #[test]
    fn next_deadline_none_after_resolution() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        rec.tick(50 + TERM);
        assert!(rec.next_deadline_ms().is_none());
    }

    #[test]
    fn observe_after_done_is_pending() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        rec.tick(50 + TERM);
        let result = rec.observe(press(KEY, 300), 300);
        assert!(result.is_pending());
    }

    #[test]
    fn tick_while_held_is_pending() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        assert!(rec.tick(500).is_pending());
    }

    #[test]
    fn tick_without_any_press_is_pending() {
        let mut rec = make_rec();
        assert!(rec.tick(500).is_pending());
    }

    #[test]
    fn reset_allows_reuse() {
        let mut rec = make_rec();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        rec.tick(50 + TERM);
        rec.reset();
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 50), 50);
        let result = rec.tick(50 + TERM);
        assert_eq!(
            result,
            Recognition::Matched(TapDanceOutcome {
                count: 1,
                keycode: Some(KeyCode::A),
            })
        );
    }

    #[test]
    fn count_exceeding_actions_returns_none_keycode() {
        let mut rec: TapDanceRecognizer<2> =
            TapDanceRecognizer::new(KEY, [Some(KeyCode::A), None], TERM);
        rec.observe(press(KEY, 0), 0);
        rec.observe(release(KEY, 10), 10);
        let result = rec.tick(10 + TERM);
        assert_eq!(
            result,
            Recognition::Matched(TapDanceOutcome {
                count: 1,
                keycode: Some(KeyCode::A),
            })
        );
    }
}
