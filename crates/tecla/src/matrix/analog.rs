//! Analog (Hall-effect style) matrix scanning core.
//!
//! Unlike [`crate::matrix::MatrixState`], which carries a boolean per key,
//! [`AnalogMatrixState`] carries a `u8` "travel" value per key: `0` means
//! fully released/rest and `255` means fully bottomed out. Board drivers are
//! responsible for normalizing raw ADC readings into this `0..=255` range
//! before handing the state to this module.
//!
//! [`RapidTrigger`] turns a stream of raw travel readings into
//! [`KeyTransition`]s (press / bottom-out / release-from-bottom / release)
//! using per-key [`Ema`] smoothing and [`Calibration`].

use crate::pipeline::event::KeyPos;

/// The raw analog travel of every switch in the matrix.
///
/// Internally stored as a row-major 2-D `u8` array. Each cell is a
/// normalized travel value: `0` is fully released/rest, `255` is fully
/// bottomed out. All access goes through safe
/// [`get`](AnalogMatrixState::get) / [`set`](AnalogMatrixState::set)
/// methods — no direct indexing.
pub type AnalogMatrixState<const ROWS: usize, const COLS: usize> =
    crate::matrix::state::Matrix<u8, ROWS, COLS>;

impl<const ROWS: usize, const COLS: usize> AnalogMatrixState<ROWS, COLS> {
    /// Creates a new [`AnalogMatrixState`] with every key at rest (`0`).
    #[must_use]
    pub const fn new() -> Self {
        Self::from_data([[0; COLS]; ROWS])
    }
}

impl<const ROWS: usize, const COLS: usize> Default for AnalogMatrixState<ROWS, COLS> {
    fn default() -> Self {
        Self::new()
    }
}

/// Hardware abstraction for reading an analog (Hall-effect style) key
/// switch matrix.
///
/// Implementations sample per-key ADC channels and return the normalized
/// [`AnalogMatrixState`] each scan cycle. Normalizing raw ADC counts into
/// the `0..=255` travel range is the implementation's responsibility.
pub trait AnalogMatrixScanner<const ROWS: usize, const COLS: usize> {
    /// The error type produced if the scan fails.
    type Error;

    /// Performs one full matrix scan and returns the current travel state.
    fn scan(&mut self) -> Result<AnalogMatrixState<ROWS, COLS>, Self::Error>;
}

/// Async hardware abstraction for reading an analog key switch matrix.
///
/// Like [`AnalogMatrixScanner`] but yields until a scan completes.
///
/// `async fn` in traits is used intentionally here — tecla targets single-threaded
/// Embassy executors where `Send` bounds on futures are not required.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait AsyncAnalogMatrixScanner<const ROWS: usize, const COLS: usize> {
    /// The error type produced if the scan fails.
    type Error;

    /// Waits for the next matrix scan and returns the travel state.
    async fn scan(&mut self) -> Result<AnalogMatrixState<ROWS, COLS>, Self::Error>;
}

/// Integer exponential moving average filter for smoothing noisy analog
/// samples.
///
/// `update` applies `value += (sample - value) >> shift` each tick, so a
/// larger `shift` smooths harder but reacts slower.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ema {
    value: i32,
}

impl Ema {
    /// Creates a new [`Ema`] seeded at `initial`.
    #[must_use]
    #[expect(clippy::as_conversions, reason = "u8 to i32 is always lossless")]
    pub const fn new(initial: u8) -> Self {
        Self {
            value: initial as i32,
        }
    }

    /// Folds in one new `sample`, smoothed by `shift`, and returns the
    /// updated value clamped to `0..=255`.
    ///
    /// `shift` is clamped to `0..=31` (i32's bit width) so an out-of-range
    /// tuning value can't panic (debug) or wrap the shift amount (release);
    /// it degrades to maximum smoothing instead.
    #[expect(clippy::as_conversions, reason = "clamped to u8 range below")]
    pub fn update(&mut self, sample: u8, shift: u8) -> u8 {
        let shift = shift.min(31);
        self.value += (i32::from(sample) - self.value) >> shift;
        self.value = self.value.clamp(0, 255);
        #[expect(clippy::cast_possible_truncation, reason = "clamped to 0..=255 above")]
        #[expect(clippy::cast_sign_loss, reason = "clamped to 0..=255 above")]
        let value = self.value as u8;
        value
    }

