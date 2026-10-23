//! Cumulative typing speed, excluding pauses between typing bursts.
//!
//! The first character of each burst establishes its timing baseline. Subsequent
//! characters separated by at most two seconds contribute their characters and
//! elapsed time. Corrections consume active time without adding characters.
//! Longer pauses contribute neither. Distinct characters in the same scan still
//! count, even though their timestamps match; a rate needs nonzero active time.

use crate::action::KeyCode;

/// Maximum gap between characters in one active typing burst.
pub const TYPING_IDLE_MS: u64 = 2_000;

/// A since-start typing-speed snapshot, independent of raw physical key counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypingSnapshot {
    /// Average words per active minute, using five characters per word.
    pub active_wpm: u16,
    /// Sum of qualifying inter-character intervals in milliseconds.
    pub active_ms: u64,
    /// All accepted characters, including the first character of every burst.
    pub characters: u64,
}

/// Small, allocation-free collector for accepted human typing.
///
/// Callers supply accepted text inputs, excluding generated playback and
/// shortcuts. No timer ticks are needed; an unchanged snapshot stays unchanged
/// throughout idle time. Lifetime totals survive pausing collection.
pub struct TypingTelemetry {
    characters: u64,
    timed_characters: u64,
    active_ms: u64,
    last_activity_ms: Option<u64>,
    enabled: bool,
}

impl TypingTelemetry {
    /// Creates an enabled collector with no typing history.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            characters: 0,
            timed_characters: 0,
            active_ms: 0,
            last_activity_ms: None,
            enabled: true,
        }
    }

    /// Returns whether collection is enabled.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Pauses or resumes collection without clearing cumulative totals.
    ///
    /// A state change starts a fresh timing baseline, excluding disabled time.
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.last_activity_ms = None;
            self.enabled = enabled;
        }
    }

    /// Records an accepted human text character at its monotonic timestamp.
    ///
    /// Identical timestamps count distinct characters without adding time.
    /// Backwards timestamps count the character total but do not contribute a
    /// speed sample or move the timing baseline backwards.
    pub fn record_character(&mut self, now_ms: u64) {
        if !self.enabled {
            return;
        }
        self.characters = self.characters.saturating_add(1);
        if let Some(previous) = self.last_activity_ms {
            let Some(elapsed_ms) = now_ms.checked_sub(previous) else {
                return;
            };
            if elapsed_ms <= TYPING_IDLE_MS {
                self.active_ms = self.active_ms.saturating_add(elapsed_ms);
                self.timed_characters = self.timed_characters.saturating_add(1);
            }
        }
        self.last_activity_ms = Some(now_ms);
    }

    /// Records correction effort without adding a character.
    ///
    /// Backspace and Delete consume active time during a typing burst. Before
    /// any character, or after a pause longer than two seconds, corrections do
    /// not establish a character baseline for the next burst.
    pub fn record_activity(&mut self, now_ms: u64) {
        if !self.enabled {
            return;
        }
        let Some(previous) = self.last_activity_ms else {
            return;
        };
        let Some(elapsed_ms) = now_ms.checked_sub(previous) else {
            return;
        };
        if elapsed_ms == 0 {
            return;
        }
        if elapsed_ms <= TYPING_IDLE_MS {
            self.active_ms = self.active_ms.saturating_add(elapsed_ms);
            self.last_activity_ms = Some(now_ms);
        } else {
            self.last_activity_ms = None;
        }
    }

    /// Records printable text or a correction from an accepted human keypress.
    ///
    /// Enter and Tab each count as one whitespace character. Backspace and
    /// Delete consume active time without adding characters. Modifiers,
    /// navigation, and function keys are excluded. Callers must also exclude
    /// control shortcuts and generated text before calling.
    pub fn record_keycode(&mut self, keycode: KeyCode, now_ms: u64) {
        let usage = keycode.usage_id();
        if matches!(usage, 0x04..=0x28 | 0x2B..=0x38 | 0x54..=0x64 | 0x67) {
            self.record_character(now_ms);
        } else if matches!(keycode, KeyCode::Backspace | KeyCode::Delete) {
            self.record_activity(now_ms);
        }
    }

    /// Returns cumulative speed over qualifying active intervals, rounded down.
    #[must_use]
    pub fn wpm(&self) -> u16 {
        if self.active_ms == 0 {
            return 0;
        }
        let words_per_minute =
            u128::from(self.timed_characters) * 12_000 / u128::from(self.active_ms);
        words_per_minute.min(u128::from(u16::MAX)) as u16
    }

    /// Returns lifetime totals without advancing time or resetting collection.
    #[must_use]
    pub fn snapshot(&self) -> TypingSnapshot {
        TypingSnapshot {
            active_wpm: self.wpm(),
            active_ms: self.active_ms,
            characters: self.characters,
        }
    }
}

