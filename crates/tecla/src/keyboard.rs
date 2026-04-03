//! Keyboard event pipeline.
//!
//! [`KeyboardProcessor`] is the synchronous core of the keyboard pipeline. It
//! accepts raw [`KeyEvent`]s one at a time and invokes a caller-supplied
//! callback with each [`HidReport`] that should be sent to the host.
//!
//! The processor is MCU-agnostic and has no Embassy dependency — async
//! integration (task topology, channels, timers) lives in the board crates
//! that call into this type.
//!
//! # Pipeline
//!
//! ```text
//! KeyEvent
//!   → tap-hold recognizer (if active)
//!   → layer resolution
//!   → modifier state
//!   → HidReport callback
//! ```
//!
//! # Tap-hold lifecycle
//!
//! When a tap-hold key is pressed a [`TapHoldRecognizer`] is created and all
//! subsequent events are fed to it until it resolves. Events that arrive
//! during disambiguation are buffered in the recognizer and replayed through
//! the pipeline after resolution via [`TapHoldRecognizer::drain_pending`].

use heapless::{Deque, Vec};

use crate::action::{Action, KeyCode, LayerOp};
use crate::event::{KeyEvent, KeyPos};
use crate::keymap::LayerStack;
use crate::modifier::ModifierState;
use crate::recognizer::{Recognition, Recognizer};
use crate::report::{ConsumerReport, HidReport, KeyboardReport, NkroKeyboardReport};
use crate::tapping::{TapHoldConfig, TapHoldDecision, TapHoldRecognizer};

/// Controls whether the keyboard emits 6KRO or NKRO HID reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RolloverMode {
    /// Six-key rollover: standard boot-protocol report, 6 simultaneous keys.
    #[default]
    Kro6,
    /// N-key rollover: bitmap report, up to 128 simultaneous keys.
    Nkro,
}

// ── ActiveHold ────────────────────────────────────────────────────────────────

/// A resolved tap-hold that is still physically held down.
///
/// Stored so that when the physical key is released the correct cleanup action
/// (modifier release or layer deactivation) can be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveHold {
    pos: KeyPos,
    decision: TapHoldDecision,
}

// ── KeyboardProcessor ─────────────────────────────────────────────────────────

/// Synchronous keyboard event pipeline.
///
/// # Type parameters
///
/// - `ROWS`, `COLS` — matrix dimensions
/// - `LAYERS` — maximum number of layers
/// - `KEYS` — total key count (`ROWS * COLS`); used by the layer press cache
/// - `Ext` — extension action type (default `()`)
pub struct KeyboardProcessor<
    const ROWS: usize,
    const COLS: usize,
    const LAYERS: usize,
    const KEYS: usize,
    Ext = (),
> where
    Ext: Copy + PartialEq,
{
    layer_stack: LayerStack<LAYERS, KEYS, Ext>,
    modifier_state: ModifierState,
    tap_hold_config: TapHoldConfig,
    active_tap_hold: Option<TapHoldRecognizer>,
    active_holds: Vec<ActiveHold, 8>,
    pressed_keys: Vec<u8, 16>,
    pending: Deque<KeyEvent, 32>,
    rollover_mode: RolloverMode,
}

impl<const ROWS: usize, const COLS: usize, const LAYERS: usize, const KEYS: usize, Ext>
    KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>