    /// Returns the current smoothed value, clamped to `0..=255`.
    #[must_use]
    #[expect(clippy::as_conversions, reason = "clamped to u8 range below")]
    pub fn value(&self) -> u8 {
        let clamped = self.value.clamp(0, 255);
        #[expect(clippy::cast_sign_loss, reason = "clamped to 0..=255 above")]
        let value = clamped as u8;
        value
    }
}

/// Auto-calibrates the observed rest (shallow) and bottom (deep) raw values
/// for one key.
///
/// Expand-only: never re-narrows if the physical range genuinely shrinks —
/// this is a deliberate simplification. A small epsilon guards against
/// widening the range on ordinary noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    rest: u8,
    bottom: u8,
}

/// Ignore observed samples within this many units of the current bound —
/// treated as noise rather than genuine range expansion.
const CALIBRATION_EPSILON: u8 = 3;

impl Calibration {
    /// Creates a new [`Calibration`] with the given initial rest and
    /// bottom raw values.
    #[must_use]
    pub const fn new(rest: u8, bottom: u8) -> Self {
        Self { rest, bottom }
    }

    /// Observes one raw `sample`, expanding `rest` downward or `bottom`
    /// upward if the sample is clearly (beyond a small epsilon) outside the
    /// currently observed range.
    pub fn observe(&mut self, sample: u8) {
        if sample.saturating_add(CALIBRATION_EPSILON) < self.rest {
            self.rest = sample;
        }
        if sample > self.bottom.saturating_add(CALIBRATION_EPSILON) {
            self.bottom = sample;
        }
    }

    /// Returns the observed rest (shallow) raw value.
    #[must_use]
    pub const fn rest(&self) -> u8 {
        self.rest
    }

    /// Returns the observed bottom (deep) raw value.
    #[must_use]
    pub const fn bottom(&self) -> u8 {
        self.bottom
    }

    /// Maps `sample` from the observed `[rest, bottom]` raw range onto a
    /// full `0..=255` scale.
    #[must_use]
    #[expect(
        clippy::as_conversions,
        reason = "result bounded to 0..=255 by min() below"
    )]
    pub fn normalize(&self, sample: u8) -> u8 {
        let offset = sample.saturating_sub(self.rest);
        let span = self.bottom.saturating_sub(self.rest).max(1);
        let scaled = (u16::from(offset) * 255 / u16::from(span)).min(255);
        scaled as u8
    }
}

/// The transition phases a key can emit as it travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerPhase {
    /// The key crossed its actuation threshold and is now considered down.
    Press,
    /// The key reached the deep "bottomed out" threshold while down.
    BottomOut,
    /// The key backed off the bottom-out threshold while still held down.
    ReleaseFromBottom,
    /// The key crossed its release threshold and is now considered up.
    Release,
}

/// One rapid-trigger transition emitted by [`RapidTrigger::process`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyTransition {
    /// The key's position in the matrix.
    pub pos: KeyPos,
    /// The phase this transition represents.
    pub phase: TriggerPhase,
    /// The normalized travel value at the moment of the transition.
    pub value: u8,
}

/// Tunable thresholds for [`RapidTrigger`], all in normalized `0..=255`
/// units (except `ema_shift`, which is a bit-shift amount).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RapidTriggerConfig {
    /// How far the key must move down from its most-recent local minimum
    /// to (re-)actuate.
    pub press_sensitivity: u8,
    /// How far the key must move up from its most-recent local maximum to
    /// release.
    pub release_sensitivity: u8,
    /// Absolute floor: never actuate below this normalized depth, avoids
    /// noise-triggering right at rest.
    pub actuation_point: u8,
    /// Absolute ceiling: never require releasing above this normalized
    /// depth (should be `<= actuation_point`).
    pub release_point: u8,
    /// Absolute deep threshold for the [`TriggerPhase::BottomOut`] /
    /// [`TriggerPhase::ReleaseFromBottom`] phases (`> actuation_point`).
    pub bottom_out_point: u8,
    /// Smoothing factor passed to [`Ema::update`].
    pub ema_shift: u8,
}

impl Default for RapidTriggerConfig {
    fn default() -> Self {
        Self {
            press_sensitivity: 10,
            release_sensitivity: 10,
            actuation_point: 20,
            release_point: 15,
            bottom_out_point: 230,
            ema_shift: 2,
        }
    }
}

