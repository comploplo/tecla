//! Layer-based keymap resolution.
//!
//! A [`LayerStack`] holds up to `LAYERS` [`Layer`]s identified by a bitmask.
//! On key press, the highest active layer with a non-[`Action::Transparent`]
//! entry wins. The resolved layer index is cached so that key release uses
//! the same action even if layers change mid-hold.

use crate::action::{Action, LayerOp};
use crate::event::KeyPos;

/// Sentinel value meaning "no layer cached for this key".
const NO_CACHE: u8 = u8::MAX;

/// A single layer: a fixed-size array of actions indexed by flat key position.
///
/// Flat index is computed as `row * COLS + col` by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer<Ext = ()>
where
    Ext: Copy + PartialEq,
{
    actions: heapless::Vec<Action<Ext>, 256>,
}

impl<Ext: Copy + PartialEq> Layer<Ext> {
    /// Creates a layer from a slice of actions.
    ///
    /// Actions are stored in row-major flat order.
    #[must_use]
    pub fn new(actions: &[Action<Ext>]) -> Self {
        let mut v = heapless::Vec::new();
        for &a in actions {
            v.push(a).ok();
        }
        Self { actions: v }
    }

    /// Returns the action at flat index `idx`, or `None` if out of bounds.
    #[must_use]
    pub fn get(&self, idx: usize) -> Option<Action<Ext>> {
        self.actions.get(idx).copied()
    }

    /// Returns the number of keys in this layer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Returns `true` if the layer has no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// A stack of up to `LAYERS` layers with bitmask-based activation.
///
/// Layer 0 is the default base layer. Higher-numbered layers take precedence.
/// [`Action::Transparent`] entries fall through to the next lower active layer.
/// [`Action::NoOp`] explicitly blocks fallthrough.
///
/// A per-key layer cache stores the resolved layer index at press time and
/// uses it at release time, preventing stuck keys when layers change mid-hold.
pub struct LayerStack<const LAYERS: usize, const KEYS: usize, Ext = ()>
where
    Ext: Copy + PartialEq,
{
    layers: heapless::Vec<Layer<Ext>, LAYERS>,
    /// Bitmask of currently active non-default layers.
    state: u32,
    /// Bitmask of default layer(s); layer 0 is always bit 0.
    default_state: u32,
    /// Per-key cache: layer index resolved at press time.
    press_cache: [u8; KEYS],
}

impl<const LAYERS: usize, const KEYS: usize, Ext: Copy + PartialEq> LayerStack<LAYERS, KEYS, Ext> {
    /// Creates a new [`LayerStack`] with the provided layers.
    ///
    /// Layer 0 is activated as the default base layer.
    #[must_use]
    pub fn new(layers: heapless::Vec<Layer<Ext>, LAYERS>) -> Self {
        Self {
            layers,
            state: 0,
            default_state: 1, // layer 0 is default
            press_cache: [NO_CACHE; KEYS],
        }
    }

    /// Resolves the action for a key at `pos` using `cols` to compute the flat
    /// index.
    ///
    /// Searches active layers top-down, skipping [`Action::Transparent`]
    /// entries. Falls back to [`Action::NoOp`] if no non-transparent action is
    /// found.
    #[must_use]
    pub fn resolve(&self, pos: KeyPos, cols: usize) -> Action<Ext> {
        let idx = pos.flat(cols);
        let active = self.state | self.default_state;

        for layer_idx in (0..LAYERS).rev() {
            if active & (1 << layer_idx) == 0 {
                continue;
            }
            if let Some(layer) = self.layers.get(layer_idx) {
                if let Some(action) = layer.get(idx) {
                    if action != Action::Transparent {
                        return action;
                    }
                }
            }
        }

        Action::NoOp
    }

    /// Records the resolved layer for `pos` at press time.
    ///
    /// Call this immediately after resolving a key press. The cached layer is
    /// used by [`release_action`](LayerStack::release_action) to ensure the
    /// same action fires on release regardless of subsequent layer changes.
    pub fn cache_press(&mut self, pos: KeyPos, cols: usize) {
        let idx = pos.flat(cols);
        let active = self.state | self.default_state;
        let mut resolved_layer: u8 = 0;

        for layer_idx in (0..LAYERS).rev() {
            if active & (1 << layer_idx) == 0 {
                continue;
            }
            if let Some(layer) = self.layers.get(layer_idx) {
                if let Some(action) = layer.get(idx) {
                    if action != Action::Transparent {
                        resolved_layer = layer_idx.try_into().unwrap_or(0);
                        break;
                    }
                }
            }
        }

        if let Some(cell) = self.press_cache.get_mut(idx) {
            *cell = resolved_layer;
        }
    }

