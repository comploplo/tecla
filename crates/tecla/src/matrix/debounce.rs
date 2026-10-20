//! Debounce algorithms for filtering electrical noise from switch contacts.
//!
//! Each implementation satisfies the [`Debouncer`] trait. The caller passes
//! the raw matrix scan result and the current timestamp; the debouncer
//! returns the stable (cooked) state when it changes.
//!
//! # Algorithms
//!
//! | Type | Press | Release | Best for |
//! |------|-------|---------|----------|
//! | [`SymDefer`] | deferred | deferred | maximum noise rejection |
//! | [`AsymEagerDefer`] | immediate | deferred | snappy feel, typical MX switches |

use crate::matrix::MatrixState;

/// A debounce algorithm for a `ROWS × COLS` matrix.
pub trait Debouncer<const ROWS: usize, const COLS: usize> {
    /// Process a raw scan result at `now_ms` milliseconds.
    ///
    /// Returns `Some(state)` when the debounced output changes, `None` when
    /// it is still settling or unchanged.
    fn process(
        &mut self,
        raw: &MatrixState<ROWS, COLS>,
        now_ms: u64,
    ) -> Option<MatrixState<ROWS, COLS>>;
}

// ── SymDefer ─────────────────────────────────────────────────────────────────

/// Symmetric deferred debounce — global timer.
///
/// When *any* key changes, a stability timer starts. After `debounce_ms`
/// milliseconds without further change the new state is accepted. Any
/// additional change resets the timer.
///
/// Offers maximum noise rejection at the cost of a small input latency on
/// every press and release.
pub struct SymDefer<const ROWS: usize, const COLS: usize> {
    debounce_ms: u64,
    cooked: MatrixState<ROWS, COLS>,
    pending: MatrixState<ROWS, COLS>,
    changed_at: Option<u64>,
}

impl<const ROWS: usize, const COLS: usize> SymDefer<ROWS, COLS> {
    /// Creates a new [`SymDefer`] with the given debounce window.
    #[must_use]
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            cooked: MatrixState::new(),
            pending: MatrixState::new(),
            changed_at: None,
        }
    }
}

impl<const ROWS: usize, const COLS: usize> Debouncer<ROWS, COLS> for SymDefer<ROWS, COLS> {
    fn process(
        &mut self,
        raw: &MatrixState<ROWS, COLS>,
        now_ms: u64,
    ) -> Option<MatrixState<ROWS, COLS>> {
        if *raw != self.pending {
            self.pending = raw.clone();
            self.changed_at = Some(now_ms);
            return None;
        }

        if let Some(changed_at) = self.changed_at {
            if now_ms.wrapping_sub(changed_at) >= self.debounce_ms {
                self.changed_at = None;
                if self.cooked != self.pending {
                    self.cooked = self.pending.clone();
                    return Some(self.cooked.clone());
                }
            }
        }

        None
    }
}

// ── AsymEagerDefer ───────────────────────────────────────────────────────────

/// Asymmetric eager-press / deferred-release debounce — per key.
///
/// - **Press** transitions are applied immediately (zero latency).
/// - **Release** transitions are deferred by `debounce_ms` milliseconds to
///   filter chatter on key-up (common with MX-style switches).
///
/// A press while a release is pending cancels the pending release.
pub struct AsymEagerDefer<const ROWS: usize, const COLS: usize> {
    debounce_ms: u64,
    cooked: MatrixState<ROWS, COLS>,
    release_pending: [[Option<u64>; COLS]; ROWS],
}

impl<const ROWS: usize, const COLS: usize> AsymEagerDefer<ROWS, COLS> {
    /// Creates a new [`AsymEagerDefer`] with the given debounce window.
    #[must_use]
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            cooked: MatrixState::new(),
            release_pending: [[None; COLS]; ROWS],
        }
    }
}