/// Per-key rapid-trigger state.
struct KeyState {
    ema: Ema,
    calibration: Calibration,
    down: bool,
    bottomed: bool,
    /// Deepest value seen since the last reversal while down; shallowest
    /// value seen while up.
    extremum: u8,
}

/// Converts raw analog travel readings into [`KeyTransition`]s using
/// per-key rapid-trigger logic: a key (re-)actuates as soon as it reverses
/// direction by [`RapidTriggerConfig::press_sensitivity`] /
/// [`RapidTriggerConfig::release_sensitivity`] from its local extremum,
/// rather than at a single fixed depth.
pub struct RapidTrigger<const ROWS: usize, const COLS: usize> {
    config: RapidTriggerConfig,
    keys: [[KeyState; COLS]; ROWS],
}

impl<const ROWS: usize, const COLS: usize> RapidTrigger<ROWS, COLS> {
    /// Creates a new [`RapidTrigger`] with every key calibrated to
    /// `initial_rest` / `initial_bottom` and released.
    #[must_use]
    pub fn new(config: RapidTriggerConfig, initial_rest: u8, initial_bottom: u8) -> Self {
        Self {
            config,
            keys: core::array::from_fn(|_| {
                core::array::from_fn(|_| KeyState {
                    ema: Ema::new(initial_rest),
                    calibration: Calibration::new(initial_rest, initial_bottom),
                    down: false,
                    bottomed: false,
                    extremum: initial_rest,
                })
            }),
        }
    }