    /// Returns the action for `pos` using the layer cached at press time.
    ///
    /// Falls back to the current resolution if no cache entry exists.
    #[must_use]
    pub fn release_action(&self, pos: KeyPos, cols: usize) -> Action<Ext> {
        let idx = pos.flat(cols);
        let cached_layer = self.press_cache.get(idx).copied().unwrap_or(NO_CACHE);

        if cached_layer == NO_CACHE {
            return self.resolve(pos, cols);
        }

        self.layers
            .get(usize::from(cached_layer))
            .and_then(|l| l.get(idx))
            .unwrap_or(Action::NoOp)
    }

    /// Clears the press cache entry for `pos`.
    ///
    /// Call this after processing a key release.
    pub fn clear_cache(&mut self, pos: KeyPos, cols: usize) {
        let idx = pos.flat(cols);
        if let Some(cell) = self.press_cache.get_mut(idx) {
            *cell = NO_CACHE;
        }
    }

    /// Applies a [`LayerOp`] to the layer stack.
    pub fn apply(&mut self, op: LayerOp) {
        match op {
            LayerOp::Momentary(n) | LayerOp::On(n) => {
                self.state |= 1 << u32::from(n);
            }
            LayerOp::Off(n) => {
                self.state &= !(1 << u32::from(n));
            }
            LayerOp::Toggle(n) => {
                self.state ^= 1 << u32::from(n);
            }
            LayerOp::SetDefault(n) => {
                self.default_state = 1 << u32::from(n);
            }
        }
    }

