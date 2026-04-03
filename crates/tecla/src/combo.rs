//! Combo keys — simultaneous key presses mapped to a single action.
//!
//! A [`ComboRecognizer`] accumulates presses from a fixed set of [`KeyPos`]
//! keys. When all `N` keys are pressed within `combo_term_ms`, it resolves
//! with the configured [`KeyCode`]. Any non-combo key press or timeout before
//! completion causes a [`Recognition::Failed`].

use crate::action::KeyCode;
use crate::event::{KeyEvent, KeyPos};
use crate::recognizer::{Recognition, Recognizer};

/// Recognizes a simultaneous `N`-key combo.
///
/// # Lifecycle
///
/// 1. Construct with [`ComboRecognizer::new`].
/// 2. Feed events via [`observe`](Recognizer::observe).
/// 3. When all `N` keys are pressed within `combo_term_ms`, yields
///    [`Recognition::Matched`] with the configured keycode.
/// 4. A non-combo key press or a [`tick`](Recognizer::tick) past the deadline
///    yields [`Recognition::Failed`] — the caller should replay buffered events.
pub struct ComboRecognizer<const N: usize> {
    keys: [KeyPos; N],
    action: KeyCode,
    combo_term_ms: u64,
    pressed: [bool; N],
    start_ms: Option<u64>,
    done: bool,
}

impl<const N: usize> ComboRecognizer<N> {
    /// Creates a new combo recognizer.
    ///
    /// `keys` is the set of positions that must all be pressed within
    /// `combo_term_ms` milliseconds.
    #[must_use]
    pub fn new(keys: [KeyPos; N], action: KeyCode, combo_term_ms: u64) -> Self {
        Self {
            keys,
            action,
            combo_term_ms,
            pressed: [false; N],
            start_ms: None,
            done: false,
        }
    }

    fn key_index(&self, pos: KeyPos) -> Option<usize> {
        self.keys.iter().position(|&k| k == pos)
    }

    fn all_pressed(&self) -> bool {
        self.pressed.iter().all(|&p| p)
    }

    fn fail(&mut self) -> Recognition<KeyCode> {
        self.done = true;
        Recognition::Failed
    }
}

impl<const N: usize> Recognizer for ComboRecognizer<N> {
    type Output = KeyCode;

    fn observe(&mut self, event: KeyEvent, now_ms: u64) -> Recognition<KeyCode> {
        if self.done {
            return Recognition::Pending;
        }

        if event.pressed {
            match self.key_index(event.pos) {
                Some(idx) => {
                    if self.start_ms.is_none() {
                        self.start_ms = Some(now_ms);
                    }
                    if let Some(slot) = self.pressed.get_mut(idx) {
                        *slot = true;
                    }
                    if self.all_pressed() {
                        self.done = true;
                        return Recognition::Matched(self.action);
                    }
                    Recognition::Pending
                }
                None => self.fail(),
            }
        } else {
            if self.key_index(event.pos).is_some() && !self.all_pressed() {
                return self.fail();
            }
            Recognition::Pending
        }
    }

    fn tick(&mut self, now_ms: u64) -> Recognition<KeyCode> {
        if self.done {
            return Recognition::Pending;
        }
        if let Some(start) = self.start_ms {
            if now_ms.wrapping_sub(start) >= self.combo_term_ms {
                return self.fail();
            }
        }
        Recognition::Pending
    }

    fn next_deadline_ms(&self) -> Option<u64> {
        if self.done {
            return None;
        }
        self.start_ms.map(|s| s.wrapping_add(self.combo_term_ms))
    }

    fn reset(&mut self) {
        self.pressed = [false; N];
        self.start_ms = None;
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

    const A: KeyPos = KeyPos::new(0, 0);
    const B: KeyPos = KeyPos::new(0, 1);
    const C: KeyPos = KeyPos::new(0, 2);
    const TERM: u64 = 50;

    fn two_key_combo() -> ComboRecognizer<2> {
        ComboRecognizer::new([A, B], KeyCode::Enter, TERM)
    }

    #[test]
    fn fires_when_both_keys_pressed_within_window() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        let result = rec.observe(press(B, 10), 10);
        assert_eq!(result, Recognition::Matched(KeyCode::Enter));
    }

    #[test]
    fn fires_in_any_order() {
        let mut rec = two_key_combo();
        rec.observe(press(B, 0), 0);
        let result = rec.observe(press(A, 10), 10);
        assert_eq!(result, Recognition::Matched(KeyCode::Enter));
    }

    #[test]
    fn pending_after_first_key_only() {
        let mut rec = two_key_combo();
        let result = rec.observe(press(A, 0), 0);
        assert!(result.is_pending());
    }

    #[test]
    fn fails_on_non_combo_key_press() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        let result = rec.observe(press(C, 10), 10);
        assert!(result.is_failed());
    }

    #[test]
    fn fails_on_timeout_via_tick() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        let result = rec.tick(TERM);
        assert!(result.is_failed());
    }

    #[test]
    fn tick_pending_before_deadline() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        assert!(rec.tick(TERM - 1).is_pending());
    }

    #[test]
    fn tick_pending_with_no_start() {
        let mut rec = two_key_combo();
        assert!(rec.tick(100).is_pending());
    }

    #[test]
    fn next_deadline_none_before_first_press() {
        let rec = two_key_combo();
        assert!(rec.next_deadline_ms().is_none());
    }

    #[test]
    fn next_deadline_set_after_first_press() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        assert_eq!(rec.next_deadline_ms(), Some(TERM));
    }

    #[test]
    fn next_deadline_none_after_resolution() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 10), 10);
        assert!(rec.next_deadline_ms().is_none());
    }

    #[test]
    fn fails_if_combo_key_released_before_all_pressed() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        let result = rec.observe(release(A, 10), 10);
        assert!(result.is_failed());
    }

    #[test]
    fn release_of_combo_key_after_completion_is_pending() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 5), 5);
        let result = rec.observe(release(A, 20), 20);
        assert!(result.is_pending());
    }

    #[test]
    fn observe_after_done_is_pending() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 5), 5);
        let result = rec.observe(press(C, 10), 10);
        assert!(result.is_pending());
    }

    #[test]
    fn tick_after_done_is_pending() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 5), 5);
        assert!(rec.tick(100).is_pending());
    }

    #[test]
    fn reset_clears_all_state() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        rec.reset();
        assert!(rec.next_deadline_ms().is_none());
        let result = rec.observe(press(A, 0), 0);
        assert!(result.is_pending());
    }

    #[test]
    fn three_key_combo_requires_all_three() {
        let mut rec: ComboRecognizer<3> = ComboRecognizer::new([A, B, C], KeyCode::Space, TERM);
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 5), 5);
        let result = rec.observe(press(C, 10), 10);
        assert_eq!(result, Recognition::Matched(KeyCode::Space));
    }

    #[test]
    fn three_key_combo_still_pending_with_two() {
        let mut rec: ComboRecognizer<3> = ComboRecognizer::new([A, B, C], KeyCode::Space, TERM);
        rec.observe(press(A, 0), 0);
        let result = rec.observe(press(B, 5), 5);
        assert!(result.is_pending());
    }

    #[test]
    fn non_combo_release_during_collection_is_pending() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        let result = rec.observe(release(C, 5), 5);
        assert!(result.is_pending());
    }
}
