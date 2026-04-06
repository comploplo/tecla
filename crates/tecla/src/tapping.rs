//! Tap-vs-hold disambiguation.
//!
//! [`TapHoldRecognizer`] implements [`Recognizer`] for a single mod-tap or
//! layer-tap key. It is created at the moment the tap-hold key is pressed and
//! resolves to a [`TapHoldDecision`] when the ambiguity is resolved.
//!
//! After resolution, the recognizer's internal waiting buffer (events that
//! arrived during disambiguation) should be drained and replayed into the
//! pipeline via [`TapHoldRecognizer::drain_pending`].

use heapless::Deque;

use crate::action::{Action, KeyCode, ModifierKey};
use crate::event::{KeyEvent, KeyPos};
use crate::recognizer::{Recognition, Recognizer};

// ── TapHoldDecision ───────────────────────────────────────────────────────────

/// The resolved outcome of tap-hold disambiguation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapHoldDecision {
    /// Key tapped — emit this keycode as a regular keypress.
    Tap(KeyCode),
    /// Key tapped — toggle this layer on/off (TapToggle tap outcome).
    TapToggleLayer(u8),
    /// Key held — activate this modifier until the key is released.
    HoldMod(ModifierKey),
    /// Key held — activate this layer until the key is released.
    HoldLayer(u8),
}

// ── TapHoldMode ───────────────────────────────────────────────────────────────

/// Controls how the tap-hold disambiguator handles interrupting key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TapHoldMode {
    /// Resolve as hold only after `tapping_term_ms` expires with no other
    /// key activity.
    #[default]
    Default,
    /// Resolve as hold immediately when another key is pressed during
    /// the tapping term (QMK `HOLD_ON_OTHER_KEY_PRESS`).
    HoldOnOtherKeyPress,
    /// Resolve as hold when another key is both pressed and released
    /// during the tapping term (QMK `PERMISSIVE_HOLD`).
    PermissiveHold,
}

// ── TapHoldConfig ─────────────────────────────────────────────────────────────

/// Configuration for a tap-hold key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapHoldConfig {
    /// How long the key must be held before it resolves as hold (milliseconds).
    pub tapping_term_ms: u64,
    /// Interrupt behaviour during the tapping window.
    pub mode: TapHoldMode,
}

impl TapHoldConfig {
    /// Creates a config with the given tapping term and default mode.
    #[must_use]
    pub const fn new(tapping_term_ms: u64) -> Self {
        Self {
            tapping_term_ms,
            mode: TapHoldMode::Default,
        }
    }
}

impl Default for TapHoldConfig {
    fn default() -> Self {
        Self::new(200)
    }
}

// ── Internal state ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Deciding tap vs. hold; key was pressed at `press_ms`.
    Deciding { press_ms: u64 },
    /// Resolved — recognizer is done.
    Done,
}

// ── TapHoldRecognizer ─────────────────────────────────────────────────────────

