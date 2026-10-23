//! Accessibility input filters.
//!
//! Three modes following the X11 AccessX / Windows FilterKeys conventions:
//!
//! - **Slow keys** — a press must be held for a configurable duration before
//!   it is accepted. Helps users who accidentally brush keys.
//! - **Bounce keys** — rapid re-presses of the same key within a window are
//!   ignored. Helps users with tremor or motor control difficulty.
//! - **Sticky keys** — modifier presses behave as one-shot: press Shift, then
//!   press A to get Shift+A. No need to hold both simultaneously.

use crate::pipeline::event::KeyPos;

/// Configuration for accessibility input filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccessibilityConfig {
    /// Minimum hold duration (ms) before a press is accepted. `None` = disabled.
    pub slow_keys_ms: Option<u16>,
    /// Minimum time (ms) between release and re-press of the same key. `None` = disabled.
    pub bounce_keys_ms: Option<u16>,
    /// When true, all modifier presses become one-shot.
    pub sticky_keys: bool,
}

impl AccessibilityConfig {
    /// Enable slow keys with the given threshold.
    #[must_use]
    pub fn with_slow_keys(mut self, ms: u16) -> Self {
        self.slow_keys_ms = Some(ms);
        self
    }

    /// Enable bounce keys with the given window.
    #[must_use]
    pub fn with_bounce_keys(mut self, ms: u16) -> Self {
        self.bounce_keys_ms = Some(ms);
        self
    }

    /// Enable sticky keys.
    #[must_use]
    pub fn with_sticky_keys(mut self) -> Self {
        self.sticky_keys = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingSlowKey {
    pos: KeyPos,
    press_time_ms: u64,
}

/// Runtime state for accessibility filters.
pub struct AccessibilityState<const KEYS: usize> {
    /// Active configuration (may be toggled at runtime via actions).
    pub config: AccessibilityConfig,
    pending_slow: heapless::Vec<PendingSlowKey, 8>,
    // Separate validity avoids a padded Option tag per timestamp.
    last_release_time: [u64; KEYS],
    has_release_time: [bool; KEYS],
}

impl<const KEYS: usize> AccessibilityState<KEYS> {
    /// Creates new state from the given config.
    #[must_use]
    pub fn new(config: AccessibilityConfig) -> Self {
        Self {
            config,
            pending_slow: heapless::Vec::new(),
            last_release_time: [0; KEYS],
            has_release_time: [false; KEYS],
        }
    }

    /// Check whether a press event should be accepted, deferred, or rejected.
    pub fn should_accept_press(&mut self, pos: KeyPos, now_ms: u64, cols: usize) -> SlowKeyResult {
        if let Some(bounce_ms) = self.config.bounce_keys_ms {
            let flat = pos.flat(cols);
            if flat < KEYS && self.has_release_time[flat] {
                let elapsed = now_ms.saturating_sub(self.last_release_time[flat]);
                if elapsed < u64::from(bounce_ms) {
                    return SlowKeyResult::Reject;
                }
            }
        }

        if let Some(slow_ms) = self.config.slow_keys_ms {
            self.pending_slow
                .push(PendingSlowKey {
                    pos,
                    press_time_ms: now_ms,
                })
                .ok();
            return SlowKeyResult::Defer(slow_ms);
        }

        SlowKeyResult::Accept
    }

    /// Returns true if the given key has a pending slow-key press.
    #[must_use]
    pub fn is_pending_slow(&self, pos: KeyPos) -> bool {
        self.pending_slow.iter().any(|p| p.pos == pos)
    }

    /// Record a release event timestamp for bounce key tracking.
    pub fn record_release(&mut self, pos: KeyPos, now_ms: u64, cols: usize) {
        let flat = pos.flat(cols);
        if flat < KEYS {
            self.last_release_time[flat] = now_ms;
            self.has_release_time[flat] = true;
        }
        self.pending_slow.retain(|p| p.pos != pos);
    }

    /// Drain any slow key presses that have met their hold threshold.
    pub fn tick_slow_keys(&mut self, now_ms: u64) -> heapless::Vec<KeyPos, 8> {
        let mut ready: heapless::Vec<KeyPos, 8> = heapless::Vec::new();
        let threshold = self.config.slow_keys_ms.unwrap_or(0);
        self.pending_slow.retain(|pending| {
            let elapsed = now_ms.saturating_sub(pending.press_time_ms);
            if elapsed >= u64::from(threshold) {
                ready.push(pending.pos).ok();
                false
            } else {
                true
            }
        });
        ready
    }

    /// Returns the next deadline at which a pending slow key will be ready.
    pub fn next_slow_key_deadline(&self) -> Option<u64> {
        let threshold = self.config.slow_keys_ms?;
        self.pending_slow
            .iter()
            .map(|p| p.press_time_ms + u64::from(threshold))
            .min()
    }
}

/// Result of checking a press against accessibility filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlowKeyResult {
    /// Press accepted immediately.
    Accept,
    /// Press deferred — will fire after the given ms if held.
    Defer(u16),
    /// Press rejected (bounce filter).
    Reject,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounce_filter() -> AccessibilityState<2> {
        AccessibilityState::new(AccessibilityConfig {
            bounce_keys_ms: Some(100),
            ..AccessibilityConfig::default()
        })
    }

    #[test]
    fn first_press_accepts_but_a_zero_timestamp_release_starts_the_bounce_window() {
        let mut state = bounce_filter();
        let key = KeyPos::new(0, 0);
        assert_eq!(state.should_accept_press(key, 0, 2), SlowKeyResult::Accept);
        state.record_release(key, 0, 2);
        assert_eq!(state.should_accept_press(key, 99, 2), SlowKeyResult::Reject);
        assert_eq!(
            state.should_accept_press(key, 100, 2),
            SlowKeyResult::Accept
        );
        // An untouched key has no release history, even at timestamp zero.
        assert_eq!(
            state.should_accept_press(KeyPos::new(0, 1), 0, 2),
            SlowKeyResult::Accept
        );
    }

    #[test]
    fn full_width_timestamps_and_the_exact_bounce_boundary_are_preserved() {
        let mut state = bounce_filter();
        let key = KeyPos::new(0, 0);
        state.record_release(key, u64::MAX - 100, 2);
        assert_eq!(
            state.should_accept_press(key, u64::MAX - 1, 2),
            SlowKeyResult::Reject
        );
        assert_eq!(
            state.should_accept_press(key, u64::MAX, 2),
            SlowKeyResult::Accept
        );
        state.record_release(key, u64::MAX, 2);
        assert_eq!(
            state.should_accept_press(key, u64::MAX, 2),
            SlowKeyResult::Reject
        );
    }

    #[test]
    fn dense_102_key_state_fits_within_optional_timestamp_storage_budget() {
        assert!(
            core::mem::size_of::<AccessibilityState<102>>()
                < core::mem::size_of::<[Option<u64>; 102]>()
        );
    }
}
