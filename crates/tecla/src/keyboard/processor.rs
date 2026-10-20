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

use crate::accessibility::{AccessibilityConfig, AccessibilityState, SlowKeyResult};
use crate::action::{
    Action, DksKeys, GamepadAction, KeyCode, KeyOverride, LayerOp, MidiAction, ModifierKey,
    MouseAction, TriLayerDef,
};
#[cfg(feature = "autocomplete")]
use crate::autocomplete::AutocompleteEngine;
#[cfg(feature = "autocorrect")]
use crate::autocorrect::AutocorrectEngine;
use crate::devices::gamepad::GamepadState;
use crate::devices::keyboard::combo::{Combo, ComboConfig};
use crate::devices::keyboard::tapping::{TapHoldConfig, TapHoldDecision, TapHoldRecognizer};
use crate::matrix::TriggerPhase;
use crate::pipeline::event::{KeyEvent, KeyPos};
use crate::pipeline::keymap::LayerStack;
use crate::pipeline::modifier::ModifierState;
use crate::pipeline::recognizer::{Recognition, Recognizer};
use crate::pipeline::report::{
    ConsumerReport, HidReport, KeyboardReport, MidiEvent, MouseReport, NkroKeyboardReport,
};
use crate::telemetry::accessibility::AccessibilityTelemetry;
use crate::telemetry::gamepad::GamepadTelemetry;
use crate::telemetry::midi::MidiTelemetry;
use crate::telemetry::pointing::PointingTelemetry;
use crate::telemetry::steno::StenoTelemetry;
use crate::telemetry::CoreTelemetry;

use super::steno_playback;

#[cfg(test)]
mod tests;

/// Bundled telemetry collectors for all modalities handled by the processor.
pub struct ProcessorTelemetry<const KEYS: usize> {
    /// Core keystroke dynamics (always active when telemetry is enabled).
    pub core: CoreTelemetry<KEYS>,
    /// Steno chord accuracy and dictionary metrics.
    pub steno: StenoTelemetry,
    /// Accessibility filter rejection rates and fatigue tracking.
    pub accessibility: AccessibilityTelemetry,
    /// MIDI velocity distributions and polyphony.
    pub midi: MidiTelemetry,
    /// Gamepad axis range and button combinations.
    pub gamepad: GamepadTelemetry,
    /// Pointing device velocity and click patterns.
    pub pointing: PointingTelemetry,
}

impl<const KEYS: usize> ProcessorTelemetry<KEYS> {
    /// Bundles every collector into one transportable frame.
    ///
    /// Non-destructive: this snapshots without clearing deltas, so a host
    /// reading the frame across several transport packets sees one consistent
    /// picture. Clearing is a separate, explicit step — see
    /// [`clear_deltas`](Self::clear_deltas).
    ///
    /// `encoder` is left `None`: encoder telemetry is owned by the encoder task,
    /// not the processor.
    #[must_use]
    pub fn frame(&self) -> crate::telemetry::transport::TelemetryFrame {
        crate::telemetry::transport::TelemetryFrame {
            core: Some(self.core.snapshot()),
            steno: Some(self.steno.snapshot()),
            midi: Some(self.midi.snapshot()),
            gamepad: Some(self.gamepad.snapshot()),
            pointing: Some(self.pointing.snapshot()),
            encoder: None,
            accessibility: Some(self.accessibility.snapshot()),
        }
    }

    /// Resets every collector's delta counters, keeping cumulative totals.
    ///
    /// Call once the host has acknowledged a frame, so the next one measures a
    /// fresh interval.
    pub fn clear_deltas(&mut self) {
        use crate::telemetry::TelemetryExt;
        self.core.clear_deltas();
        self.steno.clear_deltas();
        self.midi.clear_deltas();
        self.gamepad.clear_deltas();
        self.pointing.clear_deltas();
        self.accessibility.clear_deltas();
    }

    pub(crate) fn new() -> Self {
        Self {
            core: CoreTelemetry::new(),
            steno: StenoTelemetry::default(),
            accessibility: AccessibilityTelemetry::default(),
            midi: MidiTelemetry::default(),
            gamepad: GamepadTelemetry::default(),
            pointing: PointingTelemetry::default(),
        }
    }
}

/// Controls whether the keyboard emits 6KRO or NKRO HID reports.
///
/// The default is [`Kro6`](RolloverMode::Kro6) for maximum OS compatibility.
/// Switch to [`Nkro`](RolloverMode::Nkro) if you need more than six simultaneous
/// keys — gaming keyboards, chord-heavy layouts, or steno practice.
///
/// Note that some BIOS / boot environments only recognise the boot-protocol
/// 6KRO report. A production board crate may want to expose a key combo that
/// toggles this at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RolloverMode {
    /// Six-key rollover: standard HID boot-protocol report.
    ///
    /// Compatible with every OS and BIOS. Supports up to 6 non-modifier keys
    /// simultaneously plus all 8 modifier bits.
    #[default]
    Kro6,
    /// N-key rollover: 16-byte bitmap report, up to 128 simultaneous keys.
    ///
    /// Requires a host driver that understands the NKRO HID report descriptor.
    /// Modern Linux, macOS, and Windows all support it; older BIOSes may not.
    Nkro,
}

// ── ActiveHold ────────────────────────────────────────────────────────────────

/// The held outcome of a tap-hold key — only the hold variants.
///
/// `TapHoldDecision::Tap` is never stored here because tap decisions are fully
/// handled (press + release emitted) at resolution time and require no cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldDecision {
    Mod(ModifierKey),
    Layer(u8),
}