/// Disambiguates a single mod-tap or layer-tap key press.
///
/// # Lifecycle
///
/// 1. Construct with [`TapHoldRecognizer::mod_tap`],
///    [`TapHoldRecognizer::layer_tap`], or [`TapHoldRecognizer::from_action`],
///    passing the action and the initial press event.
/// 2. Feed subsequent events via [`observe`](Recognizer::observe).
/// 3. When [`tick`](Recognizer::tick) is called at or after
///    [`next_deadline_ms`](Recognizer::next_deadline_ms), the tapping term
///    has expired → resolves as hold.
/// 4. On [`Recognition::Matched`] or [`Recognition::Failed`], drain buffered
///    events with [`drain_pending`](TapHoldRecognizer::drain_pending) and
///    replay them.
///
/// `BUF` is the maximum number of events buffered during disambiguation.
/// Defaults to 32.
pub struct TapHoldRecognizer<const BUF: usize = 32> {
    config: TapHoldConfig,
    tap_key_pos: KeyPos,
    tap_keycode: KeyCode,
    hold_action: HoldAction,
    state: State,
    /// Events buffered while the recognizer is in `Deciding` state.
    waiting: Deque<KeyEvent, BUF>,
    /// For `PermissiveHold`: tracks whether another key was pressed.
    saw_other_press: bool,
    tap_toggle_layer: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldAction {
    Mod(ModifierKey),
    Layer(u8),
}

impl<const BUF: usize> TapHoldRecognizer<BUF> {
    /// Creates a recognizer for a `ModTap` action.
    #[must_use]
    pub fn mod_tap(
        config: TapHoldConfig,
        hold: ModifierKey,
        tap: KeyCode,
        press_event: KeyEvent,
    ) -> Self {
        Self {
            config,
            tap_key_pos: press_event.pos,
            tap_keycode: tap,
            hold_action: HoldAction::Mod(hold),
            state: State::Deciding {
                press_ms: press_event.timestamp_ms,
            },
            waiting: Deque::new(),
            saw_other_press: false,
            tap_toggle_layer: None,
        }
    }

    /// Creates a recognizer for a `LayerTap` action.
    #[must_use]
    pub fn layer_tap(
        config: TapHoldConfig,
        layer: u8,
        tap: KeyCode,
        press_event: KeyEvent,
    ) -> Self {
        Self {
            config,
            tap_key_pos: press_event.pos,
            tap_keycode: tap,
            hold_action: HoldAction::Layer(layer),
            state: State::Deciding {
                press_ms: press_event.timestamp_ms,
            },
            waiting: Deque::new(),
            saw_other_press: false,
            tap_toggle_layer: None,
        }
    }

    /// Creates a recognizer for a `TapToggle` action.
    #[must_use]
    pub fn tap_toggle(config: TapHoldConfig, layer: u8, press_event: KeyEvent) -> Self {
        Self {
            config,
            tap_key_pos: press_event.pos,
            tap_keycode: KeyCode::None,
            hold_action: HoldAction::Layer(layer),
            state: State::Deciding {
                press_ms: press_event.timestamp_ms,
            },
            waiting: Deque::new(),
            saw_other_press: false,
            tap_toggle_layer: Some(layer),
        }
    }

    /// Creates a recognizer from a generic [`Action`] if it is a tap-hold variant.
    ///
    /// Returns `None` for non-tap-hold actions.
    #[must_use]
    pub fn from_action<Ext: Copy + PartialEq>(
        config: TapHoldConfig,
        action: Action<Ext>,
        press_event: KeyEvent,
    ) -> Option<Self> {
        match action {
            Action::ModTap { hold, tap } => Some(Self::mod_tap(config, hold, tap, press_event)),
            Action::LayerTap { layer, tap } => {
                Some(Self::layer_tap(config, layer, tap, press_event))
            }
            Action::TapToggle(layer) => Some(Self::tap_toggle(config, layer, press_event)),
            _ => None,
        }
    }

    /// Drains all buffered events, calling `f` for each in order.
    ///
    /// Call this after the recognizer produces [`Recognition::Matched`] or
    /// [`Recognition::Failed`] to replay the buffered events into the pipeline.
    pub fn drain_pending<F: FnMut(KeyEvent)>(&mut self, mut f: F) {
        while let Some(event) = self.waiting.pop_front() {
            f(event);
        }
    }

    /// Returns `true` if the recognizer has buffered events waiting to be
    /// drained.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Returns the position of the tap-hold key.
    #[must_use]
    pub fn tap_pos(&self) -> KeyPos {
        self.tap_key_pos
    }

    fn resolve_tap(&mut self) -> Recognition<TapHoldDecision> {
        self.state = State::Done;
        if let Some(layer) = self.tap_toggle_layer {
            Recognition::Matched(TapHoldDecision::TapToggleLayer(layer))
        } else {
            Recognition::Matched(TapHoldDecision::Tap(self.tap_keycode))
        }
    }