where
    Ext: Copy + PartialEq,
{
    /// Creates a new processor with the given layer stack and tap-hold config.
    #[must_use]
    pub fn new(layer_stack: LayerStack<LAYERS, KEYS, Ext>, tap_hold_config: TapHoldConfig) -> Self {
        Self {
            layer_stack,
            modifier_state: ModifierState::new(),
            tap_hold_config,
            active_tap_hold: None,
            active_holds: Vec::new(),
            pressed_keys: Vec::new(),
            pending: Deque::new(),
            rollover_mode: RolloverMode::Kro6,
        }
    }

    /// Sets the rollover mode. Call this on the result of [`new`](Self::new).
    #[must_use]
    pub fn with_rollover_mode(mut self, mode: RolloverMode) -> Self {
        self.rollover_mode = mode;
        self
    }

    /// Processes a key event, calling `emit` for each HID report produced.
    ///
    /// Events are queued internally and replayed after tap-hold resolution,
    /// so `emit` may be called zero or more times per call.
    pub fn process_event<F>(&mut self, event: KeyEvent, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        self.pending.push_back(event).ok();
        self.drain_pending(now_ms, emit);
    }

    /// Drives the tap-hold timer.
    ///
    /// Call this when the Embassy timer fires at [`next_deadline_ms`]. Emits
    /// a hold-resolved report if the tapping term has expired.
    ///
    /// [`next_deadline_ms`]: KeyboardProcessor::next_deadline_ms
    pub fn tick<F>(&mut self, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if let Some(rec) = self.active_tap_hold.as_mut() {
            let result = rec.tick(now_ms);
            if result.is_pending() {
                return;
            }
            let rec = self.active_tap_hold.take().unwrap();
            self.handle_tap_hold_result(result, rec, now_ms, emit);
            self.drain_pending(now_ms, emit);
        }
    }

    /// Returns the next timer deadline the caller must wake at.
    ///
    /// `None` when no tap-hold is in flight.
    #[must_use]
    pub fn next_deadline_ms(&self) -> Option<u64> {
        self.active_tap_hold
            .as_ref()
            .and_then(|r| r.next_deadline_ms())
    }

    // ── internal ─────────────────────────────────────────────────────────────

    fn drain_pending<F>(&mut self, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        while let Some(event) = self.pending.pop_front() {
            self.process_one(event, now_ms, emit);
        }
    }

    fn process_one<F>(&mut self, event: KeyEvent, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if let Some(rec) = self.active_tap_hold.as_mut() {
            let result = rec.observe(event, now_ms);
            if result.is_pending() {
                return;
            }
            let rec = self.active_tap_hold.take().unwrap();
            self.handle_tap_hold_result(result, rec, now_ms, emit);
            return;
        }

        if event.pressed {
            self.handle_press(event, now_ms, emit);
        } else {
            self.handle_release(event, emit);
        }
    }

    fn handle_press<F>(&mut self, event: KeyEvent, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        self.layer_stack.cache_press(event.pos, COLS);
        let action = self.layer_stack.resolve(event.pos, COLS);

        match action {
            Action::Key(kc) => {
                self.press_keycode(kc, emit);
            }
            Action::Modifier(m) => {
                self.modifier_state.press(m);
                self.emit_keyboard(emit);
            }
            Action::Consumer(cc) => {
                emit(HidReport::Consumer(ConsumerReport { keycode: Some(cc) }));
            }
            Action::Layer(op) => {
                self.layer_stack.apply(op);
            }
            Action::ModTap { .. } | Action::LayerTap { .. } => {
                if let Some(rec) =
                    TapHoldRecognizer::from_action(self.tap_hold_config, action, event)
                {
                    self.active_tap_hold = Some(rec);
                }
            }
            Action::OneShotMod(m) => {
                self.modifier_state.register_oneshot(m);
            }
            Action::OneShotLayer(n) => {
                self.layer_stack.apply(LayerOp::On(n));
            }
            Action::Transparent | Action::NoOp | Action::Ext(_) => {}
        }

        let _ = now_ms;
    }

    fn handle_release<F>(&mut self, event: KeyEvent, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        let action = self.layer_stack.release_action(event.pos, COLS);
        self.layer_stack.clear_cache(event.pos, COLS);

        match action {
            Action::Key(kc) => {
                self.release_keycode(kc, emit);
            }
            Action::Modifier(m) => {
                self.modifier_state.release(m);
                self.emit_keyboard(emit);
            }
            Action::Consumer(_) => {
                emit(HidReport::Consumer(ConsumerReport { keycode: None }));
            }
            Action::Layer(LayerOp::Momentary(n)) => {
                self.layer_stack.apply(LayerOp::Off(n));
            }
            Action::OneShotLayer(n) => {
                self.layer_stack.apply(LayerOp::Off(n));
            }
            _ => {
                if let Some(idx) = self.active_holds.iter().position(|h| h.pos == event.pos) {
                    let hold = self.active_holds.swap_remove(idx);
                    self.apply_hold_release(hold.decision, emit);
                }
            }
        }
    }

    fn handle_tap_hold_result<F>(
        &mut self,
        result: Recognition<TapHoldDecision>,
        mut rec: TapHoldRecognizer,
        now_ms: u64,
        emit: &mut F,
    ) where
        F: FnMut(HidReport) + ?Sized,
    {
        match result {
            Recognition::Matched(TapHoldDecision::Tap(kc)) => {
                self.press_keycode(kc, emit);
                rec.drain_pending(|e| self.pending.push_back(e).ok().unwrap_or(()));
                self.drain_pending(now_ms, emit);
                self.release_keycode(kc, emit);
            }
            Recognition::Matched(decision @ TapHoldDecision::HoldMod(m)) => {
                let pos = rec.tap_pos();
                self.modifier_state.press(m);
                self.emit_keyboard(emit);
                self.active_holds.push(ActiveHold { pos, decision }).ok();
                rec.drain_pending(|e| self.pending.push_back(e).ok().unwrap_or(()));
            }
            Recognition::Matched(decision @ TapHoldDecision::HoldLayer(n)) => {
                let pos = rec.tap_pos();
                self.layer_stack.apply(LayerOp::On(n));
                self.active_holds.push(ActiveHold { pos, decision }).ok();
                rec.drain_pending(|e| self.pending.push_back(e).ok().unwrap_or(()));
            }
            Recognition::Pending | Recognition::Failed => {}
        }
    }

    fn apply_hold_release<F>(&mut self, decision: TapHoldDecision, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        match decision {
            TapHoldDecision::HoldMod(m) => {
                self.modifier_state.release(m);
                self.emit_keyboard(emit);
            }
            TapHoldDecision::HoldLayer(n) => {
                self.layer_stack.apply(LayerOp::Off(n));
            }
            TapHoldDecision::Tap(_) => {}
        }
    }

    fn press_keycode<F>(&mut self, kc: KeyCode, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        let uid = kc.usage_id();
        if uid != 0 && !self.pressed_keys.contains(&uid) {
            self.pressed_keys.push(uid).ok();
        }
        self.emit_keyboard(emit);
        self.modifier_state.consume_oneshot();
    }

    fn release_keycode<F>(&mut self, kc: KeyCode, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        let uid = kc.usage_id();
        if let Some(idx) = self.pressed_keys.iter().position(|&k| k == uid) {
            self.pressed_keys.swap_remove(idx);
        }
        self.modifier_state.clear_weak();
        self.emit_keyboard(emit);
    }

    fn emit_keyboard<F>(&self, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        match self.rollover_mode {
            RolloverMode::Kro6 => emit(HidReport::Keyboard(KeyboardReport::from_state(
                &self.modifier_state,
                &self.pressed_keys,
            ))),
            RolloverMode::Nkro => emit(HidReport::NkroKeyboard(NkroKeyboardReport::from_state(
                &self.modifier_state,
                &self.pressed_keys,
            ))),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::RolloverMode;
    use super::*;
    use crate::action::{Action, KeyCode, LayerOp, ModifierKey};
    use crate::event::{KeyEvent, KeyPos};
    use crate::keymap::{Layer, LayerStack};
    use crate::report::{ConsumerReport, HidReport};
    use crate::tapping::{TapHoldConfig, TapHoldMode};

    const ROWS: usize = 2;
    const COLS: usize = 3;
    const LAYERS: usize = 2;
    const KEYS: usize = 6;

    fn pos(row: u8, col: u8) -> KeyPos {
        KeyPos::new(row, col)
    }

    fn press(pos: KeyPos, ms: u64) -> KeyEvent {
        KeyEvent::press(pos, ms)
    }

    fn release(pos: KeyPos, ms: u64) -> KeyEvent {
        KeyEvent::release(pos, ms)
    }

    fn make_processor(base: &[Action]) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
        let mut layers: heapless::Vec<Layer<()>, LAYERS> = heapless::Vec::new();
        layers.push(Layer::new(base)).ok();
        let stack: LayerStack<LAYERS, KEYS> = LayerStack::new(layers);
        KeyboardProcessor::new(stack, TapHoldConfig::default())
    }

    fn make_processor_two_layers(
        base: &[Action],
        overlay: &[Action],
    ) -> KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> {
        let mut layers: heapless::Vec<Layer<()>, LAYERS> = heapless::Vec::new();
        layers.push(Layer::new(base)).ok();
        layers.push(Layer::new(overlay)).ok();
        let stack: LayerStack<LAYERS, KEYS> = LayerStack::new(layers);
        KeyboardProcessor::new(stack, TapHoldConfig::default())
    }

    fn collect_reports(f: impl FnOnce(&mut dyn FnMut(HidReport))) -> Vec<HidReport, 16> {
        let mut reports: Vec<HidReport, 16> = Vec::new();
        f(&mut |r| {
            reports.push(r).ok();
        });
        reports
    }

    fn keyboard_keycodes(report: HidReport) -> [u8; 6] {
        match report {
            HidReport::Keyboard(r) => r.keycodes,
            HidReport::Consumer(_) => [0; 6],
            HidReport::NkroKeyboard(_) => [0; 6],
        }
    }

    fn keyboard_mods(report: HidReport) -> u8 {
        match report {
            HidReport::Keyboard(r) => r.modifiers,
            HidReport::Consumer(_) => 0,
            HidReport::NkroKeyboard(_) => 0,
        }
    }

    // ── Regular key press / release ───────────────────────────────────────────

    #[test]
    fn press_key_emits_report_with_keycode() {
        let mut proc = make_processor(&[
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 0), 0), 0, emit);
        });
        assert_eq!(reports.len(), 1);
        assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    }

    #[test]
    fn release_key_emits_report_without_keycode() {
        let mut proc = make_processor(&[
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(release(pos(0, 0), 10), 10, emit);
        });
        assert_eq!(reports.len(), 1);
        assert!(!keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    }

    #[test]
    fn multiple_keys_pressed_simultaneously_appear_in_report() {
        let mut proc = make_processor(&[
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 1), 1, emit);
        });
        assert_eq!(reports.len(), 1);
        let kcs = keyboard_keycodes(*reports.first().unwrap());
        assert!(kcs.contains(&KeyCode::A.usage_id()));
        assert!(kcs.contains(&KeyCode::B.usage_id()));
    }

    #[test]
    fn noop_key_emits_no_report() {
        let mut proc = make_processor(&[Action::NoOp; 6]);
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 0), 0), 0, emit);
        });
        assert!(reports.is_empty());
    }

    #[test]
    fn duplicate_key_press_does_not_duplicate_keycode_in_report() {
        let mut proc = make_processor(&[
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 0), 1), 1, emit);
        });
        let kcs = keyboard_keycodes(*reports.first().unwrap());
        let count = kcs.iter().filter(|&&k| k == KeyCode::A.usage_id()).count();
        assert_eq!(count, 1);
    }

    // ── Modifier keys ─────────────────────────────────────────────────────────

    #[test]
    fn modifier_press_sets_modifier_byte() {
        let mut proc = make_processor(&[
            Action::Modifier(ModifierKey::LSHIFT),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 0), 0), 0, emit);
        });
        assert_eq!(reports.len(), 1);
        assert_eq!(
            keyboard_mods(*reports.first().unwrap()),
            ModifierKey::LSHIFT.bits()
        );
    }

    #[test]
    fn modifier_release_clears_modifier_byte() {
        let mut proc = make_processor(&[
            Action::Modifier(ModifierKey::LSHIFT),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(release(pos(0, 0), 10), 10, emit);
        });
        assert_eq!(keyboard_mods(*reports.first().unwrap()), 0);
    }

    // ── Consumer keys ─────────────────────────────────────────────────────────

    #[test]
    fn consumer_press_emits_consumer_report() {
        use crate::action::ConsumerCode;
        let mut proc = make_processor(&[
            Action::Consumer(ConsumerCode::VolumeUp),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 0), 0), 0, emit);
        });
        assert_eq!(reports.len(), 1);
        assert!(matches!(
            *reports.first().unwrap(),
            HidReport::Consumer(ConsumerReport { keycode: Some(_) })
        ));
    }

    #[test]
    fn consumer_release_emits_empty_consumer_report() {
        use crate::action::ConsumerCode;
        let mut proc = make_processor(&[
            Action::Consumer(ConsumerCode::VolumeUp),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(release(pos(0, 0), 10), 10, emit);
        });
        assert!(matches!(
            *reports.first().unwrap(),
            HidReport::Consumer(ConsumerReport { keycode: None })
        ));
    }

    // ── Layer operations ──────────────────────────────────────────────────────

    #[test]
    fn momentary_layer_key_activates_overlay() {
        let base = [
            Action::Layer(LayerOp::Momentary(1)),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ];
        let overlay = [
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ];
        let mut proc = make_processor_two_layers(&base, &overlay);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 1), 1, emit);
        });
        assert_eq!(reports.len(), 1);
        assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
    }

    #[test]
    fn momentary_layer_key_release_deactivates_overlay() {
        let base = [
            Action::Layer(LayerOp::Momentary(1)),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ];
        let overlay = [
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ];
        let mut proc = make_processor_two_layers(&base, &overlay);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        proc.process_event(release(pos(0, 0), 10), 10, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 20), 20, emit);
        });
        assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    }

    // ── Tap-hold: tap resolution ──────────────────────────────────────────────

    #[test]
    fn tap_hold_tap_emits_press_and_release() {
        let mut proc = make_processor(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(release(pos(0, 0), 100), 100, emit);
        });
        assert!(reports.len() >= 2);
        let has_press = reports
            .iter()
            .any(|r| keyboard_keycodes(*r).contains(&KeyCode::A.usage_id()));
        let has_release = reports
            .iter()
            .any(|r| !keyboard_keycodes(*r).contains(&KeyCode::A.usage_id()));
        assert!(has_press);
        assert!(has_release);
    }

    #[test]
    fn tap_hold_tap_does_not_activate_modifier() {
        let mut proc = make_processor(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(release(pos(0, 0), 100), 100, emit);
        });
        assert!(reports.iter().all(|r| keyboard_mods(*r) == 0));
    }

    // ── Tap-hold: hold resolution via tick ────────────────────────────────────

    #[test]
    fn tap_hold_hold_via_tick_activates_modifier() {
        let mut proc = make_processor(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.tick(200, emit);
        });
        assert_eq!(reports.len(), 1);
        assert_eq!(
            keyboard_mods(*reports.first().unwrap()),
            ModifierKey::LSHIFT.bits()
        );
    }

    #[test]
    fn tap_hold_hold_release_clears_modifier() {
        let mut proc = make_processor(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        proc.tick(200, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(release(pos(0, 0), 210), 210, emit);
        });
        assert!(reports.iter().any(|r| keyboard_mods(*r) == 0));
    }

    #[test]
    fn tap_hold_layer_hold_activates_layer() {
        let base = [
            Action::LayerTap {
                layer: 1,
                tap: KeyCode::Space,
            },
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ];
        let overlay = [
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ];
        let mut proc = make_processor_two_layers(&base, &overlay);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        proc.tick(200, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 210), 210, emit);
        });
        assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
    }

    #[test]
    fn tap_hold_layer_hold_release_deactivates_layer() {
        let base = [
            Action::LayerTap {
                layer: 1,
                tap: KeyCode::Space,
            },
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ];
        let overlay = [
            Action::Transparent,
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ];
        let mut proc = make_processor_two_layers(&base, &overlay);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        proc.tick(200, &mut |_| {});
        proc.process_event(release(pos(0, 0), 210), 210, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 220), 220, emit);
        });
        assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    }

    // ── Tap-hold: HoldOnOtherKeyPress ─────────────────────────────────────────

    #[test]
    fn hokp_hold_resolves_on_other_key_press() {
        let cfg = TapHoldConfig {
            tapping_term_ms: 200,
            mode: TapHoldMode::HoldOnOtherKeyPress,
        };
        let mut layers: heapless::Vec<Layer<()>, LAYERS> = heapless::Vec::new();
        layers
            .push(Layer::new(&[
                Action::ModTap {
                    hold: ModifierKey::LSHIFT,
                    tap: KeyCode::A,
                },
                Action::Key(KeyCode::B),
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
            ]))
            .ok();
        let stack: LayerStack<LAYERS, KEYS> = LayerStack::new(layers);
        let mut proc: KeyboardProcessor<ROWS, COLS, LAYERS, KEYS> =
            KeyboardProcessor::new(stack, cfg);

        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 50), 50, emit);
        });
        assert!(reports
            .iter()
            .any(|r| keyboard_mods(*r) == ModifierKey::LSHIFT.bits()));
    }

    // ── next_deadline_ms ──────────────────────────────────────────────────────

    #[test]
    fn next_deadline_ms_is_none_with_no_tap_hold() {
        let proc = make_processor(&[Action::Key(KeyCode::A); 6]);
        assert!(proc.next_deadline_ms().is_none());
    }

    #[test]
    fn next_deadline_ms_is_some_while_tap_hold_pending() {
        let mut proc = make_processor(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        assert!(proc.next_deadline_ms().is_some());
    }

    #[test]
    fn next_deadline_ms_is_none_after_tap_hold_resolves() {
        let mut proc = make_processor(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        proc.tick(200, &mut |_| {});
        assert!(proc.next_deadline_ms().is_none());
    }

    // ── Oneshot modifier ──────────────────────────────────────────────────────

    #[test]
    fn oneshot_mod_applies_to_next_key() {
        let mut proc = make_processor(&[
            Action::OneShotMod(ModifierKey::LSHIFT),
            Action::Key(KeyCode::A),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 10), 10, emit);
        });
        assert!(reports
            .iter()
            .any(|r| keyboard_mods(*r) & ModifierKey::LSHIFT.bits() != 0));
    }

    #[test]
    fn nkro_mode_emits_nkro_report() {
        let layout = [Action::Key(KeyCode::A), Action::NoOp];
        let mut layers = heapless::Vec::<Layer<()>, 1>::new();
        layers.push(Layer::new(&layout)).ok();
        let mut proc: KeyboardProcessor<1, 2, 1, 2> =
            KeyboardProcessor::new(LayerStack::new(layers), TapHoldConfig::default())
                .with_rollover_mode(RolloverMode::Nkro);

        let pos = KeyPos::new(0, 0);
        let mut reports: heapless::Vec<HidReport, 4> = heapless::Vec::new();
        proc.process_event(KeyEvent::press(pos, 0), 0, &mut |r| {
            reports.push(r).ok();
        });

        assert!(matches!(reports.first(), Some(HidReport::NkroKeyboard(_))));
    }

    #[test]
    fn oneshot_mod_is_cleared_after_next_key() {
        let mut proc = make_processor(&[
            Action::OneShotMod(ModifierKey::LSHIFT),
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        proc.process_event(press(pos(0, 1), 10), 10, &mut |_| {});
        proc.process_event(release(pos(0, 1), 20), 20, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 2), 30), 30, emit);
        });
        assert!(reports
            .iter()
            .all(|r| keyboard_mods(*r) & ModifierKey::LSHIFT.bits() == 0));
    }

    // ── OneShotLayer press and release ────────────────────────────────────────

    #[test]
    fn oneshot_layer_press_activates_layer() {
        let mut proc = make_processor_two_layers(
            &[
                Action::OneShotLayer(1),
                Action::Key(KeyCode::A),
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
            ],
            &[
                Action::Transparent,
                Action::Key(KeyCode::B),
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
            ],
        );
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 1), 1, emit);
        });
        assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::B.usage_id()));
    }

    #[test]
    fn oneshot_layer_release_deactivates_layer() {
        let mut proc = make_processor_two_layers(
            &[
                Action::OneShotLayer(1),
                Action::Key(KeyCode::A),
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
            ],
            &[
                Action::Transparent,
                Action::Key(KeyCode::B),
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
                Action::NoOp,
            ],
        );
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        proc.process_event(release(pos(0, 0), 5), 5, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.process_event(press(pos(0, 1), 10), 10, emit);
        });
        assert!(keyboard_keycodes(*reports.first().unwrap()).contains(&KeyCode::A.usage_id()));
    }

    // ── process_one: tap-hold active, event is Pending (line 177) ────────────

    #[test]
    fn second_key_during_tap_hold_is_buffered() {
        let mut proc = make_processor(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::Key(KeyCode::B),
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let mut count = 0usize;
        proc.process_event(press(pos(0, 1), 50), 50, &mut |_| {
            count += 1;
        });
        assert_eq!(count, 0);
    }

    // ── tick() with no active tap-hold ────────────────────────────────────────

    #[test]
    fn tick_with_no_tap_hold_does_nothing() {
        let mut proc = make_processor(&[Action::Key(KeyCode::A); 6]);
        let mut count = 0usize;
        proc.tick(100, &mut |_| {
            count += 1;
        });
        assert_eq!(count, 0);
    }

    // ── tick() pending early-return ───────────────────────────────────────────

    #[test]
    fn tick_before_deadline_returns_without_emitting() {
        let mut proc = make_processor(&[
            Action::ModTap {
                hold: ModifierKey::LSHIFT,
                tap: KeyCode::A,
            },
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
            Action::NoOp,
        ]);
        proc.process_event(press(pos(0, 0), 0), 0, &mut |_| {});
        let reports = collect_reports(|emit| {
            proc.tick(50, emit);
        });
        assert!(reports.is_empty());
    }

    // ── keyboard_keycodes / keyboard_mods Consumer and NkroKeyboard arms ────────

    #[test]
    fn keyboard_keycodes_returns_zeros_for_consumer_report() {
        let report = HidReport::Consumer(ConsumerReport { keycode: None });
        assert_eq!(keyboard_keycodes(report), [0; 6]);
    }

    #[test]
    fn keyboard_mods_returns_zero_for_consumer_report() {
        let report = HidReport::Consumer(ConsumerReport { keycode: None });
        assert_eq!(keyboard_mods(report), 0);
    }

    #[test]
    fn keyboard_keycodes_returns_zeros_for_nkro_report() {
        use crate::report::NkroKeyboardReport;
        let report = HidReport::NkroKeyboard(NkroKeyboardReport::default());
        assert_eq!(keyboard_keycodes(report), [0; 6]);
    }

    #[test]
    fn keyboard_mods_returns_zero_for_nkro_report() {
        use crate::report::NkroKeyboardReport;
        let report = HidReport::NkroKeyboard(NkroKeyboardReport::default());
        assert_eq!(keyboard_mods(report), 0);
    }
}