impl<const ROWS: usize, const COLS: usize> Debouncer<ROWS, COLS> for AsymEagerDefer<ROWS, COLS> {
    fn process(
        &mut self,
        raw: &MatrixState<ROWS, COLS>,
        now_ms: u64,
    ) -> Option<MatrixState<ROWS, COLS>> {
        let mut changed = false;

        for (row, pending_row) in self.release_pending.iter_mut().enumerate() {
            for (col, pending_slot) in pending_row.iter_mut().enumerate() {
                let raw_pressed = raw.get(row, col).unwrap_or(false);
                let cooked_pressed = self.cooked.get(row, col).unwrap_or(false);

                if raw_pressed && !cooked_pressed {
                    self.cooked.set(row, col, true);
                    *pending_slot = None;
                    changed = true;
                } else if !raw_pressed && cooked_pressed {
                    match *pending_slot {
                        None => {
                            *pending_slot = Some(now_ms);
                        }
                        Some(since) => {
                            if now_ms.wrapping_sub(since) >= self.debounce_ms {
                                self.cooked.set(row, col, false);
                                *pending_slot = None;
                                changed = true;
                            }
                        }
                    }
                } else if raw_pressed {
                    *pending_slot = None;
                }
            }
        }

        if changed {
            Some(self.cooked.clone())
        } else {
            None
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    fn pressed<const R: usize, const C: usize>(row: usize, col: usize) -> MatrixState<R, C> {
        let mut s = MatrixState::new();
        s.set(row, col, true);
        s
    }

    fn all_released<const R: usize, const C: usize>() -> MatrixState<R, C> {
        MatrixState::new()
    }

    // ── SymDefer ─────────────────────────────────────────────────────────────

    #[test]
    fn sym_defer_does_not_apply_change_before_debounce_period() {
        let mut d = SymDefer::<4, 12>::new(5);
        let raw = pressed::<4, 12>(0, 0);
        assert!(d.process(&raw, 0).is_none());
        assert!(d.process(&raw, 3).is_none());
    }

    #[test]
    fn sym_defer_applies_stable_change_after_debounce_period() {
        let mut d = SymDefer::<4, 12>::new(5);
        let raw = pressed::<4, 12>(0, 0);
        d.process(&raw, 0);
        let result = d.process(&raw, 5);
        assert_eq!(result.unwrap().get(0, 0), Some(true));
    }

    #[test]
    fn sym_defer_resets_timer_on_new_change_during_debounce() {
        let mut d = SymDefer::<4, 12>::new(5);
        let first = pressed::<4, 12>(0, 0);
        let second = pressed::<4, 12>(0, 1);
        d.process(&first, 0);
        // New change at t=3 resets the timer
        d.process(&second, 3);
        // At t=6, only 3ms have passed since the second change — not yet stable
        assert!(d.process(&second, 6).is_none());
        // At t=8, 5ms have passed since second change — now stable
        let result = d.process(&second, 8).unwrap();
        assert_eq!(result.get(0, 0), Some(false));
        assert_eq!(result.get(0, 1), Some(true));
    }

    #[test]
    fn sym_defer_returns_none_when_state_is_already_cooked() {
        let mut d = SymDefer::<4, 12>::new(5);
        let raw = pressed::<4, 12>(0, 0);
        // Apply the change
        d.process(&raw, 0);
        d.process(&raw, 5);
        // Feed the same raw state again — nothing new
        assert!(d.process(&raw, 10).is_none());
    }

    #[test]
    fn sym_defer_applies_release_after_debounce() {
        let mut d = SymDefer::<4, 12>::new(5);
        let pressed_state = pressed::<4, 12>(0, 0);
        let released_state = all_released::<4, 12>();

        // First, accept the press
        d.process(&pressed_state, 0);
        d.process(&pressed_state, 5);

        // Now release
        d.process(&released_state, 10);
        let result = d.process(&released_state, 15);
        assert_eq!(result.unwrap().get(0, 0), Some(false));
    }

    // ── AsymEagerDefer ───────────────────────────────────────────────────────

    #[test]
    fn asym_eager_applies_press_immediately() {
        let mut d = AsymEagerDefer::<4, 12>::new(5);
        let raw = pressed::<4, 12>(0, 0);
        let result = d.process(&raw, 0);
        assert_eq!(result.unwrap().get(0, 0), Some(true));
    }

    #[test]
    fn asym_eager_does_not_apply_release_immediately() {
        let mut d = AsymEagerDefer::<4, 12>::new(5);
        let press = pressed::<4, 12>(0, 0);
        let release = all_released::<4, 12>();
        d.process(&press, 0);
        assert!(d.process(&release, 1).is_none());
    }

    #[test]
    fn asym_eager_applies_release_after_debounce() {
        let mut d = AsymEagerDefer::<4, 12>::new(5);
        let press = pressed::<4, 12>(0, 0);
        let release = all_released::<4, 12>();
        d.process(&press, 0);
        d.process(&release, 1); // start release timer
        d.process(&release, 4); // not yet
        let result = d.process(&release, 6); // 5ms elapsed
        assert_eq!(result.unwrap().get(0, 0), Some(false));
    }

    #[test]
    fn asym_eager_press_during_release_timer_cancels_release() {
        let mut d = AsymEagerDefer::<4, 12>::new(5);
        let press = pressed::<4, 12>(0, 0);
        let release = all_released::<4, 12>();
        d.process(&press, 0);
        d.process(&release, 1); // start release timer
                                // Key bounces back before timer expires
        let result = d.process(&press, 3);
        // Re-press accepted (already cooked as pressed, so no new event)
        // Then release again
        d.process(&release, 4);
        // Timer starts fresh; should not fire at t=7 (only 3ms since t=4)
        assert!(d.process(&release, 7).is_none());
        // Should fire at t=9 (5ms since t=4)
        assert_eq!(d.process(&release, 9).unwrap().get(0, 0), Some(false));
        // And no spurious events from the bounce
        assert!(result.is_none());
    }

    #[test]
    fn asym_eager_does_not_emit_spurious_event_when_unchanged() {
        let mut d = AsymEagerDefer::<4, 12>::new(5);
        let release = all_released::<4, 12>();
        // Start and end in released state — never pressed
        assert!(d.process(&release, 0).is_none());
        assert!(d.process(&release, 100).is_none());
    }

    #[test]
    fn sym_defer_returns_none_when_settled_state_matches_cooked() {
        let mut d = SymDefer::<4, 12>::new(5);
        let press_state = pressed::<4, 12>(0, 0);
        let release_state = all_released::<4, 12>();

        d.process(&press_state, 0);
        d.process(&press_state, 5);

        d.process(&release_state, 10);
        d.process(&press_state, 12);
        let result = d.process(&press_state, 17);
        assert!(result.is_none());
    }
}