    /// Processes one raw scan, invoking `on_transition` for every
    /// [`KeyTransition`] that occurred this tick.
    ///
    /// A key can emit up to two transitions in the same tick (e.g. Press
    /// followed by BottomOut), and a full matrix scan can produce more
    /// transitions than any fixed-capacity buffer could safely bound, so
    /// transitions are delivered by callback instead of collected into a
    /// `Vec` — nothing is ever silently dropped, regardless of ROWS*COLS.
    ///
    /// `now_ms` is accepted for API symmetry with [`crate::matrix::debounce::Debouncer`]
    /// and reserved for future direction-reversal-timing use; the current
    /// algorithm is purely state-based.
    pub fn process(
        &mut self,
        raw: &AnalogMatrixState<ROWS, COLS>,
        _now_ms: u64,
        mut on_transition: impl FnMut(KeyTransition),
    ) {
        let config = &self.config;

        for (r, row) in self.keys.iter_mut().enumerate() {
            for (c, key) in row.iter_mut().enumerate() {
                let Some(raw_sample) = raw.get(r, c) else {
                    continue;
                };
                let pos = KeyPos::new(r as u8, c as u8);

                let smoothed = key.ema.update(raw_sample, config.ema_shift);
                key.calibration.observe(smoothed);
                let v = key.calibration.normalize(smoothed);

                if !key.down {
                    key.extremum = key.extremum.min(v);
                    if v >= key.extremum.saturating_add(config.press_sensitivity)
                        && v >= config.actuation_point
                    {
                        key.down = true;
                        key.extremum = v;
                        on_transition(KeyTransition {
                            pos,
                            phase: TriggerPhase::Press,
                            value: v,
                        });
                    }
                }

                if key.down {
                    key.extremum = key.extremum.max(v);
                    if !key.bottomed && v >= config.bottom_out_point {
                        key.bottomed = true;
                        on_transition(KeyTransition {
                            pos,
                            phase: TriggerPhase::BottomOut,
                            value: v,
                        });
                    } else if key.bottomed
                        && v <= config
                            .bottom_out_point
                            .saturating_sub(config.release_sensitivity)
                    {
                        key.bottomed = false;
                        on_transition(KeyTransition {
                            pos,
                            phase: TriggerPhase::ReleaseFromBottom,
                            value: v,
                        });
                    }

                    if v <= key.extremum.saturating_sub(config.release_sensitivity)
                        && v <= config.release_point
                    {
                        key.down = false;
                        key.bottomed = false;
                        key.extremum = v;
                        on_transition(KeyTransition {
                            pos,
                            phase: TriggerPhase::Release,
                            value: v,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AnalogMatrixState ────────────────────────────────────────────────────

    #[test]
    fn new_analog_matrix_state_has_all_keys_at_rest() {
        let state = AnalogMatrixState::<4, 12>::new();
        for (_, _, value) in state.iter() {
            assert_eq!(value, 0);
        }
    }

    #[test]
    fn default_analog_matrix_state_equals_new() {
        let a = AnalogMatrixState::<3, 6>::new();
        let b = AnalogMatrixState::<3, 6>::default();
        assert_eq!(a, b);
    }

    #[test]
    fn new_analog_matrix_state_set_and_get_round_trips() {
        let mut state = AnalogMatrixState::<4, 12>::new();
        state.set(2, 5, 200);
        assert_eq!(state.get(2, 5), Some(200));
    }

    // ── Ema ──────────────────────────────────────────────────────────────────

    #[test]
    fn ema_converges_toward_step_input() {
        // Integer EMA with a right-shift step has a small rounding dead
        // zone near the target (the >>shift delta truncates to 0 once the
        // remaining gap is smaller than `1 << shift`), so it settles very
        // close to but not exactly at the step value.
        let mut ema = Ema::new(0);
        for _ in 0..200 {
            ema.update(255, 2);
        }
        assert!(ema.value() >= 250);
    }

    #[test]
    fn ema_moves_toward_sample_each_update() {
        let mut ema = Ema::new(0);
        let first = ema.update(100, 2);
        assert!(first > 0);
        assert!(first < 100);
    }

    #[test]
    fn ema_clamps_to_upper_bound() {
        let mut ema = Ema::new(250);
        for _ in 0..50 {
            ema.update(255, 1);
        }
        assert!(ema.value() >= 253);
    }

    #[test]
    fn ema_clamps_to_lower_bound() {
        let mut ema = Ema::new(5);
        for _ in 0..50 {
            ema.update(0, 1);
        }
        assert_eq!(ema.value(), 0);
    }

    #[test]
    fn ema_shift_at_or_above_32_does_not_panic() {
        // A shift >= i32's bit width used to panic in debug builds and wrap
        // the shift amount (mod 32) in release builds; it must now clamp
        // instead of overflowing.
        let mut ema = Ema::new(0);
        assert_eq!(ema.update(255, 32), 0);
        assert_eq!(ema.update(255, 255), 0);
    }

    // ── Calibration ──────────────────────────────────────────────────────────

    #[test]
    fn calibration_widens_rest_downward_on_clear_outlier() {
        let mut cal = Calibration::new(50, 200);
        cal.observe(10);
        assert_eq!(cal.rest(), 10);
    }

    #[test]
    fn calibration_widens_bottom_upward_on_clear_outlier() {
        let mut cal = Calibration::new(50, 200);
        cal.observe(240);
        assert_eq!(cal.bottom(), 240);
    }

    #[test]
    fn calibration_ignores_noise_within_epsilon() {
        let mut cal = Calibration::new(50, 200);
        cal.observe(51); // within epsilon of rest, should not widen bottom side
        cal.observe(48); // within epsilon of rest
        assert_eq!(cal.rest(), 50);
        cal.observe(198); // within epsilon of bottom
        assert_eq!(cal.bottom(), 200);
    }

    #[test]
    fn normalize_maps_rest_to_zero() {
        let cal = Calibration::new(50, 200);
        assert_eq!(cal.normalize(50), 0);
    }

    #[test]
    fn normalize_maps_bottom_to_255() {
        let cal = Calibration::new(50, 200);
        assert_eq!(cal.normalize(200), 255);
    }

    #[test]
    fn normalize_maps_midpoint_roughly_to_half() {
        let cal = Calibration::new(0, 200);
        let mid = cal.normalize(100);
        assert!((120..135).contains(&mid));
    }

    #[test]
    fn normalize_saturates_below_rest() {
        let cal = Calibration::new(50, 200);
        assert_eq!(cal.normalize(0), 0);
    }

    // ── RapidTrigger ─────────────────────────────────────────────────────────

    fn raw_state<const R: usize, const C: usize>(
        row: usize,
        col: usize,
        value: u8,
    ) -> AnalogMatrixState<R, C> {
        let mut s = AnalogMatrixState::new();
        s.set(row, col, value);
        s
    }

    /// Test helper: collects one tick's transitions from the callback-based
    /// [`RapidTrigger::process`] into a `Vec` for assertions.
    fn collect_transitions<const R: usize, const C: usize>(
        rt: &mut RapidTrigger<R, C>,
        raw: &AnalogMatrixState<R, C>,
    ) -> heapless::Vec<KeyTransition, 4> {
        let mut transitions = heapless::Vec::new();
        rt.process(raw, 0, |t| {
            let _ = transitions.push(t);
        });
        transitions
    }

    #[test]
    fn rapid_trigger_fires_press_when_crossing_actuation_threshold() {
        let config = RapidTriggerConfig::default();
        let mut rt = RapidTrigger::<1, 1>::new(config, 0, 255);

        // Drive the EMA up over several ticks until it crosses actuation
        // threshold + press sensitivity from its resting extremum.
        let mut fired = None;
        for _ in 0..100 {
            let raw = raw_state::<1, 1>(0, 0, 255);
            let transitions = collect_transitions(&mut rt, &raw);
            if let Some(t) = transitions.iter().find(|t| t.phase == TriggerPhase::Press) {
                fired = Some(*t);
                break;
            }
        }
        let t = fired.expect("press should fire");
        assert_eq!(t.pos, KeyPos::new(0, 0));
        assert!(t.value >= config.actuation_point);
    }

    #[test]
    fn rapid_trigger_does_not_fire_press_below_actuation_point() {
        let config = RapidTriggerConfig::default();
        let mut rt = RapidTrigger::<1, 1>::new(config, 0, 255);

        // Small dip that never reaches actuation_point.
        for _ in 0..20 {
            let raw = raw_state::<1, 1>(0, 0, 5);
            let transitions = collect_transitions(&mut rt, &raw);
            assert!(transitions.is_empty());
        }
    }

    #[test]
    fn rapid_trigger_fires_bottom_out_and_release_from_bottom() {
        let config = RapidTriggerConfig::default();
        let mut rt = RapidTrigger::<1, 1>::new(config, 0, 255);

        // Full press to bottom.
        let mut saw_bottom_out = false;
        for _ in 0..100 {
            let raw = raw_state::<1, 1>(0, 0, 255);
            let transitions = collect_transitions(&mut rt, &raw);
            if transitions
                .iter()
                .any(|t| t.phase == TriggerPhase::BottomOut)
            {
                saw_bottom_out = true;
                break;
            }
        }
        assert!(saw_bottom_out, "bottom-out should fire on full press");

        // Back off from the bottom but stay held.
        let mut saw_release_from_bottom = false;
        for _ in 0..100 {
            let raw = raw_state::<1, 1>(0, 0, 100);
            let transitions = collect_transitions(&mut rt, &raw);
            if transitions
                .iter()
                .any(|t| t.phase == TriggerPhase::ReleaseFromBottom)
            {
                saw_release_from_bottom = true;
                break;
            }
            if transitions.iter().any(|t| t.phase == TriggerPhase::Release) {
                break;
            }
        }
        assert!(saw_release_from_bottom, "release-from-bottom should fire");
    }

    #[test]
    fn rapid_trigger_fires_release_on_backoff_from_extremum() {
        let config = RapidTriggerConfig::default();
        let mut rt = RapidTrigger::<1, 1>::new(config, 0, 255);

        // Press partway down (not to bottom).
        for _ in 0..100 {
            let raw = raw_state::<1, 1>(0, 0, 100);
            let transitions = collect_transitions(&mut rt, &raw);
            if transitions.iter().any(|t| t.phase == TriggerPhase::Press) {
                break;
            }
        }

        // Back off enough to release.
        let mut saw_release = false;
        for _ in 0..100 {
            let raw = raw_state::<1, 1>(0, 0, 0);
            let transitions = collect_transitions(&mut rt, &raw);
            if transitions.iter().any(|t| t.phase == TriggerPhase::Release) {
                saw_release = true;
                break;
            }
        }
        assert!(saw_release, "release should fire on backoff");
    }

    #[test]
    fn rapid_trigger_does_not_spuriously_repress_on_small_dither() {
        let config = RapidTriggerConfig::default();
        let mut rt = RapidTrigger::<1, 1>::new(config, 0, 255);

        // Press down and hold.
        let mut pressed = false;
        for _ in 0..100 {
            let raw = raw_state::<1, 1>(0, 0, 150);
            let transitions = collect_transitions(&mut rt, &raw);
            if transitions.iter().any(|t| t.phase == TriggerPhase::Press) {
                pressed = true;
            }
            if pressed {
                break;
            }
        }
        assert!(pressed);

        // Dither by a couple of units around the held position — should
        // never emit another Press (a key that's already down only needs
        // to re-cross release_sensitivity/press_sensitivity from a real
        // reversal, not from single-unit noise).
        let dither = [150u8, 148, 151, 149, 150, 147, 152];
        for &v in dither.iter().cycle().take(50) {
            let raw = raw_state::<1, 1>(0, 0, v);
            let transitions = collect_transitions(&mut rt, &raw);
            assert!(
                !transitions.iter().any(|t| t.phase == TriggerPhase::Press),
                "dither must not re-trigger a press"
            );
        }
    }

    #[test]
    fn rapid_trigger_fast_full_press_in_one_tick_hits_press_and_bottom_out() {
        let config = RapidTriggerConfig::default();
        let mut rt = RapidTrigger::<1, 1>::new(config, 0, 255);
        // First warm up the EMA/calibration close to rest with a few ticks
        // at 0, then slam straight to a full-travel raw sample in a single
        // tick, relying on the fact that Press and the bottom-out/release
        // checks both run within one `process` call.
        for _ in 0..5 {
            let raw = raw_state::<1, 1>(0, 0, 0);
            rt.process(&raw, 0, |_| {});
        }
        // Feed max repeatedly; since the EMA lags, either the same tick or
        // a subsequent one should show both Press and BottomOut without
        // ever missing the press step.
        let mut saw_press = false;
        let mut saw_bottom_out_after_press = false;
        for _ in 0..100 {
            let raw = raw_state::<1, 1>(0, 0, 255);
            let transitions = collect_transitions(&mut rt, &raw);
            if transitions.iter().any(|t| t.phase == TriggerPhase::Press) {
                saw_press = true;
            }
            if saw_press
                && transitions
                    .iter()
                    .any(|t| t.phase == TriggerPhase::BottomOut)
            {
                saw_bottom_out_after_press = true;
            }
            if saw_bottom_out_after_press {
                break;
            }
        }
        assert!(saw_press);
        assert!(saw_bottom_out_after_press);
    }

    #[test]
    fn rapid_trigger_delivers_more_than_16_transitions_in_one_tick() {
        // A 4x5 = 20-key grid where every key releases in the same tick used
        // to silently lose transitions past the old fixed-capacity-16 `Vec`;
        // the callback-based API must deliver all of them.
        const ROWS: usize = 4;
        const COLS: usize = 5;
        let config = RapidTriggerConfig::default();
        let mut rt = RapidTrigger::<ROWS, COLS>::new(config, 0, 255);

        let full_state = |value: u8| {
            let mut s = AnalogMatrixState::<ROWS, COLS>::new();
            for r in 0..ROWS {
                for c in 0..COLS {
                    s.set(r, c, value);
                }
            }
            s
        };

        // Warm up every key to Press.
        let mut pressed = [[false; COLS]; ROWS];
        let mut pressed_count = 0usize;
        for _ in 0..200 {
            let raw = full_state(255);
            rt.process(&raw, 0, |t| {
                let cell = &mut pressed[usize::from(t.pos.row)][usize::from(t.pos.col)];
                if t.phase == TriggerPhase::Press && !*cell {
                    *cell = true;
                    pressed_count += 1;
                }
            });
            if pressed_count == ROWS * COLS {
                break;
            }
        }
        assert_eq!(pressed_count, ROWS * COLS, "every key should press");

        // Every key shares the same trajectory (identical initial state and
        // identical raw feed), so they all cross the release threshold on
        // the same tick — the first tick that emits any Release must emit
        // all 20, proving the callback doesn't cap or drop past 16.
        let raw = full_state(0);
        let mut release_count = 0usize;
        for _ in 0..200 {
            release_count = 0;
            rt.process(&raw, 0, |t| {
                if t.phase == TriggerPhase::Release {
                    release_count += 1;
                }
            });
            if release_count > 0 {
                break;
            }
        }
        assert_eq!(
            release_count,
            ROWS * COLS,
            "all 20 releases must be delivered in the same tick, not capped at 16"
        );
    }
}