impl Default for TypingTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl super::TelemetryExt for TypingTelemetry {
    type Snapshot = TypingSnapshot;

    fn snapshot_state(&self) -> Self::Snapshot {
        self.snapshot()
    }

    fn clear_deltas(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::TelemetryExt;

    #[test]
    fn first_character_establishes_a_baseline_even_after_long_uptime() {
        let mut typing = TypingTelemetry::new();
        assert_eq!(typing.snapshot(), TypingSnapshot::default());
        typing.record_character(86_400_000);
        assert_eq!(typing.snapshot().characters, 1);
        assert_eq!(typing.wpm(), 0);
        assert_eq!(typing.snapshot().active_ms, 0);
    }

    #[test]
    fn separate_bursts_keep_their_speed_without_counting_the_break() {
        let mut typing = TypingTelemetry::new();
        for base in [0, 3_600_000] {
            for index in 0..51 {
                typing.record_character(base + index * 200);
            }
        }
        assert_eq!(typing.wpm(), 60);
        assert_eq!(typing.snapshot().active_ms, 20_000);
        assert_eq!(typing.snapshot().characters, 102);
    }

    #[test]
    fn average_weights_bursts_by_active_duration() {
        let mut typing = TypingTelemetry::new();
        for now_ms in [0, 100, 200, 10_000, 10_300, 10_600] {
            typing.record_character(now_ms);
        }
        assert_eq!(typing.wpm(), 60);
        assert_eq!(typing.snapshot().active_ms, 800);
    }

    #[test]
    fn idle_boundary_is_inclusive_and_longer_gaps_start_a_new_burst() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(0);
        typing.record_character(TYPING_IDLE_MS);
        assert_eq!(typing.wpm(), 6);
        let before_pause = typing.snapshot();
        typing.record_character(TYPING_IDLE_MS * 2 + 1);
        assert_eq!(typing.snapshot().active_ms, before_pause.active_ms);
        assert_eq!(typing.wpm(), before_pause.active_wpm);
        typing.record_character(TYPING_IDLE_MS * 2 + 201);
        assert_eq!(typing.snapshot().active_ms, 2_200);
    }

