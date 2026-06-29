//! Combo keys — simultaneous key presses mapped to a single action.
//!
//! A [`ComboRecognizer`] accumulates presses from a fixed set of [`KeyPos`]
//! keys. When all `N` keys are pressed within `combo_term_ms`, it resolves
//! with the configured [`Action`]. Any non-combo key press or timeout before
//! completion causes a [`Recognition::Failed`].
//!
//! Note: the live combo path is the processor's inline state machine, which
//! treats combos as steno-style *chords* — it accumulates the pressed keys and
//! fires the matching combo's action on full release (no timing window). The
//! standalone [`ComboRecognizer`] below keeps the older eager/windowed model
//! and is currently unused by the processor.

use crate::action::{Action, KeyCode};
use crate::pipeline::event::{KeyEvent, KeyPos};
use crate::pipeline::recognizer::{Recognition, Recognizer};

/// A combo (chord) definition.
///
/// Combos are defined by the **keycodes** of their member keys, not by matrix
/// position: the processor resolves each pressed key to the keycode it produces
/// on the base layer and matches on that. So you write the key names (`A`, `F`,
/// …) exactly as in the keymap, never raw `(row, col)` coordinates.
///
/// The processor suppresses the individual key presses while a chord is
/// building. When every member key is held and then fully released, it taps
/// `action`; otherwise it replays the buffered key events normally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo<Ext = ()>
where
    Ext: Copy + PartialEq,
{
    /// Keycodes (base-layer identities) that must be pressed together.
    pub keys: &'static [KeyCode],
    /// Action tapped when the combo resolves.
    pub action: Action<Ext>,
}

impl<Ext> Combo<Ext>
where
    Ext: Copy + PartialEq,
{
    /// Creates a new combo.
    #[must_use]
    pub const fn new(keys: &'static [KeyCode], action: Action<Ext>) -> Self {
        Self { keys, action }
    }

    /// Returns `true` if `key` participates in this combo.
    #[must_use]
    pub fn contains(&self, key: KeyCode) -> bool {
        self.keys.contains(&key)
    }
}

impl Combo<()> {
    /// Creates a combo that taps a keycode.
    #[must_use]
    pub const fn key(keys: &'static [KeyCode], key: KeyCode) -> Self {
        Self::new(keys, Action::Key(key))
    }
}

/// Combo (chord) table.
///
/// Chords are release-to-fire: there is no timing window, so this is just the
/// set of defined combos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComboConfig<Ext = ()>
where
    Ext: Copy + PartialEq + 'static,
{
    combos: &'static [Combo<Ext>],
}

impl<Ext> ComboConfig<Ext>
where
    Ext: Copy + PartialEq + 'static,
{
    /// Creates a combo config from a table of chords.
    #[must_use]
    pub const fn new(combos: &'static [Combo<Ext>]) -> Self {
        Self { combos }
    }

    /// Creates an empty combo config.
    #[must_use]
    pub const fn empty() -> Self {
        Self::new(&[])
    }

    /// Configured combos.
    #[must_use]
    pub const fn combos(self) -> &'static [Combo<Ext>] {
        self.combos
    }
}

impl<Ext> Default for ComboConfig<Ext>
where
    Ext: Copy + PartialEq + 'static,
{
    fn default() -> Self {
        Self::empty()
    }
}

/// Recognizes a simultaneous `N`-key combo.
///
/// # Lifecycle
///
/// 1. Construct with [`ComboRecognizer::new`].
/// 2. Feed events via [`observe`](Recognizer::observe).
/// 3. When all `N` keys are pressed within `combo_term_ms`, yields
///    [`Recognition::Matched`] with the configured action.
/// 4. A non-combo key press or a [`tick`](Recognizer::tick) past the deadline
///    yields [`Recognition::Failed`] — the caller should replay buffered events.
pub struct ComboRecognizer<const N: usize, Ext = ()>
where
    Ext: Copy + PartialEq,
{
    keys: [KeyPos; N],
    action: Action<Ext>,
    combo_term_ms: u64,
    pressed: [bool; N],
    start_ms: Option<u64>,
    done: bool,
}

impl<const N: usize, Ext> ComboRecognizer<N, Ext>
where
    Ext: Copy + PartialEq,
{
    /// Creates a new combo recognizer.
    ///
    /// `keys` is the set of positions that must all be pressed within
    /// `combo_term_ms` milliseconds.
    #[must_use]
    pub fn new(keys: [KeyPos; N], action: Action<Ext>, combo_term_ms: u64) -> Self {
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

    fn fail(&mut self) -> Recognition<Action<Ext>> {
        self.done = true;
        Recognition::Failed
    }
}

impl<const N: usize> ComboRecognizer<N> {
    /// Creates a combo recognizer that resolves to [`Action::Key`].
    #[must_use]
    pub fn key(keys: [KeyPos; N], key: KeyCode, combo_term_ms: u64) -> Self {
        Self::new(keys, Action::Key(key), combo_term_ms)
    }
}

impl<const N: usize, Ext> Recognizer for ComboRecognizer<N, Ext>
where
    Ext: Copy + PartialEq,
{
    type Output = Action<Ext>;

    fn observe(&mut self, event: KeyEvent, now_ms: u64) -> Recognition<Action<Ext>> {
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

    fn tick(&mut self, now_ms: u64) -> Recognition<Action<Ext>> {
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
    use crate::action::ModifierKey;

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
        ComboRecognizer::key([A, B], KeyCode::Enter, TERM)
    }

    #[test]
    fn fires_when_both_keys_pressed_within_window() {
        let mut rec = two_key_combo();
        rec.observe(press(A, 0), 0);
        let result = rec.observe(press(B, 10), 10);
        assert_eq!(result, Recognition::Matched(Action::Key(KeyCode::Enter)));
    }

    #[test]
    fn fires_in_any_order() {
        let mut rec = two_key_combo();
        rec.observe(press(B, 0), 0);
        let result = rec.observe(press(A, 10), 10);
        assert_eq!(result, Recognition::Matched(Action::Key(KeyCode::Enter)));
    }

    #[test]
    fn fires_with_standard_action() {
        let action = Action::ModifiedKey {
            modifier: ModifierKey::LSHIFT,
            key: KeyCode::A,
        };
        let mut rec: ComboRecognizer<2> = ComboRecognizer::new([A, B], action, TERM);
        rec.observe(press(A, 0), 0);
        let result = rec.observe(press(B, 10), 10);
        assert_eq!(result, Recognition::Matched(action));
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
        let mut rec: ComboRecognizer<3> = ComboRecognizer::key([A, B, C], KeyCode::Space, TERM);
        rec.observe(press(A, 0), 0);
        rec.observe(press(B, 5), 5);
        let result = rec.observe(press(C, 10), 10);
        assert_eq!(result, Recognition::Matched(Action::Key(KeyCode::Space)));
    }

    #[test]
    fn three_key_combo_still_pending_with_two() {
        let mut rec: ComboRecognizer<3> = ComboRecognizer::key([A, B, C], KeyCode::Space, TERM);
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
