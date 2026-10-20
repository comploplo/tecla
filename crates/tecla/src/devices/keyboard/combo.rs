//! Combo keys — simultaneous key presses mapped to a single action.
//!
//! A [`Combo`] is defined by the keycodes of its member keys. The live combo
//! path is the processor's inline state machine, which treats combos as
//! steno-style *chords* — it accumulates the pressed keys and fires the
//! matching combo's action the instant the held set matches (no timing
//! window), holding it — repeating exactly like any other held key — until
//! the first member releases.

use crate::action::{Action, KeyCode};

/// A combo (chord) definition.
///
/// Combos are defined by the **keycodes** of their member keys, not by matrix
/// position: the processor resolves each pressed key to the keycode it produces
/// on the base layer and matches on that. So you write the key names (`A`, `F`,
/// …) exactly as in the keymap, never raw `(row, col)` coordinates.
///
/// The processor suppresses the individual key presses while a chord is
/// building. The instant every member key is held, it presses `action` —
/// held and repeating like any other key — and releases it on the first
/// member key-up; if the held set never matches before all keys release, the
/// buffered key events are replayed normally instead.
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
/// Chords have no timing window — they resolve the instant the held set
/// matches — so this is just the set of defined combos.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ModifierKey;

    static DF: &[KeyCode] = &[KeyCode::D, KeyCode::F];
    static COMBOS: &[Combo] = &[Combo::key(DF, KeyCode::Backspace)];

    #[test]
    fn key_combo_taps_the_given_keycode() {
        let c = Combo::key(DF, KeyCode::Backspace);
        assert_eq!(c.action, Action::Key(KeyCode::Backspace));
        assert_eq!(c.keys, DF);
    }

    #[test]
    fn new_combo_carries_an_arbitrary_action() {
        let action = Action::ModifiedKey {
            modifier: ModifierKey::LSHIFT,
            key: KeyCode::A,
        };
        assert_eq!(Combo::<()>::new(DF, action).action, action);
    }

    #[test]
    fn contains_matches_only_member_keys() {
        let c = Combo::key(DF, KeyCode::Backspace);
        assert!(c.contains(KeyCode::D));
        assert!(c.contains(KeyCode::F));
        assert!(!c.contains(KeyCode::G));
    }

    #[test]
    fn config_round_trips_its_table() {
        assert_eq!(ComboConfig::new(COMBOS).combos(), COMBOS);
    }

    #[test]
    fn empty_config_has_no_combos() {
        assert!(ComboConfig::<()>::empty().combos().is_empty());
        assert_eq!(ComboConfig::<()>::default(), ComboConfig::empty());
    }
}