    fn resolve_hold(&mut self) -> Recognition<TapHoldDecision> {
        self.state = State::Done;
        let decision = match self.hold_action {
            HoldAction::Mod(m) => TapHoldDecision::HoldMod(m),
            HoldAction::Layer(n) => TapHoldDecision::HoldLayer(n),
        };
        Recognition::Matched(decision)
    }
}

impl<const BUF: usize> Recognizer for TapHoldRecognizer<BUF> {
    type Output = TapHoldDecision;

    #[expect(
        clippy::unused_result_ok,
        reason = "heapless push overflow silently dropped in no_std"
    )]
    fn observe(&mut self, event: KeyEvent, now_ms: u64) -> Recognition<TapHoldDecision> {
        let press_ms = match self.state {
            State::Deciding { press_ms } => press_ms,
            State::Done => return Recognition::Pending,
        };

        // Tapping term expired — resolve as hold.
        if now_ms.wrapping_sub(press_ms) >= self.config.tapping_term_ms {
            return self.resolve_hold();
        }

        // Original tap-hold key released before term → tap.
        if event.pos == self.tap_key_pos && !event.pressed {
            return self.resolve_tap();
        }

        // Another key event arrived during the tapping window.
        match self.config.mode {
            TapHoldMode::HoldOnOtherKeyPress if event.pressed => {
                return self.resolve_hold();
            }
            TapHoldMode::PermissiveHold => {
                if event.pressed {
                    self.saw_other_press = true;
                } else if self.saw_other_press {
                    return self.resolve_hold();
                }
            }
            _ => {}
        }

        // Buffer the event for replay after resolution.
        self.waiting.push_back(event).ok();
        Recognition::Pending
    }

    fn tick(&mut self, now_ms: u64) -> Recognition<TapHoldDecision> {
        match self.state {
            State::Deciding { press_ms }
                if now_ms.wrapping_sub(press_ms) >= self.config.tapping_term_ms =>
            {
                self.resolve_hold()
            }
            _ => Recognition::Pending,
        }
    }

    fn next_deadline_ms(&self) -> Option<u64> {
        match self.state {
            State::Deciding { press_ms } => {
                Some(press_ms.wrapping_add(self.config.tapping_term_ms))
            }
            State::Done => None,
        }
    }

    fn reset(&mut self) {
        self.state = State::Done;
        self.waiting.clear();
        self.saw_other_press = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERM: u64 = 200;
    const CFG: TapHoldConfig = TapHoldConfig::new(TERM);

    fn pos(row: u8, col: u8) -> KeyPos {
        KeyPos::new(row, col)
    }

    fn press(pos: KeyPos, ms: u64) -> KeyEvent {
        KeyEvent::press(pos, ms)
    }

    fn release(pos: KeyPos, ms: u64) -> KeyEvent {
        KeyEvent::release(pos, ms)
    }

    fn mod_tap_rec(press_ms: u64) -> TapHoldRecognizer<32> {
        TapHoldRecognizer::mod_tap(
            CFG,
            ModifierKey::LSHIFT,
            KeyCode::A,
            press(pos(0, 0), press_ms),
        )
    }

    fn layer_tap_rec(press_ms: u64) -> TapHoldRecognizer<32> {
        TapHoldRecognizer::layer_tap(CFG, 1, KeyCode::Space, press(pos(0, 0), press_ms))
    }

    // ── Tap resolution ────────────────────────────────────────────────────────

    #[test]
    fn tap_resolved_when_key_released_within_tapping_term() {
        let mut rec = mod_tap_rec(0);
        let result = rec.observe(release(pos(0, 0), 100), 100);
        assert_eq!(
            result,
            Recognition::Matched(TapHoldDecision::Tap(KeyCode::A))
        );
    }

    #[test]
    fn tap_for_layer_tap_resolved_within_tapping_term() {
        let mut rec = layer_tap_rec(0);
        let result = rec.observe(release(pos(0, 0), 100), 100);
        assert_eq!(
            result,
            Recognition::Matched(TapHoldDecision::Tap(KeyCode::Space))
        );
    }

    // ── Hold resolution ───────────────────────────────────────────────────────

    #[test]
    fn hold_resolved_when_tick_fires_at_tapping_term() {
        let mut rec = mod_tap_rec(0);
        let result = rec.tick(TERM);
        assert_eq!(
            result,
            Recognition::Matched(TapHoldDecision::HoldMod(ModifierKey::LSHIFT))
        );
    }

    #[test]
    fn hold_resolved_when_observe_called_after_tapping_term() {
        let mut rec = mod_tap_rec(0);
        let other = press(pos(0, 1), TERM + 1);
        let result = rec.observe(other, TERM + 1);
        assert_eq!(
            result,
            Recognition::Matched(TapHoldDecision::HoldMod(ModifierKey::LSHIFT))
        );
    }

    #[test]
    fn hold_layer_tap_resolved_after_tapping_term() {
        let mut rec = layer_tap_rec(0);
        let result = rec.tick(TERM);
        assert_eq!(result, Recognition::Matched(TapHoldDecision::HoldLayer(1)));
    }

    #[test]
    fn next_deadline_is_press_time_plus_tapping_term() {
        let rec = mod_tap_rec(50);
        assert_eq!(rec.next_deadline_ms(), Some(50 + TERM));
    }

    #[test]
    fn next_deadline_is_none_after_resolution() {
        let mut rec = mod_tap_rec(0);
        rec.tick(TERM);
        assert_eq!(rec.next_deadline_ms(), None);
    }

    // ── HoldOnOtherKeyPress mode ──────────────────────────────────────────────

    #[test]
    fn hokp_resolves_hold_immediately_on_other_key_press() {
        let cfg = TapHoldConfig {
            tapping_term_ms: TERM,
            mode: TapHoldMode::HoldOnOtherKeyPress,
        };
        let mut rec: TapHoldRecognizer<32> =
            TapHoldRecognizer::mod_tap(cfg, ModifierKey::LSHIFT, KeyCode::A, press(pos(0, 0), 0));
        let result = rec.observe(press(pos(0, 1), 50), 50);
        assert_eq!(
            result,
            Recognition::Matched(TapHoldDecision::HoldMod(ModifierKey::LSHIFT))
        );
    }

    #[test]
    fn hokp_does_not_resolve_hold_on_other_key_release() {
        let cfg = TapHoldConfig {
            tapping_term_ms: TERM,
            mode: TapHoldMode::HoldOnOtherKeyPress,
        };
        let mut rec: TapHoldRecognizer<32> =
            TapHoldRecognizer::mod_tap(cfg, ModifierKey::LSHIFT, KeyCode::A, press(pos(0, 0), 0));
        let result = rec.observe(release(pos(0, 1), 50), 50);
        assert!(result.is_pending());
    }

    // ── PermissiveHold mode ───────────────────────────────────────────────────

    #[test]
    fn permissive_hold_resolves_on_nested_press_and_release() {
        let cfg = TapHoldConfig {
            tapping_term_ms: TERM,
            mode: TapHoldMode::PermissiveHold,
        };
        let mut rec: TapHoldRecognizer<32> =
            TapHoldRecognizer::mod_tap(cfg, ModifierKey::LSHIFT, KeyCode::A, press(pos(0, 0), 0));
        rec.observe(press(pos(0, 1), 50), 50);
        let result = rec.observe(release(pos(0, 1), 100), 100);
        assert_eq!(
            result,
            Recognition::Matched(TapHoldDecision::HoldMod(ModifierKey::LSHIFT))
        );
    }

    #[test]
    fn permissive_hold_does_not_resolve_on_release_without_prior_press() {
        let cfg = TapHoldConfig {
            tapping_term_ms: TERM,
            mode: TapHoldMode::PermissiveHold,
        };
        let mut rec: TapHoldRecognizer<32> =
            TapHoldRecognizer::mod_tap(cfg, ModifierKey::LSHIFT, KeyCode::A, press(pos(0, 0), 0));
        let result = rec.observe(release(pos(0, 1), 50), 50);
        assert!(result.is_pending());
    }

    // ── Default mode: other keys are buffered ─────────────────────────────────

    #[test]
    fn default_mode_buffers_other_key_events() {
        let mut rec = mod_tap_rec(0);
        rec.observe(press(pos(0, 1), 50), 50);
        assert!(rec.has_pending());
    }

    #[test]
    fn drain_pending_yields_buffered_events_in_order() {
        let mut rec = mod_tap_rec(0);
        let e1 = press(pos(0, 1), 50);
        let e2 = press(pos(0, 2), 60);
        rec.observe(e1, 50);
        rec.observe(e2, 60);
        rec.tick(TERM); // resolve

        let mut drained = heapless::Vec::<KeyEvent, 8>::new();
        rec.drain_pending(|e| drained.push(e).ok().unwrap_or(()));
        assert_eq!(drained.get(0), Some(&e1));
        assert_eq!(drained.get(1), Some(&e2));
    }

    #[test]
    fn drain_pending_clears_the_buffer() {
        let mut rec = mod_tap_rec(0);
        rec.observe(press(pos(0, 1), 50), 50);
        rec.tick(TERM);
        rec.drain_pending(|_| {});
        assert!(!rec.has_pending());
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_state_and_buffer() {
        let mut rec = mod_tap_rec(0);
        rec.observe(press(pos(0, 1), 50), 50);
        rec.reset();
        assert!(!rec.has_pending());
        assert_eq!(rec.next_deadline_ms(), None);
    }

    // ── from_action ──────────────────────────────────────────────────────────

    #[test]
    fn from_action_mod_tap_creates_recognizer() {
        let action = Action::<()>::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        };
        let rec: Option<TapHoldRecognizer<32>> =
            TapHoldRecognizer::from_action(CFG, action, press(pos(0, 0), 0));
        assert!(rec.is_some());
    }

    #[test]
    fn from_action_layer_tap_creates_recognizer() {
        let action = Action::<()>::LayerTap {
            layer: 2,
            tap: KeyCode::Space,
        };
        let rec: Option<TapHoldRecognizer<32>> =
            TapHoldRecognizer::from_action(CFG, action, press(pos(0, 0), 0));
        assert!(rec.is_some());
    }

    #[test]
    fn from_action_non_tap_hold_returns_none() {
        let action = Action::<()>::Key(KeyCode::A);
        let rec: Option<TapHoldRecognizer<32>> =
            TapHoldRecognizer::from_action(CFG, action, press(pos(0, 0), 0));
        assert!(rec.is_none());
    }

    // ── TapHoldConfig ────────────────────────────────────────────────────────

    #[test]
    fn default_config_has_200ms_tapping_term() {
        assert_eq!(TapHoldConfig::default().tapping_term_ms, 200);
    }

    #[test]
    fn default_config_mode_is_default() {
        assert_eq!(TapHoldConfig::default().mode, TapHoldMode::Default);
    }

    #[test]
    fn observe_after_done_returns_pending() {
        let mut rec = mod_tap_rec(0);
        rec.tick(TERM);
        let result = rec.observe(press(pos(0, 1), TERM + 10), TERM + 10);
        assert!(result.is_pending());
    }

    #[test]
    fn tick_before_deadline_returns_pending() {
        let mut rec = mod_tap_rec(0);
        let result = rec.tick(TERM - 1);
        assert!(result.is_pending());
    }

    #[test]
    fn tick_after_done_returns_pending() {
        let mut rec = mod_tap_rec(0);
        rec.tick(TERM);
        let result = rec.tick(TERM + 100);
        assert!(result.is_pending());
    }
}
