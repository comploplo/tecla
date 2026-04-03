//! Modifier key state tracking.
//!
//! QMK distinguishes three modifier stacks that are OR'd together when
//! building the HID report:
//!
//! - **real** — explicitly held modifier keys
//! - **weak** — applied only while another non-modifier key is also pressed
//!   (used by mod-tap tap resolution to avoid stray hotkeys)
//! - **oneshot** — applied once to the next keypress, then automatically
//!   cleared

use crate::action::ModifierKey;

/// Tracks all active modifier keys across three logical stacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierState {
    /// Always-active modifiers (explicitly held keys).
    real: ModifierKey,
    /// Active only while another key is co-pressed (mod-tap tap side).
    weak: ModifierKey,
    /// Applied once on the next keypress, then cleared.
    oneshot: ModifierKey,
}

impl ModifierState {
    /// Creates a new, empty [`ModifierState`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            real: ModifierKey::NONE,
            weak: ModifierKey::NONE,
            oneshot: ModifierKey::NONE,
        }
    }

    // ── Real modifiers ────────────────────────────────────────────────────

    /// Adds `mods` to the real modifier stack.
    pub fn press(&mut self, mods: ModifierKey) {
        self.real = self.real.union(mods);
    }

    /// Removes `mods` from the real modifier stack.
    pub fn release(&mut self, mods: ModifierKey) {
        self.real = self.real.without(mods);
    }

    /// Returns the currently held real modifiers.
    #[must_use]
    pub const fn real(&self) -> ModifierKey {
        self.real
    }

    // ── Weak modifiers ────────────────────────────────────────────────────

    /// Sets the weak modifier mask (replaces, not OR).
    ///
    /// Call this when a mod-tap key resolves as a tap — the modifier should
    /// apply to that tap action only.
    pub fn set_weak(&mut self, mods: ModifierKey) {
        self.weak = mods;
    }

    /// Clears all weak modifiers.
    pub fn clear_weak(&mut self) {
        self.weak = ModifierKey::NONE;
    }

    /// Returns the current weak modifier mask.
    #[must_use]
    pub const fn weak(&self) -> ModifierKey {
        self.weak
    }

    // ── Oneshot modifiers ─────────────────────────────────────────────────

    /// Registers a oneshot modifier. It will apply once and then clear.
    pub fn register_oneshot(&mut self, mods: ModifierKey) {
        self.oneshot = self.oneshot.union(mods);
    }

    /// Consumes and returns the pending oneshot modifiers, clearing them.
    ///
    /// Call this when a non-modifier key is pressed while oneshot is active.
    pub fn consume_oneshot(&mut self) -> ModifierKey {
        let consumed = self.oneshot;
        self.oneshot = ModifierKey::NONE;
        consumed
    }

    /// Returns the current oneshot modifier mask without consuming it.
    #[must_use]
    pub const fn oneshot(&self) -> ModifierKey {
        self.oneshot
    }

    // ── Combined report byte ──────────────────────────────────────────────

    /// Returns the effective modifier byte for the current HID report.
    ///
    /// This is the union of all three stacks.
    #[must_use]
    pub fn report_byte(&self) -> ModifierKey {
        self.real.union(self.weak).union(self.oneshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_no_modifiers() {
        let state = ModifierState::new();
        assert!(state.report_byte().is_empty());
    }

    #[test]
    fn default_equals_new() {
        assert_eq!(ModifierState::default(), ModifierState::new());
    }

    #[test]
    fn pressing_modifier_appears_in_real() {
        let mut state = ModifierState::new();
        state.press(ModifierKey::LSHIFT);
        assert!(state.real().contains(ModifierKey::LSHIFT));
    }

    #[test]
    fn releasing_modifier_clears_it_from_real() {
        let mut state = ModifierState::new();
        state.press(ModifierKey::LSHIFT);
        state.release(ModifierKey::LSHIFT);
        assert!(!state.real().contains(ModifierKey::LSHIFT));
    }

    #[test]
    fn releasing_unpressed_modifier_is_a_noop() {
        let mut state = ModifierState::new();
        state.release(ModifierKey::LCTRL);
        assert!(state.real().is_empty());
    }

    #[test]
    fn multiple_real_modifiers_accumulate() {
        let mut state = ModifierState::new();
        state.press(ModifierKey::LCTRL);
        state.press(ModifierKey::LSHIFT);
        assert!(state.real().contains(ModifierKey::LCTRL));
        assert!(state.real().contains(ModifierKey::LSHIFT));
    }

    #[test]
    fn release_only_clears_specified_modifier() {
        let mut state = ModifierState::new();
        state.press(ModifierKey::LCTRL);
        state.press(ModifierKey::LSHIFT);
        state.release(ModifierKey::LCTRL);
        assert!(!state.real().contains(ModifierKey::LCTRL));
        assert!(state.real().contains(ModifierKey::LSHIFT));
    }

    #[test]
    fn weak_modifier_appears_in_report_byte() {
        let mut state = ModifierState::new();
        state.set_weak(ModifierKey::LALT);
        assert!(state.report_byte().contains(ModifierKey::LALT));
    }

    #[test]
    fn clear_weak_removes_weak_modifier() {
        let mut state = ModifierState::new();
        state.set_weak(ModifierKey::LALT);
        state.clear_weak();
        assert!(state.weak().is_empty());
    }

    #[test]
    fn set_weak_replaces_previous_weak_modifier() {
        let mut state = ModifierState::new();
        state.set_weak(ModifierKey::LALT);
        state.set_weak(ModifierKey::LGUI);
        assert!(!state.weak().contains(ModifierKey::LALT));
        assert!(state.weak().contains(ModifierKey::LGUI));
    }

    #[test]
    fn oneshot_appears_in_report_byte() {
        let mut state = ModifierState::new();
        state.register_oneshot(ModifierKey::LSHIFT);
        assert!(state.report_byte().contains(ModifierKey::LSHIFT));
    }

    #[test]
    fn consume_oneshot_returns_pending_mods() {
        let mut state = ModifierState::new();
        state.register_oneshot(ModifierKey::LSHIFT);
        let consumed = state.consume_oneshot();
        assert_eq!(consumed, ModifierKey::LSHIFT);
    }

    #[test]
    fn consume_oneshot_clears_the_oneshot_stack() {
        let mut state = ModifierState::new();
        state.register_oneshot(ModifierKey::LSHIFT);
        state.consume_oneshot();
        assert!(state.oneshot().is_empty());
    }

    #[test]
    fn report_byte_combines_all_three_stacks() {
        let mut state = ModifierState::new();
        state.press(ModifierKey::LCTRL);
        state.set_weak(ModifierKey::LSHIFT);
        state.register_oneshot(ModifierKey::LALT);
        let report = state.report_byte();
        assert!(report.contains(ModifierKey::LCTRL));
        assert!(report.contains(ModifierKey::LSHIFT));
        assert!(report.contains(ModifierKey::LALT));
    }

    #[test]
    fn stacks_are_independent_of_each_other() {
        let mut state = ModifierState::new();
        state.press(ModifierKey::LCTRL);
        state.clear_weak();
        state.consume_oneshot();
        assert!(state.real().contains(ModifierKey::LCTRL));
    }
}