/// A resolved tap-hold that is still physically held down.
///
/// Stored so that when the physical key is released the correct cleanup action
/// (modifier release or layer deactivation) can be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveHold {
    pos: KeyPos,
    decision: HoldDecision,
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
    Ext: Copy + PartialEq + 'static,
{
    layer_stack: LayerStack<LAYERS, KEYS, Ext>,
    modifier_state: ModifierState,
    tap_hold_config: TapHoldConfig,
    active_tap_hold: Option<TapHoldRecognizer>,
    active_holds: Vec<ActiveHold, 8>,
    combo_config: ComboConfig<Ext>,
    // Chords fire as soon as the held set exactly matches a combo: buffer the
    // presses (to replay on no-match), accumulate the union of pressed combo
    // keys, and track which are still held so we know when the chord resolves
    // (matched, or fully released with no match).
    // ponytail: cap 8 keys per chord; bump if a board defines wider chords.
    combo_pending: Deque<KeyEvent, 8>,
    combo_pressed: Vec<KeyCode, 8>,
    combo_held: Vec<KeyPos, 8>,
    // Set when the held chord matches a combo: the action is pressed (held,
    // repeating exactly like any other key) until the first member releases.
    combo_active: Option<(Action<Ext>, KeyPos)>,
    // Tracks which keycode is currently "held" for each physically-active DKS
    // key, so process_dks_phase and handle_action_release know what to
    // release before pressing the next phase's keycode. Bounded to 8
    // simultaneously-active DKS keys (matches other small rollover caps in
    // this struct); a 9th DKS key held at the same time is not pressed at
    // all (see handle_action_press's Dks arm) rather than being pressed
    // untracked, which would make it unreleasable.
    dks_active: Vec<(KeyPos, DksKeys, KeyCode), 8>,
    pressed_keys: Vec<u8, 16>,
    pending: Deque<KeyEvent, 32>,
    rollover_mode: RolloverMode,
    caps_word_active: bool,
    locked_layers: u32,
    last_keycode: Option<KeyCode>,
    repeat_active: bool,
    tri_layers: &'static [TriLayerDef],
    overrides: &'static [KeyOverride<Ext>],
    macro_slots: [Deque<KeyCode, 64>; 4],
    macro_recording: Option<u8>,
    playback_queue: Deque<KeyCode, 64>,
    steno_playback: steno_playback::StenoPlayback,
    mouse_buttons: u8,
    gamepad: GamepadState,
    bootmagic_triggered: bool,
    last_any_press_ms: u64,
    last_tap_hold_tap: Option<(KeyPos, u64)>,
    pending_ble: Option<crate::transport::ble::BleCommand>,
    pending_rgb: Option<crate::rgb::RgbCommand>,
    pending_audio: Option<crate::audio::AudioCommand>,
    pending_haptic: Option<crate::haptic::HapticCommand>,
    pending_output: Option<crate::pipeline::hid::OutputMode>,
    pending_sleep: bool,
    #[cfg(feature = "autocorrect")]
    autocorrect: Option<AutocorrectEngine>,
    /// When set, typed keycodes are queued in `typed_out` for a remote engine
    /// (e.g. on a second core) instead of running the engine inline.
    #[cfg(feature = "autocorrect")]
    autocorrect_offload: bool,
    /// Keycodes awaiting forwarding to a remote autocorrect engine.
    #[cfg(feature = "autocorrect")]
    typed_out: Deque<KeyCode, 32>,
    #[cfg(feature = "autocomplete")]
    autocomplete: Option<AutocompleteEngine>,
    steno_engine: Option<crate::devices::steno::StenoEngine>,
    steno_chord: u64,
    steno_held_count: u32,
    accessibility: Option<AccessibilityState<KEYS>>,
    telemetry: Option<ProcessorTelemetry<KEYS>>,
}

impl<const ROWS: usize, const COLS: usize, const LAYERS: usize, const KEYS: usize, Ext>
    KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>