    /// Returns the active layer bitmask (non-default layers).
    #[must_use]
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// Returns the default layer bitmask.
    #[must_use]
    pub const fn default_state(&self) -> u32 {
        self.default_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{KeyCode, ModifierKey};

    const COLS: usize = 3;

    fn pos(row: u8, col: u8) -> KeyPos {
        KeyPos::new(row, col)
    }

    fn make_stack(base: &[Action], overlay: Option<&[Action]>) -> LayerStack<2, 6> {
        let mut layers: heapless::Vec<Layer, 2> = heapless::Vec::new();
        layers.push(Layer::new(base)).ok();
        if let Some(o) = overlay {
            layers.push(Layer::new(o)).ok();
        }
        LayerStack::new(layers)
    }

    #[test]
    fn resolves_key_from_base_layer() {
        let stack = make_stack(
            &[
                Action::Key(KeyCode::A),
                Action::Key(KeyCode::B),
                Action::Key(KeyCode::C),
                Action::Key(KeyCode::D),
                Action::Key(KeyCode::E),
                Action::Key(KeyCode::F),
            ],
            None,
        );
        assert_eq!(stack.resolve(pos(0, 0), COLS), Action::Key(KeyCode::A));
        assert_eq!(stack.resolve(pos(0, 2), COLS), Action::Key(KeyCode::C));
    }

    #[test]
    fn higher_layer_wins_over_base() {
        let mut stack = make_stack(
            &[Action::Key(KeyCode::A); 6],
            Some(&[Action::Key(KeyCode::B); 6]),
        );
        stack.apply(LayerOp::On(1));
        assert_eq!(stack.resolve(pos(0, 0), COLS), Action::Key(KeyCode::B));
    }

    #[test]
    fn transparent_falls_through_to_lower_layer() {
        let mut stack = make_stack(
            &[Action::Key(KeyCode::A); 6],
            Some(&[Action::Transparent; 6]),
        );
        stack.apply(LayerOp::On(1));
        assert_eq!(stack.resolve(pos(0, 0), COLS), Action::Key(KeyCode::A));
    }

    #[test]
    fn noop_blocks_transparent_fallthrough() {
        let mut stack = make_stack(&[Action::Key(KeyCode::A); 6], Some(&[Action::NoOp; 6]));
        stack.apply(LayerOp::On(1));
        assert_eq!(stack.resolve(pos(0, 0), COLS), Action::NoOp);
    }

    #[test]
    fn layer_on_activates_layer() {
        let mut stack: LayerStack<2, 6> = make_stack(&[Action::NoOp; 6], None);
        stack.apply(LayerOp::On(1));
        assert!(stack.state() & (1 << 1) != 0);
    }

    #[test]
    fn layer_off_deactivates_layer() {
        let mut stack: LayerStack<2, 6> = make_stack(&[Action::NoOp; 6], None);
        stack.apply(LayerOp::On(1));
        stack.apply(LayerOp::Off(1));
        assert_eq!(stack.state() & (1 << 1), 0);
    }

    #[test]
    fn layer_toggle_flips_state() {
        let mut stack: LayerStack<2, 6> = make_stack(&[Action::NoOp; 6], None);
        stack.apply(LayerOp::Toggle(1));
        assert!(stack.state() & (1 << 1) != 0);
        stack.apply(LayerOp::Toggle(1));
        assert_eq!(stack.state() & (1 << 1), 0);
    }

    #[test]
    fn set_default_changes_base_layer() {
        let mut stack: LayerStack<2, 6> = make_stack(&[Action::NoOp; 6], None);
        stack.apply(LayerOp::SetDefault(1));
        assert_eq!(stack.default_state(), 1 << 1);
    }

    #[test]
    fn press_cache_stores_resolved_layer() {
        let mut stack = make_stack(
            &[Action::Key(KeyCode::A); 6],
            Some(&[Action::Key(KeyCode::B); 6]),
        );
        stack.apply(LayerOp::On(1));
        stack.cache_press(pos(0, 0), COLS);
        // Deactivate layer 1 after press
        stack.apply(LayerOp::Off(1));
        // Release action should still use layer 1 (cached)
        assert_eq!(
            stack.release_action(pos(0, 0), COLS),
            Action::Key(KeyCode::B)
        );
    }

    #[test]
    fn release_action_without_cache_uses_current_layer() {
        let stack = make_stack(&[Action::Key(KeyCode::A); 6], None);
        assert_eq!(
            stack.release_action(pos(0, 0), COLS),
            Action::Key(KeyCode::A)
        );
    }

    #[test]
    fn clear_cache_removes_press_cache_entry() {
        let mut stack = make_stack(
            &[Action::Key(KeyCode::A); 6],
            Some(&[Action::Key(KeyCode::B); 6]),
        );
        stack.apply(LayerOp::On(1));
        stack.cache_press(pos(0, 0), COLS);
        stack.apply(LayerOp::Off(1));
        stack.clear_cache(pos(0, 0), COLS);
        // After clear, release falls back to current layer (layer 0)
        assert_eq!(
            stack.release_action(pos(0, 0), COLS),
            Action::Key(KeyCode::A)
        );
    }

    #[test]
    fn layer_api_is_accessible() {
        let layer: Layer = Layer::new(&[Action::Key(KeyCode::A), Action::Key(KeyCode::B)]);
        assert_eq!(layer.len(), 2);
        assert!(!layer.is_empty());
        assert_eq!(layer.get(0), Some(Action::Key(KeyCode::A)));
        assert_eq!(layer.get(2), None);
    }

    #[test]
    fn empty_layer_is_empty() {
        let layer: Layer = Layer::new(&[]);
        assert!(layer.is_empty());
        assert_eq!(layer.len(), 0);
    }

    #[test]
    fn modifier_action_resolves_from_layer() {
        let stack = make_stack(&[Action::Modifier(ModifierKey::LSHIFT); 6], None);
        assert_eq!(
            stack.resolve(pos(0, 0), COLS),
            Action::Modifier(ModifierKey::LSHIFT)
        );
    }

    #[test]
    fn resolve_returns_noop_when_all_layers_transparent() {
        let stack = make_stack(&[Action::Transparent; 6], None);
        assert_eq!(stack.resolve(pos(0, 0), COLS), Action::NoOp);
    }

    #[test]
    fn resolve_returns_noop_for_out_of_bounds_key() {
        let stack = make_stack(&[Action::Key(KeyCode::A); 6], None);
        assert_eq!(stack.resolve(pos(2, 0), COLS), Action::NoOp);
    }

    #[test]
    fn resolve_with_active_layer_index_beyond_pushed_layers() {
        let mut stack = make_stack(&[Action::Key(KeyCode::A); 6], None);
        stack.apply(LayerOp::On(1));
        assert_eq!(stack.resolve(pos(0, 0), COLS), Action::Key(KeyCode::A));
    }

    #[test]
    fn cache_press_uses_layer_zero_when_all_transparent() {
        let mut stack = make_stack(&[Action::Transparent; 6], None);
        stack.cache_press(pos(0, 0), COLS);
        assert_eq!(stack.release_action(pos(0, 0), COLS), Action::Transparent);
    }

    #[test]
    fn cache_press_for_out_of_bounds_key_uses_layer_zero() {
        let mut stack = make_stack(&[Action::Key(KeyCode::A); 6], None);
        stack.cache_press(pos(2, 0), COLS);
        let action = stack.release_action(pos(2, 0), COLS);
        assert_eq!(action, Action::NoOp);
    }

    #[test]
    fn cache_press_with_active_layer_index_beyond_pushed_layers() {
        let mut stack = make_stack(&[Action::Key(KeyCode::A); 6], None);
        stack.apply(LayerOp::On(1));
        stack.cache_press(pos(0, 0), COLS);
        assert_eq!(
            stack.release_action(pos(0, 0), COLS),
            Action::Key(KeyCode::A)
        );
    }
}