    #[test]
    fn characters_in_the_same_scan_count_without_dividing_by_zero() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(0);
        typing.record_character(0);
        assert_eq!(typing.wpm(), 0);
        typing.record_character(100);
        typing.record_character(200);
        assert_eq!(typing.snapshot().characters, 4);
        assert_eq!(typing.snapshot().active_ms, 200);
        assert_eq!(typing.wpm(), 180);
    }

    #[test]
    fn out_of_order_events_do_not_move_the_timing_baseline_backwards() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(100);
        typing.record_character(50);
        assert_eq!(typing.wpm(), 0);
        typing.record_character(300);
        assert_eq!(typing.snapshot().characters, 3);
        assert_eq!(typing.snapshot().active_ms, 200);
        assert_eq!(typing.wpm(), 60);
    }

    #[test]
    fn pause_and_resume_preserve_totals_and_exclude_disabled_time() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(0);
        typing.record_character(200);
        let before_pause = typing.snapshot();
        typing.set_enabled(false);
        typing.record_character(400);
        assert!(!typing.enabled());
        assert_eq!(typing.snapshot(), before_pause);
        typing.set_enabled(true);
        typing.record_character(500);
        assert!(typing.enabled());
        assert_eq!(typing.snapshot().active_ms, 200);
        typing.record_character(700);
        assert_eq!(typing.wpm(), 60);
        assert_eq!(typing.snapshot().characters, 4);
    }

    #[test]
    fn repeated_enable_and_delta_collection_preserve_continuity() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(0);
        typing.set_enabled(true);
        typing.record_character(200);
        let snapshot = typing.collect();
        assert_eq!(typing.snapshot(), snapshot);
        typing.record_character(400);
        assert_eq!(typing.snapshot().active_ms, 400);
        assert_eq!(typing.wpm(), 60);
    }

    #[test]
    fn text_keycodes_count_but_controls_do_not_add_characters() {
        let mut typing = TypingTelemetry::new();
        for keycode in [
            KeyCode::A,
            KeyCode::N0,
            KeyCode::Space,
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Slash,
            KeyCode::Kp1,
            KeyCode::KpEnter,
            KeyCode::NonUsBackslash,
        ] {
            typing.record_keycode(keycode, 100);
        }
        for keycode in [
            KeyCode::None,
            KeyCode::LShift,
            KeyCode::Escape,
            KeyCode::F1,
            KeyCode::Left,
            KeyCode::NumLock,
        ] {
            typing.record_keycode(keycode, 200);
        }
        assert_eq!(typing.snapshot().characters, 9);
        assert_eq!(typing.wpm(), 0);
    }

    #[test]
    fn corrections_cost_active_time_without_adding_characters() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(0);
        typing.record_character(200);
        typing.record_keycode(KeyCode::Backspace, 400);
        typing.record_keycode(KeyCode::Delete, 600);
        assert_eq!(typing.snapshot().characters, 2);
        assert_eq!(typing.snapshot().active_ms, 600);
        assert_eq!(typing.wpm(), 20);
        typing.record_character(800);
        assert_eq!(typing.snapshot().characters, 3);
        assert_eq!(typing.snapshot().active_ms, 800);
        assert_eq!(typing.wpm(), 30);
    }

    #[test]
    fn non_typing_keys_neither_extend_bursts_nor_consume_active_time() {
        let mut typing = TypingTelemetry::new();
        typing.record_keycode(KeyCode::A, 0);
        typing.record_keycode(KeyCode::LShift, 1_000);
        typing.record_keycode(KeyCode::Left, 1_800);
        typing.record_keycode(KeyCode::F1, 2_000);
        typing.record_keycode(KeyCode::B, 2_001);
        assert_eq!(typing.snapshot().characters, 2);
        assert_eq!(typing.snapshot().active_ms, 0);
        typing.record_keycode(KeyCode::C, 2_201);
        assert_eq!(typing.wpm(), 60);
        assert_eq!(typing.snapshot().active_ms, 200);
    }

    #[test]
    fn corrections_do_not_start_a_burst_or_join_across_an_idle_gap() {
        let mut typing = TypingTelemetry::new();
        typing.record_activity(0);
        typing.record_character(200);
        assert_eq!(typing.snapshot().active_ms, 0);
        typing.record_character(400);
        let snapshot = typing.snapshot();
        typing.record_activity(2_401);
        typing.record_activity(2_500);
        typing.record_character(2_600);
        assert_eq!(typing.snapshot().active_ms, snapshot.active_ms);
        assert_eq!(typing.wpm(), snapshot.active_wpm);
        typing.record_character(2_800);
        assert_eq!(typing.wpm(), 60);
    }

    #[test]
    fn correction_timing_has_the_same_boundary_and_ordering_rules() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(100);
        typing.record_activity(100);
        typing.record_activity(0);
        assert_eq!(typing.snapshot().active_ms, 0);
        typing.record_activity(2_100);
        assert_eq!(typing.snapshot().active_ms, 2_000);
        typing.record_character(2_300);
        assert_eq!(typing.snapshot().characters, 2);
        assert_eq!(typing.wpm(), 5);
    }

    #[test]
    fn disabled_corrections_and_characters_preserve_the_snapshot() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(0);
        typing.record_character(200);
        typing.set_enabled(false);
        let snapshot = typing.snapshot();
        typing.record_keycode(KeyCode::Backspace, 400);
        typing.record_keycode(KeyCode::A, 600);
        typing.record_activity(800);
        assert_eq!(typing.snapshot(), snapshot);
    }

    #[test]
    fn isolated_characters_never_create_a_rate() {
        let mut typing = TypingTelemetry::new();
        for now_ms in [0, 10_000, 20_000] {
            typing.record_character(now_ms);
        }
        assert_eq!(typing.snapshot().characters, 3);
        assert_eq!(typing.wpm(), 0);
        assert_eq!(typing.snapshot().active_ms, 0);
    }

    #[test]
    fn long_uptime_and_saturated_totals_do_not_wrap() {
        let mut typing = TypingTelemetry::new();
        typing.record_character(u64::MAX - 400);
        typing.record_character(u64::MAX - 200);
        typing.record_character(u64::MAX);
        assert_eq!(typing.wpm(), 60);

        typing.characters = u64::MAX;
        typing.timed_characters = u64::MAX;
        typing.active_ms = u64::MAX;
        typing.last_activity_ms = Some(0);
        typing.record_character(200);
        assert_eq!(typing.snapshot().characters, u64::MAX);
        assert_eq!(typing.snapshot().active_ms, u64::MAX);
        assert_eq!(typing.wpm(), 12_000);

        typing.active_ms = 1;
        assert_eq!(typing.wpm(), u16::MAX);
    }
}