where
    Ext: Copy + PartialEq + 'static,
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
            combo_config: ComboConfig::empty(),
            combo_pending: Deque::new(),
            combo_pressed: Vec::new(),
            combo_held: Vec::new(),
            combo_active: None,
            dks_active: Vec::new(),
            pressed_keys: Vec::new(),
            pending: Deque::new(),
            rollover_mode: RolloverMode::Kro6,
            caps_word_active: false,
            locked_layers: 0,
            last_keycode: None,
            repeat_active: false,
            tri_layers: &[],
            overrides: &[],
            macro_slots: [Deque::new(), Deque::new(), Deque::new(), Deque::new()],
            macro_recording: None,
            playback_queue: Deque::new(),
            steno_playback: steno_playback::StenoPlayback::new(),
            mouse_buttons: 0,
            gamepad: GamepadState::new(),
            bootmagic_triggered: false,
            last_any_press_ms: 0,
            last_tap_hold_tap: None,
            pending_ble: None,
            pending_rgb: None,
            pending_audio: None,
            pending_haptic: None,
            pending_output: None,
            pending_sleep: false,
            #[cfg(feature = "autocorrect")]
            autocorrect: None,
            #[cfg(feature = "autocorrect")]
            autocorrect_offload: false,
            #[cfg(feature = "autocorrect")]
            typed_out: Deque::new(),
            #[cfg(feature = "autocomplete")]
            autocomplete: None,
            steno_engine: None,
            steno_chord: 0,
            steno_held_count: 0,
            accessibility: None,
            telemetry: None,
        }
    }

    /// Sets the rollover mode, consuming and returning `self` for builder chaining.
    #[must_use]
    pub fn with_rollover_mode(mut self, mode: RolloverMode) -> Self {
        self.rollover_mode = mode;
        self
    }

    /// Sets the tri-layer definitions for automatic layer activation.
    #[must_use]
    pub fn with_tri_layers(mut self, defs: &'static [TriLayerDef]) -> Self {
        self.tri_layers = defs;
        self
    }

    /// Sets the key override table for modifier-aware key remapping.
    #[must_use]
    pub fn with_overrides(mut self, overrides: &'static [KeyOverride<Ext>]) -> Self {
        self.overrides = overrides;
        self
    }

    /// Sets combo definitions and timing.
    ///
    /// Combo keys are buffered until either a combo resolves or the configured
    /// combo term expires. On timeout/failure, buffered events are replayed
    /// through the normal processor path.
    #[must_use]
    pub fn with_combos(mut self, config: ComboConfig<Ext>) -> Self {
        self.combo_config = config;
        self
    }

    /// Attaches an autocorrect engine, consuming and returning `self`.
    #[cfg(feature = "autocorrect")]
    #[must_use]
    pub fn with_autocorrect(mut self, engine: AutocorrectEngine) -> Self {
        self.autocorrect = Some(engine);
        self
    }

    /// Routes typed keycodes to a *remote* autocorrect engine instead of running
    /// one inline. Pair with [`autocorrect_task`](crate::task::autocorrect_task),
    /// [`pop_typed_keycode`](Self::pop_typed_keycode), and
    /// [`inject_correction`](Self::inject_correction). The remote engine may run
    /// on a second core. Do not combine with [`with_autocorrect`](Self::with_autocorrect).
    #[cfg(feature = "autocorrect")]
    #[must_use]
    pub fn with_autocorrect_offload(mut self) -> Self {
        self.autocorrect_offload = true;
        self
    }

    /// Pops the next keycode queued for a remote autocorrect engine.
    ///
    /// Returns `None` unless [`with_autocorrect_offload`](Self::with_autocorrect_offload)
    /// is set. The async task layer drains this into the engine's input channel.
    #[cfg(feature = "autocorrect")]
    pub fn pop_typed_keycode(&mut self) -> Option<KeyCode> {
        self.typed_out.pop_front()
    }

    /// Applies a correction produced by a remote autocorrect engine: emits
    /// `backspaces` backspaces followed by the `replacement` keycodes through
    /// the normal playback path.
    #[cfg(feature = "autocorrect")]
    pub fn inject_correction<F>(&mut self, backspaces: usize, replacement: &[KeyCode], emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        for _ in 0..backspaces {
            let _ = self.playback_queue.push_back(KeyCode::Backspace);
        }
        for &kc in replacement {
            let _ = self.playback_queue.push_back(kc);
        }
        self.drain_playback(emit);
    }

    /// Attaches an autocomplete engine, consuming and returning `self`.
    #[cfg(feature = "autocomplete")]
    #[must_use]
    pub fn with_autocomplete(mut self, engine: AutocompleteEngine) -> Self {
        self.autocomplete = Some(engine);
        self
    }

    /// Attaches a steno engine, consuming and returning `self`.
    #[must_use]
    pub fn with_steno(mut self, engine: crate::devices::steno::StenoEngine) -> Self {
        self.steno_engine = Some(engine);
        self
    }

    /// Attaches accessibility features (slow keys, bounce keys, sticky keys).
    #[must_use]
    pub fn with_accessibility(mut self, config: AccessibilityConfig) -> Self {
        self.accessibility = Some(AccessibilityState::new(config));
        self
    }

    /// Enables telemetry collection, consuming and returning `self`.
    #[must_use]
    pub fn with_telemetry(mut self) -> Self {
        self.telemetry = Some(ProcessorTelemetry::new());
        self
    }

    /// Returns a reference to the telemetry state, if enabled.
    #[must_use]
    pub fn telemetry(&self) -> Option<&ProcessorTelemetry<KEYS>> {
        self.telemetry.as_ref()
    }

    /// Returns a mutable reference to the telemetry state, if enabled.
    pub fn telemetry_mut(&mut self) -> Option<&mut ProcessorTelemetry<KEYS>> {
        self.telemetry.as_mut()
    }

    /// Returns `true` if a [`Action::Bootmagic`] key was pressed.
    ///
    /// Board crates check this at startup to decide whether to jump to the
    /// bootloader or wipe configuration.
    #[must_use]
    pub fn bootmagic_triggered(&self) -> bool {
        self.bootmagic_triggered
    }

    /// Returns and clears the pending BLE command, if any.
    #[must_use]
    pub fn pending_ble_command(&mut self) -> Option<crate::transport::ble::BleCommand> {
        self.pending_ble.take()
    }

    /// Returns and clears the pending RGB command, if any.
    #[must_use]
    pub fn pending_rgb_command(&mut self) -> Option<crate::rgb::RgbCommand> {
        self.pending_rgb.take()
    }

    /// Returns and clears the pending audio command, if any.
    ///
    /// Board crates drain this each loop and forward it to their
    /// [`AudioController`](crate::audio::AudioController).
    #[must_use]
    pub fn pending_audio_command(&mut self) -> Option<crate::audio::AudioCommand> {
        self.pending_audio.take()
    }

    /// Returns and clears the pending haptic command, if any.
    ///
    /// Board crates drain this each loop and forward it to their
    /// [`HapticController`](crate::haptic::HapticController).
    #[must_use]
    pub fn pending_haptic_command(&mut self) -> Option<crate::haptic::HapticCommand> {
        self.pending_haptic.take()
    }

    /// Returns and clears the pending output-mode switch, if any.
    #[must_use]
    pub fn pending_output_mode(&mut self) -> Option<crate::pipeline::hid::OutputMode> {
        self.pending_output.take()
    }

    /// Returns and clears the pending sleep request.
    ///
    /// Board crates that support a low-power state check this each loop; on
    /// boards that do not, [`Action::Sleep`](crate::action::Action::Sleep) has
    /// no effect beyond setting this flag.
    #[must_use]
    pub fn pending_sleep(&mut self) -> bool {
        core::mem::take(&mut self.pending_sleep)
    }

    /// Returns the default layer bitmask.
    #[must_use]
    pub fn default_state(&self) -> u32 {
        self.layer_stack.default_state()
    }

    /// Returns the locked-layers bitmask.
    #[must_use]
    pub fn locked_layers(&self) -> u32 {
        self.locked_layers
    }

    /// Returns the current rollover mode.
    #[must_use]
    pub fn rollover_mode(&self) -> RolloverMode {
        self.rollover_mode
    }

    /// Returns a reference to a dynamic macro slot, or `None` if out of range.
    #[must_use]
    pub fn macro_slot(&self, idx: u8) -> Option<&Deque<KeyCode, 64>> {
        self.macro_slots.get(usize::from(idx))
    }

    /// Restores previously persisted state at boot.
    ///
    /// Call this once after construction, before processing any events.
    pub fn restore_state(
        &mut self,
        default_state: u32,
        locked_layers: u32,
        rollover_mode: RolloverMode,
        macros: [Deque<KeyCode, 64>; 4],
    ) {
        self.layer_stack.apply(LayerOp::SetDefault(0));
        for bit in 0..32u8 {
            if default_state & (1 << u32::from(bit)) != 0 {
                self.layer_stack.apply(LayerOp::SetDefault(bit));
                break;
            }
        }
        self.locked_layers = locked_layers;
        for bit in 0..32u8 {
            if locked_layers & (1 << u32::from(bit)) != 0 {
                self.layer_stack.apply(LayerOp::On(bit));
            }
        }
        self.rollover_mode = rollover_mode;
        self.macro_slots = macros;
    }

    /// Processes a key event, calling `emit` for each HID report produced.
    ///
    /// Events are queued internally and replayed after tap-hold resolution,
    /// so `emit` may be called zero or more times per call.
    pub fn process_event<F: FnMut(HidReport) + ?Sized>(
        &mut self,
        event: KeyEvent,
        now_ms: u64,
        emit: &mut F,
    ) {
        if let Some(ref mut acc) = self.accessibility {
            if event.pressed {
                match acc.should_accept_press(event.pos, now_ms, COLS) {
                    SlowKeyResult::Accept => {
                        if let Some(ref mut t) = self.telemetry {
                            t.accessibility.record_slow_accept();
                        }
                    }
                    SlowKeyResult::Defer(_) => return,
                    SlowKeyResult::Reject => {
                        if let Some(ref mut t) = self.telemetry {
                            t.accessibility.record_bounce_reject();
                        }
                        return;
                    }
                }
            } else {
                let was_pending_slow = acc.is_pending_slow(event.pos);
                acc.record_release(event.pos, now_ms, COLS);
                if was_pending_slow {
                    if let Some(ref mut t) = self.telemetry {
                        t.accessibility.record_slow_reject();
                    }
                }
            }
        }
        let _ = self.pending.push_back(event);
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
        if let Some(ref mut acc) = self.accessibility {
            let ready = acc.tick_slow_keys(now_ms);
            for pos in ready {
                // A deferred press that survives to its deadline is an
                // *accepted* slow key. Without this, slow_reject_rate only
                // ever sees rejects and reads 0 when nothing was rejected —
                // indistinguishable from "a perfect 0% reject rate".
                if let Some(ref mut t) = self.telemetry {
                    t.accessibility.record_slow_accept();
                }
                let event = KeyEvent::press(pos, now_ms);
                let _ = self.pending.push_back(event);
            }
        }

        if let Some(rec) = self.active_tap_hold.as_mut() {
            let result = rec.tick(now_ms);
            if result.is_pending() {
                return;
            }
            if let Some(rec) = self.active_tap_hold.take() {
                if let Recognition::Matched(decision) = result {
                    self.handle_tap_hold_result(decision, rec, now_ms, emit);
                }
            }
        }
        self.drain_pending(now_ms, emit);
    }

    /// Advances a Dynamic Keystroke ([`Action::Dks`]) key through its
    /// bottom-out or release-from-bottom phase.
    ///
    /// Call this from an analog board task whenever
    /// [`crate::matrix::RapidTrigger::process`] yields a
    /// [`TriggerPhase::BottomOut`] or [`TriggerPhase::ReleaseFromBottom`]
    /// transition for a key position. Ordinary [`TriggerPhase::Press`] /
    /// [`TriggerPhase::Release`] transitions should instead go through the
    /// normal [`process_event`](Self::process_event) as a regular
    /// [`KeyEvent`].
    ///
    /// A no-op if the key at `pos` isn't currently an active [`Action::Dks`]
    /// key, or if `phase` is [`TriggerPhase::Press`] or
    /// [`TriggerPhase::Release`].
    pub fn process_dks_phase<F>(&mut self, pos: KeyPos, phase: TriggerPhase, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        let Some(idx) = self.dks_active.iter().position(|(p, _, _)| *p == pos) else {
            return;
        };
        let (_, dks, held) = self.dks_active[idx];
        let new_kc = match phase {
            TriggerPhase::BottomOut => dks.bottom_out,
            TriggerPhase::ReleaseFromBottom => dks.release_from_bottom,
            TriggerPhase::Press | TriggerPhase::Release => return,
        };
        if new_kc == held {
            return;
        }
        self.release_keycode(held, emit);
        self.press_keycode(new_kc, emit);
        self.dks_active[idx].2 = new_kc;
    }

    /// Returns the next timer deadline the caller must wake at.
    ///
    /// `None` when no tap-hold is in flight.
    #[must_use]
    pub fn next_deadline_ms(&self) -> Option<u64> {
        let tap_hold = self
            .active_tap_hold
            .as_ref()
            .and_then(|r| r.next_deadline_ms());
        let slow_key = self
            .accessibility
            .as_ref()
            .and_then(|a| a.next_slow_key_deadline());
        // Chords have no timing window — they resolve on the next press or
        // release event, never a timer — so they never contribute a deadline.
        let mut deadline = tap_hold;
        if let Some(slow_key) = slow_key {
            deadline = Some(deadline.map_or(slow_key, |d| d.min(slow_key)));
        }
        deadline
    }

    /// Replaces a single action in the keymap at runtime.
    ///
    /// Returns `true` if both the layer and key indices were valid.
    pub fn set_action(&mut self, layer: usize, key: usize, action: Action<Ext>) -> bool {
        self.layer_stack.set_action(layer, key, action)
    }

    /// Returns the action at `(layer, key)`, or `None` if out of bounds.
    #[must_use]
    pub fn get_action(&self, layer: usize, key: usize) -> Option<Action<Ext>> {
        self.layer_stack.get_action(layer, key)
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
        if self.handle_combo_event(event, now_ms, emit) {
            return;
        }
        self.process_one_without_combo(event, now_ms, emit);
    }

    fn process_one_without_combo<F>(&mut self, event: KeyEvent, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if let Some(rec) = self.active_tap_hold.as_mut() {
            let result = rec.observe(event, now_ms);
            if result.is_pending() {
                return;
            }
            if let Some(rec) = self.active_tap_hold.take() {
                if let Recognition::Matched(decision) = result {
                    self.handle_tap_hold_result(decision, rec, now_ms, emit);
                }
            }
            return;
        }

        if event.pressed {
            self.last_any_press_ms = event.timestamp_ms;
            if let Some(ref mut t) = self.telemetry {
                t.core.record_press(event.pos, now_ms, COLS);
            }
            self.handle_press(event, now_ms, emit);
        } else {
            if let Some(ref mut t) = self.telemetry {
                let hold_ms = t.core.last_hold_time(event.pos, now_ms, COLS);
                t.core.record_release(event.pos, now_ms, COLS);
                if let Some(ms) = hold_ms {
                    t.accessibility.record_hold_time(ms);
                }
            }
            self.handle_release(event, emit);
        }
    }

    /// Press-to-fire, hold-to-repeat chord recognition.
    ///
    /// While combo keys are held we buffer their presses. As soon as the
    /// accumulated set exactly matches a combo, its action is pressed — held
    /// and repeating exactly like any other key — and released on the first
    /// member key-up, same as letting go of a normal held key. If the held
    /// set is fully released without ever matching, the buffered presses are
    /// replayed as normal taps. A non-combo key pressed before any match
    /// aborts the in-progress chord, replaying its keys as normal presses; one
    /// pressed after a match is independent rollover, not an abort.
    ///
    /// Returns `true` when the event was consumed by chord handling.
    ///
    /// ponytail: no timing window, so a fast roll across two chord keys fires
    /// the chord; put chords on keys you don't roll, or on a dedicated layer.
    fn handle_combo_event<F>(&mut self, event: KeyEvent, now_ms: u64, emit: &mut F) -> bool
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if self.combo_config.combos().is_empty() {
            return false;
        }

        if event.pressed {
            if let Some(kc) = self.combo_key_for(event.pos) {
                let _ = self.combo_pending.push_back(event);
                if !self.combo_pressed.contains(&kc) {
                    let _ = self.combo_pressed.push(kc);
                }
                if !self.combo_held.contains(&event.pos) {
                    let _ = self.combo_held.push(event.pos);
                }
                if self.combo_active.is_none() {
                    if let Some(combo) = self.combo_match() {
                        self.combo_pending.clear();
                        self.combo_pressed.clear();
                        match combo.action {
                            // Tap-hold and toggle actions aren't holdable —
                            // they're their own tap/hold decision or a single
                            // instantaneous flip, so fire them instantly, same
                            // as a directly-mapped key would.
                            Action::ModTap { tap: kc, .. } | Action::LayerTap { tap: kc, .. } => {
                                self.press_keycode(kc, emit);
                                self.release_keycode(kc, emit);
                            }
                            Action::TapToggle(layer) => {
                                self.layer_stack.apply(LayerOp::Toggle(layer));
                                self.check_tri_layers();
                            }
                            _ => {
                                // `event.pos` is the key that completed the
                                // match — a real position to anchor the held
                                // action, released on the first key-up below.
                                self.handle_action_press(
                                    combo.action,
                                    KeyEvent::press(event.pos, now_ms),
                                    now_ms,
                                    emit,
                                );
                                self.combo_active = Some((combo.action, event.pos));
                            }
                        }
                    }
                }
                return true;
            }
            // A non-combo key joined before any match: abandon the chord,
            // replaying held keys. After a match, this is just rollover.
            if self.combo_active.is_none() && !self.combo_pressed.is_empty() {
                self.abort_combo(now_ms, emit);
            }
            return false;
        }

        // Release: only combo keys that are part of the building/held chord matter.
        let Some(idx) = self.combo_held.iter().position(|&p| p == event.pos) else {
            return false;
        };
        self.combo_held.swap_remove(idx);

        if let Some((action, pos)) = self.combo_active.take() {
            // Already matched and held: release on the first key-up, exactly
            // like releasing any other held key.
            self.handle_action_release(action, KeyEvent::release(pos, now_ms), emit);
            return true;
        }

        if self.combo_pressed.is_empty() {
            // Already resolved (aborted or replayed above) — the remaining
            // physical releases are just drained silently.
            return true;
        }

        if self.combo_held.is_empty() {
            self.replay_combo_as_taps(now_ms, emit);
        }
        true
    }

    /// Aborts an in-progress chord because a non-combo key was pressed. The
    /// chord keys are still physically held, so replay their presses as normal
    /// presses and let their later releases flow through unchanged.
    fn abort_combo<F>(&mut self, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        self.combo_pressed.clear();
        self.combo_held.clear();
        while let Some(event) = self.combo_pending.pop_front() {
            self.process_one_without_combo(event, now_ms, emit);
        }
    }

    /// No combo matched the released chord: the keys were pressed and released,
    /// so replay each as a discrete tap in press order.
    /// ponytail: discrete taps lose the original rollover overlap.
    fn replay_combo_as_taps<F>(&mut self, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        self.combo_pressed.clear();
        self.combo_held.clear();
        while let Some(event) = self.combo_pending.pop_front() {
            self.process_one_without_combo(event, now_ms, emit);
            let release = KeyEvent::release(event.pos, now_ms);
            self.process_one_without_combo(release, now_ms, emit);
        }
    }

    /// The base-layer keycode a position produces, for combo matching.
    ///
    /// Combos are defined by keycode, so a press is mapped to the key it types
    /// on the base layer (layer 0). Mod-tap / layer-tap keys resolve to their
    /// tap keycode.
    /// ponytail: base layer only — a combo whose keys live on another layer
    /// won't match; resolve against the active layer if that's ever needed.
    fn combo_keycode(&self, pos: KeyPos) -> Option<KeyCode> {
        match self.layer_stack.get_action(0, pos.flat(COLS)) {
            Some(Action::Key(kc))
            | Some(Action::ModTap { tap: kc, .. })
            | Some(Action::LayerTap { tap: kc, .. }) => Some(kc),
            _ => None,
        }
    }

    /// Returns the keycode at `pos` if it participates in some combo.
    fn combo_key_for(&self, pos: KeyPos) -> Option<KeyCode> {
        let kc = self.combo_keycode(pos)?;
        self.combo_config
            .combos()
            .iter()
            .any(|combo| combo.contains(kc))
            .then_some(kc)
    }

    /// Finds a combo whose keycode set exactly equals the accumulated chord.
    fn combo_match(&self) -> Option<Combo<Ext>> {
        self.combo_config.combos().iter().copied().find(|combo| {
            combo.keys.len() == self.combo_pressed.len()
                && combo
                    .keys
                    .iter()
                    .copied()
                    .all(|kc| self.combo_pressed.contains(&kc))
        })
    }

    fn handle_press<F>(&mut self, event: KeyEvent, now_ms: u64, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        // A press for a position that is already down (has a live press cache
        // entry) is debounce noise a well-formed scanner never produces.
        // Re-resolving it would overwrite the cached press-time action — if a
        // layer changed in between, the originally pressed keycode could then
        // never be released. Ignore it.
        if self.layer_stack.has_cached_press(event.pos, COLS) {
            return;
        }
        self.layer_stack.cache_press(event.pos, COLS);
        let resolved = self.layer_stack.resolve(event.pos, COLS);

        let action = match resolved {
            Action::Key(kc) => self.apply_override(kc).unwrap_or(resolved),
            other => other,
        };

        self.handle_action_press(action, event, now_ms, emit);
    }

    #[expect(
        clippy::too_many_lines,
        reason = "central dispatch over all Action variants"
    )]
    fn handle_action_press<F>(
        &mut self,
        action: Action<Ext>,
        event: KeyEvent,
        now_ms: u64,
        emit: &mut F,
    ) where
        F: FnMut(HidReport) + ?Sized,
    {
        match action {
            Action::Key(kc) => {
                self.press_keycode(kc, emit);
            }
            Action::Dks(dks) => {
                // Track before pressing: if `dks_active` is already full (8
                // concurrent DKS keys), don't press a keycode we have no
                // slot to release later — that would leave it stuck asserted
                // forever, since `handle_action_release`'s Dks arm only
                // releases keys it finds in `dks_active`.
                if self.dks_active.push((event.pos, dks, dks.press)).is_ok() {
                    self.press_keycode(dks.press, emit);
                }
            }
            Action::Modifier(m) => {
                let sticky = self
                    .accessibility
                    .as_ref()
                    .is_some_and(|a| a.config.sticky_keys);
                if sticky {
                    self.modifier_state.register_oneshot(m);
                    if let Some(ref mut t) = self.telemetry {
                        t.accessibility.record_sticky_use();
                    }
                } else {
                    self.modifier_state.press(m);
                }
                self.emit_keyboard(emit);
            }
            Action::Consumer(cc) => {
                emit(HidReport::Consumer(ConsumerReport { keycode: Some(cc) }));
            }
            Action::Layer(op) => {
                self.layer_stack.apply(op);
                self.check_tri_layers();
            }
            Action::LayerLock(n) => {
                let bit = 1 << u32::from(n);
                self.locked_layers ^= bit;
                if self.locked_layers & bit != 0 {
                    self.layer_stack.apply(LayerOp::On(n));
                } else {
                    self.layer_stack.apply(LayerOp::Off(n));
                }
                self.check_tri_layers();
            }
            Action::ModTap { .. } | Action::LayerTap { .. } | Action::TapToggle(_) => {
                if let Some(mut rec) =
                    TapHoldRecognizer::from_action(self.tap_hold_config, action, event)
                {
                    let cfg = &self.tap_hold_config;
                    let quick_tap_hit = cfg.quick_tap_ms > 0
                        && self.last_tap_hold_tap.is_some_and(|(pos, ts)| {
                            pos == event.pos
                                && event.timestamp_ms.wrapping_sub(ts) < cfg.quick_tap_ms
                        });
                    let idle_too_short = cfg.require_prior_idle_ms > 0
                        && self.last_any_press_ms > 0
                        && event.timestamp_ms.wrapping_sub(self.last_any_press_ms)
                            < cfg.require_prior_idle_ms;
                    if quick_tap_hit || idle_too_short {
                        rec.set_force_tap(true);
                    }
                    self.active_tap_hold = Some(rec);
                }
            }
            Action::OneShotMod(m) => {
                self.modifier_state.register_oneshot(m);
            }
            Action::OneShotLayer(n) => {
                self.layer_stack.apply(LayerOp::On(n));
                self.check_tri_layers();
            }
            Action::CapsWord => {
                self.caps_word_active = true;
            }
            Action::RepeatKey => {
                if let Some(kc) = self.last_keycode {
                    self.repeat_active = true;
                    self.press_keycode(kc, emit);
                    self.repeat_active = false;
                }
            }
            Action::MacroRecord(slot) => {
                if let Some(buf) = self.macro_slots.get_mut(usize::from(slot)) {
                    buf.clear();
                    self.macro_recording = Some(slot);
                }
            }
            Action::MacroStop => {
                self.macro_recording = None;
            }
            Action::MacroPlay(slot) => {
                if let Some(buf) = self.macro_slots.get(usize::from(slot)) {
                    self.playback_queue.clear();
                    for kc in buf {
                        let _ = self.playback_queue.push_back(*kc);
                    }
                }
                self.drain_playback(emit);
            }
            Action::SendString(codes) => {
                self.playback_queue.clear();
                for kc in codes {
                    let _ = self.playback_queue.push_back(*kc);
                }
                self.drain_playback(emit);
            }
            Action::LayerMod { layer, modifier } => {
                self.layer_stack.apply(LayerOp::On(layer));
                self.check_tri_layers();
                self.modifier_state.press(modifier);
                self.emit_keyboard(emit);
            }
            Action::ModifiedKey { modifier, key } => {
                self.modifier_state.press(modifier);
                self.press_keycode(key, emit);
            }
            Action::Mouse(ma) => {
                if let Some(ref mut t) = self.telemetry {
                    match ma {
                        MouseAction::Move { dx, dy } => t.pointing.record_movement(dx, dy),
                        MouseAction::Click(_) => t.pointing.record_click(false),
                        MouseAction::Scroll(_) => t.pointing.record_scroll(),
                    }
                }
                self.handle_mouse_press(ma, emit);
            }
            Action::Midi(ma) => {
                if let Some(ref mut t) = self.telemetry {
                    match ma {
                        MidiAction::NoteOn { velocity, .. } => t.midi.record_note_on(velocity),
                        MidiAction::NoteOff { .. } => t.midi.record_note_off(),
                        MidiAction::Cc { .. } => t.midi.record_cc(),
                        MidiAction::PolyPressure { .. } | MidiAction::ChannelPressure { .. } => {}
                    }
                }
                Self::emit_midi_press(ma, emit);
            }
            Action::Gamepad(ga) => {
                if let Some(ref mut t) = self.telemetry {
                    if let GamepadAction::ButtonPress(btn) = ga {
                        t.gamepad.record_button_press(btn);
                    }
                    if let GamepadAction::AxisSet { axis, value } = ga {
                        t.gamepad.record_axis(axis, value);
                    }
                }
                self.handle_gamepad_press(ga, emit);
            }
            Action::AutocorrectToggle =>
            {
                #[cfg(feature = "autocorrect")]
                if let Some(ref mut e) = self.autocorrect {
                    e.set_enabled(!e.enabled());
                }
            }
            Action::AutocompleteToggle =>
            {
                #[cfg(feature = "autocomplete")]
                if let Some(ref mut e) = self.autocomplete {
                    e.set_enabled(!e.enabled());
                }
            }
            Action::AutocompleteAccept => {
                #[cfg(feature = "autocomplete")]
                {
                    if let Some(ref mut e) = self.autocomplete {
                        if let Some(codes) = e.accept() {
                            for &uid in codes.as_slice() {
                                if let Some(kc) = KeyCode::from_usage_id(uid) {
                                    let _ = self.playback_queue.push_back(kc);
                                }
                            }
                        }
                    }
                    self.drain_playback(emit);
                }
            }
            Action::BounceKeysToggle => {
                if let Some(ref mut a) = self.accessibility {
                    a.config.bounce_keys_ms = if a.config.bounce_keys_ms.is_some() {
                        None
                    } else {
                        Some(100)
                    };
                }
            }
            Action::SlowKeysToggle => {
                if let Some(ref mut a) = self.accessibility {
                    a.config.slow_keys_ms = if a.config.slow_keys_ms.is_some() {
                        None
                    } else {
                        Some(300)
                    };
                }
            }
            Action::StickyKeysToggle => {
                if let Some(ref mut a) = self.accessibility {
                    a.config.sticky_keys = !a.config.sticky_keys;
                }
            }
            Action::Bootmagic => {
                self.bootmagic_triggered = true;
            }
            Action::BleAdvertise => {
                self.pending_ble = Some(crate::transport::ble::BleCommand::Advertise);
            }
            Action::BleDisconnect => {
                self.pending_ble = Some(crate::transport::ble::BleCommand::Disconnect);
            }
            Action::BleHostSwitch(n) => {
                self.pending_ble = Some(crate::transport::ble::BleCommand::SwitchHost(n));
            }
            Action::Rgb(cmd) => {
                self.pending_rgb = Some(cmd);
            }
            Action::Steno(bit) => {
                self.steno_chord |= 1u64 << bit.min(63);
                self.steno_held_count = self.steno_held_count.saturating_add(1);
            }
            Action::Audio(cmd) => {
                self.pending_audio = Some(cmd);
            }
            Action::Haptic(cmd) => {
                self.pending_haptic = Some(cmd);
            }
            Action::OutputSelect(mode) => {
                self.pending_output = Some(mode);
            }
            Action::Sleep => {
                self.pending_sleep = true;
            }
            Action::Transparent | Action::NoOp | Action::Ext(_) => {}
        }

        if let Some(slot) = self.macro_recording {
            if let Action::Key(kc) = action {
                if let Some(buf) = self.macro_slots.get_mut(usize::from(slot)) {
                    let _ = buf.push_back(kc);
                }
            }
        }

        let _ = now_ms;
    }

    fn handle_release<F>(&mut self, event: KeyEvent, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        let action = self.layer_stack.release_action(event.pos, COLS);
        self.layer_stack.clear_cache(event.pos, COLS);

        self.handle_action_release(action, event, emit);
    }

    fn handle_action_release<F>(&mut self, action: Action<Ext>, event: KeyEvent, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        match action {
            Action::Key(kc) => {
                self.release_keycode(kc, emit);
            }
            Action::Dks(_) => {
                if let Some(idx) = self.dks_active.iter().position(|(p, _, _)| *p == event.pos) {
                    let (_, _, held) = self.dks_active[idx];
                    self.release_keycode(held, emit);
                    self.dks_active.swap_remove(idx);
                }
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
                self.check_tri_layers();
            }
            Action::OneShotLayer(n) => {
                self.layer_stack.apply(LayerOp::Off(n));
                self.check_tri_layers();
            }
            Action::RepeatKey => {
                if let Some(kc) = self.last_keycode {
                    self.release_keycode(kc, emit);
                }
            }
            Action::LayerMod { layer, modifier } => {
                self.modifier_state.release(modifier);
                self.emit_keyboard(emit);
                self.layer_stack.apply(LayerOp::Off(layer));
                self.check_tri_layers();
            }
            Action::ModifiedKey { modifier, key } => {
                self.release_keycode(key, emit);
                self.modifier_state.release(modifier);
                self.emit_keyboard(emit);
            }
            Action::Mouse(ma) => {
                self.handle_mouse_release(ma, emit);
            }
            Action::Midi(ma) => {
                if let Some(ref mut t) = self.telemetry {
                    if matches!(ma, MidiAction::NoteOn { .. }) {
                        t.midi.record_note_off();
                    }
                }
                Self::emit_midi_release(ma, emit);
            }
            Action::Gamepad(ga) => {
                if let Some(ref mut t) = self.telemetry {
                    if let GamepadAction::ButtonPress(btn) = ga {
                        t.gamepad.record_button_release(btn);
                    }
                }
                self.handle_gamepad_release(ga, emit);
            }
            Action::Steno(_) => {
                self.steno_held_count = self.steno_held_count.saturating_sub(1);
                if self.steno_held_count == 0 && self.steno_chord != 0 {
                    let chord = crate::devices::steno::StenoChord {
                        keys: self.steno_chord,
                    };
                    let was_pending = self
                        .steno_engine
                        .as_ref()
                        .is_some_and(|e| e.pending_stroke_count() > 0);
                    let hit = self
                        .steno_engine
                        .as_mut()
                        .and_then(|engine| engine.process_stroke(chord));
                    if let Some(translation) = hit {
                        if let Some(ref mut t) = self.telemetry {
                            t.steno.record_hit();
                            t.steno.record_stroke_time(event.timestamp_ms);
                            if was_pending {
                                t.steno.record_multi_stroke_complete();
                            }
                        }
                        let mut owned: Vec<u8, 512> = Vec::new();
                        if owned.extend_from_slice(translation).is_ok() {
                            self.playback_steno_translation(owned.as_slice(), emit);
                        }
                    } else if self.steno_engine.is_some() {
                        let now_pending = self
                            .steno_engine
                            .as_ref()
                            .is_some_and(|e| e.pending_stroke_count() > 0);
                        if let Some(ref mut t) = self.telemetry {
                            if now_pending && !was_pending {
                                t.steno.record_multi_stroke_start();
                            } else if !now_pending {
                                t.steno.record_miss();
                            }
                            t.steno.record_stroke_time(event.timestamp_ms);
                        }
                    }
                    self.steno_chord = 0;
                }
            }
            Action::Audio(_)
            | Action::AutocompleteAccept
            | Action::AutocompleteToggle
            | Action::AutocorrectToggle
            | Action::BleAdvertise
            | Action::BounceKeysToggle
            | Action::BleDisconnect
            | Action::BleHostSwitch(_)
            | Action::Bootmagic
            | Action::CapsWord
            | Action::Haptic(_)
            | Action::LayerLock(_)
            | Action::MacroRecord(_)
            | Action::MacroStop
            | Action::MacroPlay(_)
            | Action::OutputSelect(_)
            | Action::Rgb(_)
            | Action::SendString(_)
            | Action::Sleep
            | Action::SlowKeysToggle
            | Action::StickyKeysToggle => {}
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
        decision: TapHoldDecision,
        mut rec: TapHoldRecognizer,
        now_ms: u64,
        emit: &mut F,
    ) where
        F: FnMut(HidReport) + ?Sized,
    {
        match decision {
            TapHoldDecision::Tap(kc) => {
                self.last_tap_hold_tap = Some((rec.tap_pos(), now_ms));
                // The recognizer consumed the key's own release, so
                // handle_release never runs for this position — clear its
                // press cache here or the next press would be ignored as a
                // duplicate.
                self.layer_stack.clear_cache(rec.tap_pos(), COLS);
                self.press_keycode(kc, emit);
                rec.drain_pending(|e| {
                    let _ = self.pending.push_back(e);
                });
                self.drain_pending(now_ms, emit);
                self.release_keycode(kc, emit);
            }
            TapHoldDecision::HoldMod(m) => {
                let pos = rec.tap_pos();
                self.modifier_state.press(m);
                self.emit_keyboard(emit);
                let _ = self.active_holds.push(ActiveHold {
                    pos,
                    decision: HoldDecision::Mod(m),
                });
                rec.drain_pending(|e| {
                    let _ = self.pending.push_back(e);
                });
            }
            TapHoldDecision::TapToggleLayer(n) => {
                // Same as Tap: the recognizer consumed the release, so clear
                // the press cache it would have cleared.
                self.layer_stack.clear_cache(rec.tap_pos(), COLS);
                self.layer_stack.apply(LayerOp::Toggle(n));
                self.check_tri_layers();
                rec.drain_pending(|e| {
                    let _ = self.pending.push_back(e);
                });
                self.drain_pending(now_ms, emit);
            }
            TapHoldDecision::HoldLayer(n) => {
                let pos = rec.tap_pos();
                self.layer_stack.apply(LayerOp::On(n));
                let _ = self.active_holds.push(ActiveHold {
                    pos,
                    decision: HoldDecision::Layer(n),
                });
                rec.drain_pending(|e| {
                    let _ = self.pending.push_back(e);
                });
            }
        }
    }

    fn apply_hold_release<F>(&mut self, decision: HoldDecision, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        match decision {
            HoldDecision::Mod(m) => {
                self.modifier_state.release(m);
                self.emit_keyboard(emit);
            }
            HoldDecision::Layer(n) => {
                self.layer_stack.apply(LayerOp::Off(n));
                self.check_tri_layers();
            }
        }
    }

    fn press_keycode<F>(&mut self, kc: KeyCode, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if kc == KeyCode::Backspace {
            if let Some(ref mut t) = self.telemetry {
                t.core.record_error();
            }
        }

        if !self.repeat_active {
            self.last_keycode = Some(kc);
        }

        let mut caps_shift_applied = false;
        if self.caps_word_active {
            if Self::is_caps_word_shift(kc) {
                self.modifier_state.press(ModifierKey::LSHIFT);
                caps_shift_applied = true;
            } else if !Self::is_caps_word_continue(kc) {
                self.caps_word_active = false;
            }
        }

        #[cfg(feature = "autocorrect")]
        if self.autocorrect_offload {
            // Offload mode: hand the keycode to the remote engine via the task
            // layer instead of searching the trie on this (real-time) path.
            let _ = self.typed_out.push_back(kc);
        } else if let Some(ref mut engine) = self.autocorrect {
            if let Some(correction) = engine.process_keycode(kc, 0) {
                for _ in 0..correction.backspaces {
                    let _ = self.playback_queue.push_back(KeyCode::Backspace);
                }
                for &uid in correction.replacement_slice() {
                    if let Some(rkc) = KeyCode::from_usage_id(uid) {
                        let _ = self.playback_queue.push_back(rkc);
                    }
                }
                self.drain_playback(emit);
            }
        }

        #[cfg(feature = "autocomplete")]
        if let Some(ref mut engine) = self.autocomplete {
            engine.process_keycode(kc);
        }

        if let Some(m) = kc.as_modifier() {
            self.modifier_state.press(m);
        } else {
            let uid = kc.usage_id();
            if uid != 0 && !self.pressed_keys.contains(&uid) {
                let _ = self.pressed_keys.push(uid);
            }
        }
        self.emit_keyboard(emit);
        self.modifier_state.consume_oneshot();

        if caps_shift_applied {
            self.modifier_state.release(ModifierKey::LSHIFT);
        }
    }

    fn release_keycode<F>(&mut self, kc: KeyCode, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if let Some(m) = kc.as_modifier() {
            self.modifier_state.release(m);
        } else {
            let uid = kc.usage_id();
            if let Some(idx) = self.pressed_keys.iter().position(|&k| k == uid) {
                self.pressed_keys.swap_remove(idx);
            }
        }
        self.modifier_state.clear_weak();
        self.emit_keyboard(emit);
    }

    fn is_caps_word_shift(kc: KeyCode) -> bool {
        let uid = kc.usage_id();
        (0x04..=0x1D).contains(&uid)
    }

    fn is_caps_word_continue(kc: KeyCode) -> bool {
        let uid = kc.usage_id();
        (0x04..=0x1D).contains(&uid)
            || (0x1E..=0x27).contains(&uid)
            || uid == 0x2D
            || uid == 0x2A
            || uid == 0x4C
    }

    fn check_tri_layers(&mut self) {
        for def in self.tri_layers {
            let both = (1 << u32::from(def.layer1)) | (1 << u32::from(def.layer2));
            if self.layer_stack.state() & both == both {
                self.layer_stack.apply(LayerOp::On(def.target));
            } else {
                self.layer_stack.apply(LayerOp::Off(def.target));
            }
        }
    }

    fn drain_playback<F>(&mut self, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        while let Some(kc) = self.playback_queue.pop_front() {
            self.press_keycode(kc, emit);
            self.release_keycode(kc, emit);
        }
    }

    fn playback_steno_translation<F: FnMut(HidReport) + ?Sized>(
        &mut self,
        translation: &[u8],
        emit: &mut F,
    ) {
        let mut playback = core::mem::take(&mut self.steno_playback);
        let mut steno_telem = self
            .telemetry
            .as_mut()
            .map(|t| core::mem::take(&mut t.steno));
        playback.play_translation(translation, steno_telem.as_mut(), self, emit);
        if let (Some(ref mut t), Some(telem)) = (&mut self.telemetry, steno_telem) {
            t.steno = telem;
        }
        self.steno_playback = playback;
    }

    fn handle_mouse_press<F>(&mut self, action: MouseAction, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        match action {
            MouseAction::Move { dx, dy } => {
                // Keymap mouse-keys stay i8 — a keymap constant has no use for
                // 16-bit range; only a sensor does. Widened at the boundary.
                emit(HidReport::Mouse(MouseReport {
                    buttons: self.mouse_buttons,
                    dx: dx.into(),
                    dy: dy.into(),
                    scroll: 0,
                }));
            }
            MouseAction::Click(btn) => {
                self.mouse_buttons |= 1 << btn.saturating_sub(1);
                emit(HidReport::Mouse(MouseReport {
                    buttons: self.mouse_buttons,
                    dx: 0,
                    dy: 0,
                    scroll: 0,
                }));
            }
            MouseAction::Scroll(amount) => {
                emit(HidReport::Mouse(MouseReport {
                    buttons: self.mouse_buttons,
                    dx: 0,
                    dy: 0,
                    scroll: amount,
                }));
            }
        }
    }

    fn handle_mouse_release<F>(&mut self, action: MouseAction, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if let MouseAction::Click(btn) = action {
            self.mouse_buttons &= !(1 << btn.saturating_sub(1));
            emit(HidReport::Mouse(MouseReport {
                buttons: self.mouse_buttons,
                dx: 0,
                dy: 0,
                scroll: 0,
            }));
        }
    }

    fn emit_midi_press<F>(action: MidiAction, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        let event = match action {
            MidiAction::NoteOn {
                channel,
                note,
                velocity,
            } => MidiEvent::NoteOn {
                channel,
                note,
                velocity,
            },
            MidiAction::NoteOff { channel, note } => MidiEvent::NoteOff { channel, note },
            MidiAction::Cc {
                channel,
                controller,
                value,
            } => MidiEvent::Cc {
                channel,
                controller,
                value,
            },
            MidiAction::PolyPressure {
                channel,
                note,
                pressure,
            } => MidiEvent::PolyPressure {
                channel,
                note,
                pressure,
            },
            MidiAction::ChannelPressure { channel, pressure } => {
                MidiEvent::ChannelPressure { channel, pressure }
            }
        };
        emit(HidReport::Midi(event));
    }

    fn emit_midi_release<F>(action: MidiAction, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if let MidiAction::NoteOn { channel, note, .. } = action {
            emit(HidReport::Midi(MidiEvent::NoteOff { channel, note }));
        }
    }

    fn handle_gamepad_press<F>(&mut self, action: GamepadAction, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        let report = self.gamepad.press(action);
        emit(HidReport::Gamepad(report));
    }

    fn handle_gamepad_release<F>(&mut self, action: GamepadAction, emit: &mut F)
    where
        F: FnMut(HidReport) + ?Sized,
    {
        if let Some(report) = self.gamepad.release(action) {
            emit(HidReport::Gamepad(report));
        }
    }

    fn apply_override(&self, kc: KeyCode) -> Option<Action<Ext>> {
        for ov in self.overrides {
            if ov.trigger == kc && self.modifier_state.contains(ov.required_mods) {
                return Some(ov.replacement);
            }
        }
        None
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

impl<const ROWS: usize, const COLS: usize, const LAYERS: usize, const KEYS: usize, Ext, F>
    steno_playback::KeyOutput<F> for KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>
where
    Ext: Copy + PartialEq + 'static,
    F: FnMut(HidReport) + ?Sized,
{
    fn press_keycode(&mut self, kc: KeyCode, emit: &mut F) {
        KeyboardProcessor::press_keycode(self, kc, emit);
    }

    fn release_keycode(&mut self, kc: KeyCode, emit: &mut F) {
        KeyboardProcessor::release_keycode(self, kc, emit);
    }

    fn press_modifier(&mut self, m: ModifierKey) {
        self.modifier_state.press(m);
    }

    fn release_modifier(&mut self, m: ModifierKey) {
        self.modifier_state.release(m);
    }

    fn flush_keyboard(&self, emit: &mut F) {
        KeyboardProcessor::emit_keyboard(self, emit);
    }

    fn drain_playback_queue(&mut self, emit: &mut F) {
        KeyboardProcessor::drain_playback(self, emit);
    }
}
