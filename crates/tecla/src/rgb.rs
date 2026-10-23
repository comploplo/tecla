//! RGB animation engine — pure core logic, no hardware drivers.
//!
//! Provides color types, an [`Animation`] trait, seven built-in animations,
//! and an [`RgbController`] that board crates own and tick on a timer.

use crate::pipeline::event::KeyEvent;

/// An RGB color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb {
    /// Creates a new RGB color.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Black (all channels zero).
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    /// White (all channels max).
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
    };
    /// Pure red.
    pub const RED: Self = Self { r: 255, g: 0, b: 0 };
    /// Pure green.
    pub const GREEN: Self = Self { r: 0, g: 255, b: 0 };
    /// Pure blue.
    pub const BLUE: Self = Self { r: 0, g: 0, b: 255 };
}

/// An HSV color value (hue 0–255, saturation 0–255, value 0–255).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hsv {
    /// Hue (0–255 maps to 0°–360°).
    pub h: u8,
    /// Saturation (0 = grey, 255 = fully saturated).
    pub s: u8,
    /// Value / brightness (0 = black, 255 = full brightness).
    pub v: u8,
}

impl Hsv {
    /// Creates a new HSV color.
    #[must_use]
    pub const fn new(h: u8, s: u8, v: u8) -> Self {
        Self { h, s, v }
    }
}

#[expect(
    clippy::integer_division_remainder_used,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "u16 arithmetic with values ≤ 255*258; truncation to u8 is intentional"
)]
fn scale(channel: u8, brightness: u8) -> u8 {
    (u16::from(channel) * u16::from(brightness) / 255) as u8
}

/// Converts an HSV color to RGB.
#[must_use]
#[expect(
    clippy::integer_division_remainder_used,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "standard HSV→RGB with 6 sectors over u8 range; intermediate u16 values ≤ 65535"
)]
pub fn hsv_to_rgb(hsv: Hsv) -> Rgb {
    if hsv.s == 0 {
        return Rgb::new(hsv.v, hsv.v, hsv.v);
    }

    let region = hsv.h / 43;
    let remainder = (u16::from(hsv.h) - u16::from(region) * 43) * 6;

    let p = (u16::from(hsv.v) * (255 - u16::from(hsv.s)) / 255) as u8;
    let q = (u16::from(hsv.v) * (255 - (u16::from(hsv.s) * remainder / 255)) / 255) as u8;
    let t = (u16::from(hsv.v) * (255 - (u16::from(hsv.s) * (255 - remainder) / 255)) / 255) as u8;

    match region {
        0 => Rgb::new(hsv.v, t, p),
        1 => Rgb::new(q, hsv.v, p),
        2 => Rgb::new(p, hsv.v, t),
        3 => Rgb::new(p, q, hsv.v),
        4 => Rgb::new(t, p, hsv.v),
        _ => Rgb::new(hsv.v, p, q),
    }
}

/// Trait for LED animations.
pub trait Animation {
    /// Advance the animation by one frame.
    fn tick(&mut self, led_count: usize, elapsed_ms: u32, buffer: &mut [Rgb]);

    /// React to a key event (optional, for reactive animations).
    fn on_key_event(&mut self, _event: KeyEvent, _led_count: usize, _buffer: &mut [Rgb]) {}

    /// Adopt the controller's base color.
    ///
    /// [`RgbController::tick`] calls this every frame so the hue/saturation
    /// keycodes ([`RgbCommand::HueUp`] and friends) reach the animation.
    /// Animations with no single base color leave it as a no-op.
    fn set_base_color(&mut self, _hsv: Hsv) {}
}

/// Sets all LEDs to a single static color.
#[derive(Debug, Clone, Copy)]
pub struct SolidColor {
    /// The color to display.
    pub color: Rgb,
}

impl Animation for SolidColor {
    fn tick(&mut self, _led_count: usize, _elapsed_ms: u32, buffer: &mut [Rgb]) {
        for led in &mut *buffer {
            *led = self.color;
        }
    }

    fn on_key_event(&mut self, _event: KeyEvent, _led_count: usize, _buffer: &mut [Rgb]) {}

    fn set_base_color(&mut self, hsv: Hsv) {
        self.color = hsv_to_rgb(hsv);
    }
}

/// Rotating rainbow across all LEDs.
#[derive(Debug, Clone, Copy)]
pub struct RainbowCycle {
    /// Animation speed (higher = faster).
    pub speed: u8,
    /// Starting hue offset.
    pub offset: u8,
}

impl Animation for RainbowCycle {
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "hue spread arithmetic with values ≤ 255; truncation to u8 is intentional"
    )]
    fn tick(&mut self, _led_count: usize, elapsed_ms: u32, buffer: &mut [Rgb]) {
        let base_hue = self
            .offset
            .wrapping_add((elapsed_ms.wrapping_mul(u32::from(self.speed)) / 256) as u8);
        let len = buffer.len();
        for (i, led) in buffer.iter_mut().enumerate() {
            let hue = if len > 1 {
                base_hue.wrapping_add((i as u16 * 255 / (len - 1) as u16) as u8)
            } else {
                base_hue
            };
            *led = hsv_to_rgb(Hsv::new(hue, 255, 255));
        }
    }

    fn on_key_event(&mut self, _event: KeyEvent, _led_count: usize, _buffer: &mut [Rgb]) {}
}

/// Pulsing brightness on a single color.
#[derive(Debug, Clone, Copy)]
pub struct Breathing {
    /// Base color.
    pub color: Rgb,
    /// Breathing speed (higher = faster).
    pub speed: u8,
    /// Starting phase offset.
    pub phase: u8,
}

impl Animation for Breathing {
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "phase arithmetic with values ≤ 255; truncation to u8 is intentional"
    )]
    fn tick(&mut self, _led_count: usize, elapsed_ms: u32, buffer: &mut [Rgb]) {
        let angle = self
            .phase
            .wrapping_add((elapsed_ms.wrapping_mul(u32::from(self.speed)) / 256) as u8);

        let idx = angle as usize;
        let brightness: u8 = if idx < 128 {
            (idx * 2) as u8
        } else {
            ((255 - idx) * 2) as u8
        };

        let scaled = Rgb::new(
            scale(self.color.r, brightness),
            scale(self.color.g, brightness),
            scale(self.color.b, brightness),
        );

        for led in &mut *buffer {
            *led = scaled;
        }
    }

    fn on_key_event(&mut self, _event: KeyEvent, _led_count: usize, _buffer: &mut [Rgb]) {}

    fn set_base_color(&mut self, hsv: Hsv) {
        self.color = hsv_to_rgb(hsv);
    }
}

/// Keys light up on press and fade out over time.
#[derive(Debug, Clone)]
pub struct ReactivePress {
    /// Color for lit keys.
    pub color: Rgb,
    /// Brightness decay per tick (higher = faster fade).
    pub decay: u8,
    /// Per-key brightness values.
    pub brightness: [u8; 128],
}

impl ReactivePress {
    /// Creates a new reactive press animation.
    #[must_use]
    pub fn new(color: Rgb, decay: u8) -> Self {
        Self {
            color,
            decay,
            brightness: [0; 128],
        }
    }
}

impl Animation for ReactivePress {
    fn tick(&mut self, _led_count: usize, _elapsed_ms: u32, buffer: &mut [Rgb]) {
        for (i, led) in buffer.iter_mut().enumerate() {
            if let Some(b) = self.brightness.get(i) {
                *led = Rgb::new(
                    scale(self.color.r, *b),
                    scale(self.color.g, *b),
                    scale(self.color.b, *b),
                );
            } else {
                *led = Rgb::BLACK;
            }
        }
        for b in &mut self.brightness {
            *b = b.saturating_sub(self.decay);
        }
    }

    fn on_key_event(&mut self, event: KeyEvent, _led_count: usize, _buffer: &mut [Rgb]) {
        if event.pressed {
            let idx = event.pos.flat(16);
            if let Some(b) = self.brightness.get_mut(idx) {
                *b = 255;
            }
        }
    }

    fn set_base_color(&mut self, hsv: Hsv) {
        self.color = hsv_to_rgb(hsv);
    }
}

/// Rotating rainbow whose speed reacts to typing speed (approximate WPM),
/// scaled against the user's own observed personal-best pace rather than a
/// fixed assumed maximum.
#[derive(Debug, Clone, Copy)]
pub struct WpmRainbow {
    inner: RainbowCycle,
    last_press_ms: Option<u64>,
    interval_ema_ms: u32,
    /// Whether at least one real inter-press interval has been measured yet.
    /// Until then `interval_ema_ms` is just the seeded placeholder, not a
    /// real pace, so it must not be compared against `max_wpm_seen`.
    has_interval: bool,
    ticks_since_press: u32,
    max_wpm_seen: u16,
}

impl WpmRainbow {
    /// Ambient/idle rainbow speed floor.
    const IDLE_SPEED: u8 = 8;
    /// Matches [`RainbowCycle::speed`]'s full `u8` range.
    const MAX_SPEED: u8 = 255;
    /// Modest initial personal-best seed so the first few keystrokes don't
    /// already peg the animation at full speed; grows as the user types
    /// faster over the session.
    const INITIAL_MAX_WPM: u16 = 40;
    /// Bit-shift EMA smoothing factor for the inter-press interval (mirrors
    /// [`crate::matrix::analog::Ema::update`]'s pattern).
    const EMA_SHIFT: u8 = 2;
    /// Sane ceiling for a single inter-press interval, so an idle gap of
    /// hours can't produce a nonsensical (or truncated) interval.
    const MAX_INTERVAL_MS: u32 = 60_000;
    /// Ticks of no presses after which speed decays back to the ambient
    /// floor. ~2s at a plausible 60Hz RGB tick rate (4s at 30Hz) — this is a
    /// cosmetic effect and doesn't need to be exact.
    const IDLE_TICKS: u32 = 120;

    /// Creates a new WPM-reactive rainbow, idle at the ambient speed floor.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: RainbowCycle {
                speed: Self::IDLE_SPEED,
                offset: 0,
            },
            last_press_ms: None,
            // No data yet; placeholder only — has_interval gates its use.
            interval_ema_ms: 300,
            has_interval: false,
            ticks_since_press: 0,
            max_wpm_seen: Self::INITIAL_MAX_WPM,
        }
    }

    /// Approximate words-per-minute derived from the smoothed inter-press
    /// interval, using the standard 5-characters-per-word convention.
    #[must_use]
    #[expect(
        clippy::integer_division_remainder_used,
        reason = "keystrokes/min ÷ 5 chars/word folded into one division"
    )]
    pub fn current_wpm(&self) -> u16 {
        let wpm = 12_000 / self.interval_ema_ms.max(1);
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            reason = "wpm clamped to u16::MAX before narrowing"
        )]
        let wpm = wpm.min(u32::from(u16::MAX)) as u16;
        wpm
    }

    /// Returns the highest WPM observed so far this session — a ratcheting
    /// personal best that only ever increases.
    #[must_use]
    pub const fn max_wpm(&self) -> u16 {
        self.max_wpm_seen
    }
}

impl Default for WpmRainbow {
    fn default() -> Self {
        Self::new()
    }
}

impl Animation for WpmRainbow {
    fn tick(&mut self, led_count: usize, elapsed_ms: u32, buffer: &mut [Rgb]) {
        self.ticks_since_press = self.ticks_since_press.saturating_add(1);

        let wpm = self.current_wpm();

        self.inner.speed = if self.ticks_since_press > Self::IDLE_TICKS || !self.has_interval {
            Self::IDLE_SPEED
        } else {
            #[expect(
                clippy::integer_division_remainder_used,
                reason = "scales wpm onto the ambient-floor..=255 speed range"
            )]
            let scaled =
                u32::from(wpm) * u32::from(Self::MAX_SPEED) / u32::from(self.max_wpm_seen.max(1));
            #[expect(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                reason = "scaled clamped to MAX_SPEED (255) before narrowing"
            )]
            let scaled = scaled.min(u32::from(Self::MAX_SPEED)) as u8;
            scaled.max(Self::IDLE_SPEED)
        };

        // Ratchet the personal best *after* scaling speed against it, so this
        // tick's speed is always computed against the pace established
        // before this tick, not a denominator silently rewritten to equal
        // the numerator moments earlier.
        if wpm > self.max_wpm_seen {
            self.max_wpm_seen = wpm;
        }

        self.inner.tick(led_count, elapsed_ms, buffer);
    }

    fn on_key_event(&mut self, event: KeyEvent, led_count: usize, buffer: &mut [Rgb]) {
        if event.pressed {
            if let Some(prev) = self.last_press_ms {
                let interval_ms = event
                    .timestamp_ms
                    .saturating_sub(prev)
                    .min(u64::from(Self::MAX_INTERVAL_MS));
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_truncation,
                    reason = "interval clamped to MAX_INTERVAL_MS (60_000) before narrowing to u32"
                )]
                let interval_ms = interval_ms as u32;

                // Mirror Ema::update's shift-based smoothing (matrix/analog.rs).
                // i32 comfortably holds both operands (both ≤ MAX_INTERVAL_MS);
                // u32 can't go negative so the diff needs a signed type.
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_wrap,
                    reason = "interval_ema_ms ≤ MAX_INTERVAL_MS (60_000), fits i32 losslessly"
                )]
                let mut ema = self.interval_ema_ms as i32;
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_wrap,
                    reason = "interval_ms ≤ MAX_INTERVAL_MS (60_000), fits i32 losslessly"
                )]
                let sample = interval_ms as i32;
                ema += (sample - ema) >> Self::EMA_SHIFT;
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_wrap,
                    reason = "MAX_INTERVAL_MS (60_000) fits i32 losslessly"
                )]
                let max_interval = Self::MAX_INTERVAL_MS as i32;
                ema = ema.clamp(0, max_interval);
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_sign_loss,
                    reason = "ema clamped to 0..=MAX_INTERVAL_MS above"
                )]
                let ema = ema as u32;
                self.interval_ema_ms = ema;
                self.has_interval = true;
            }
            self.last_press_ms = Some(event.timestamp_ms);
            self.ticks_since_press = 0;
        }
        self.inner.on_key_event(event, led_count, buffer);
    }
}

/// Keys accumulate heat on press and slowly cool over time, shifting hue
/// from cool (blue) when barely warm to hot (red) as heat builds. Unlike
/// [`ReactivePress`]'s flash-and-fade, heat lingers so repeated presses on a
/// key visibly build up into a hotspot.
#[derive(Debug, Clone)]
pub struct Heatmap {
    /// Per-key heat values.
    pub heat: [u8; 128],
    /// Heat added per press (saturating at 255).
    pub heat_step: u8,
    /// Heat lost per tick (small, so heat lingers and accumulates).
    pub decay: u8,
}

impl Heatmap {
    /// Creates a new heatmap animation with all keys cold.
    #[must_use]
    pub fn new(heat_step: u8, decay: u8) -> Self {
        Self {
            heat: [0; 128],
            heat_step,
            decay,
        }
    }
}

impl Default for Heatmap {
    fn default() -> Self {
        // Modest step with slow decay so heat accumulates and lingers.
        Self::new(40, 2)
    }
}

impl Animation for Heatmap {
    #[expect(
        clippy::integer_division_remainder_used,
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "heat (≤255) scaled onto the 0..170 hue range; truncation to u8 is intentional"
    )]
    fn tick(&mut self, _led_count: usize, _elapsed_ms: u32, buffer: &mut [Rgb]) {
        for (i, led) in buffer.iter_mut().enumerate() {
            if let Some(&h) = self.heat.get(i) {
                if h > 0 {
                    // heat=255 → hue 0 (red/hot); heat near 0 → hue ~170 (blue/cool).
                    let hue = 170u8.saturating_sub((u16::from(h) * 170 / 255) as u8);
                    *led = hsv_to_rgb(Hsv::new(hue, 255, h));
                } else {
                    *led = Rgb::BLACK;
                }
            } else {
                *led = Rgb::BLACK;
            }
        }
        for h in &mut self.heat {
            *h = h.saturating_sub(self.decay);
        }
    }

    fn on_key_event(&mut self, event: KeyEvent, _led_count: usize, _buffer: &mut [Rgb]) {
        if event.pressed {
            let idx = event.pos.flat(16);
            if let Some(h) = self.heat.get_mut(idx) {
                *h = h.saturating_add(self.heat_step);
            }
        }
    }
}

/// Max simultaneous ripples in flight (a cosmetic cap, not a correctness one —
/// pressing a 5th key while 4 ripples are still expanding just reuses the
/// oldest slot instead of starting a 5th ring).
const MAX_RIPPLES: usize = 4;

/// Integer square root (floored), via Newton's method — no floating point,
/// so this stays usable on `no_std` targets without `libm`.
fn isqrt(n: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[derive(Debug, Clone, Copy)]
struct RippleState {
    origin_row: u8,
    origin_col: u8,
    age_ticks: u16,
    /// Chosen when the ripple started, from the typing pace at that moment —
    /// see [`Ripple::hue_for_pace`].
    color: Rgb,
}

/// A ring of color expands outward from each pressed key and fades as it
/// travels, using each LED's real matrix row/col — unlike [`ReactivePress`]
/// and [`Heatmap`], which only ever check "is this the same key" and so can
/// get away with the fixed `pos.flat(16)` convention, a ripple needs genuine
/// relative geometry between *different* keys. That convention reshapes a
/// board's real layout into a 16-columns-wide strip, which would flatten
/// almost any board's rings into a horizontal smear. `positions[i]` gives
/// the board's real `(row, col)` for buffer slot `i`; board code should feed
/// this animation the *unremapped* [`KeyEvent`] (not passed through whatever
/// flat(16) remap it applies for other animations), since its origin here is
/// also real row/col, not a synthetic index.
///
/// Each ripple's color is picked from the typing pace at the moment it
/// started (fast presses run hot/red, slow presses run cool/blue), mirroring
/// [`WpmRainbow`]'s interval-EMA approach.
#[derive(Debug, Clone)]
pub struct Ripple<const LEDS: usize> {
    ripples: [Option<RippleState>; MAX_RIPPLES],
    positions: [(u8, u8); LEDS],
    /// Ticks between each one-cell expansion of the ring radius.
    pub ticks_per_step: u16,
    /// Ring radius (in matrix cells) at which a ripple fully fades out.
    pub max_radius: u16,
    /// Ring softness in matrix cells: how far a cell can sit from the ring's
    /// exact radius and still glow, tapering linearly to nothing at the
    /// edge. Higher = a thicker, more gradual ring instead of a hard,
    /// one-cell-wide edge.
    pub thickness: u16,
    last_press_ms: Option<u64>,
    interval_ema_ms: u32,
    has_interval: bool,
}

impl<const LEDS: usize> Ripple<LEDS> {
    /// Interval at/below which a ripple is colored fully "hot" (hue 0).
    const FAST_INTERVAL_MS: u32 = 120;
    /// Interval at/above which a ripple is colored fully "cool" (hue 170).
    const SLOW_INTERVAL_MS: u32 = 600;
    /// Bit-shift EMA smoothing factor (mirrors [`crate::matrix::analog::Ema::update`]).
    const EMA_SHIFT: u8 = 2;
    /// Sane ceiling for a single inter-press interval.
    const MAX_INTERVAL_MS: u32 = 60_000;

    /// Creates a new ripple animation over a board's real per-LED
    /// `(row, col)` table. `ticks_per_step` controls how fast the ring
    /// expands, `max_radius` how far it travels before fading out, and
    /// `thickness` how soft/gradual the ring's edge is.
    #[must_use]
    pub const fn new(
        positions: [(u8, u8); LEDS],
        ticks_per_step: u16,
        max_radius: u16,
        thickness: u16,
    ) -> Self {
        Self {
            ripples: [None; MAX_RIPPLES],
            positions,
            ticks_per_step,
            max_radius,
            thickness,
            last_press_ms: None,
            // No data yet; placeholder only — has_interval gates its use.
            interval_ema_ms: 300,
            has_interval: false,
        }
    }

    /// Hue for the current typing pace: hot (0) when fast, cool (170) when
    /// slow, linearly interpolated in between. Before any real interval has
    /// been observed, defaults to cool.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "scaled onto the 0..170 hue range, which fits u8 losslessly"
    )]
    fn hue_for_pace(&self) -> u8 {
        if !self.has_interval {
            return 170;
        }
        let interval = self
            .interval_ema_ms
            .clamp(Self::FAST_INTERVAL_MS, Self::SLOW_INTERVAL_MS);
        let span = Self::SLOW_INTERVAL_MS - Self::FAST_INTERVAL_MS;
        (((interval - Self::FAST_INTERVAL_MS) * 170) / span) as u8
    }
}

impl<const LEDS: usize> Animation for Ripple<LEDS> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "dr/dc are bounded by realistic board dimensions; their squared sum's isqrt fits u16 losslessly"
    )]
    fn tick(&mut self, _led_count: usize, _elapsed_ms: u32, buffer: &mut [Rgb]) {
        for led in &mut *buffer {
            *led = Rgb::BLACK;
        }
        let ticks_per_step = self.ticks_per_step.max(1);
        let max_radius = self.max_radius.max(1);
        let thickness = self.thickness.max(1);
        for slot in &mut self.ripples {
            let Some(r) = slot else { continue };
            let radius = r.age_ticks / ticks_per_step;
            if radius > max_radius {
                *slot = None;
                continue;
            }
            let radius_fade =
                255 - (u32::from(radius) * 255 / u32::from(max_radius)).min(255) as u8;
            for (i, led) in buffer.iter_mut().enumerate() {
                let Some(&(row, col)) = self.positions.get(i) else {
                    continue;
                };
                let dr = i32::from(row) - i32::from(r.origin_row);
                let dc = i32::from(col) - i32::from(r.origin_col);
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "dr*dr + dc*dc is always non-negative"
                )]
                let dist_sq = (dr * dr + dc * dc) as u32;
                let dist = isqrt(dist_sq) as u16;
                let delta = dist.abs_diff(radius);
                if delta > thickness {
                    continue;
                }
                let taper = 255 - (u32::from(delta) * 255 / u32::from(thickness)).min(255) as u8;
                let intensity = scale(radius_fade, taper);
                let ring = Rgb::new(
                    scale(r.color.r, intensity),
                    scale(r.color.g, intensity),
                    scale(r.color.b, intensity),
                );
                // Max-combine so overlapping ripples blend instead of one
                // overwriting the other.
                *led = Rgb::new(led.r.max(ring.r), led.g.max(ring.g), led.b.max(ring.b));
            }
            r.age_ticks = r.age_ticks.saturating_add(1);
        }
    }

    fn on_key_event(&mut self, event: KeyEvent, _led_count: usize, _buffer: &mut [Rgb]) {
        if event.pressed {
            if let Some(prev) = self.last_press_ms {
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_truncation,
                    reason = "interval clamped to MAX_INTERVAL_MS before narrowing to u32"
                )]
                let interval_ms = event
                    .timestamp_ms
                    .saturating_sub(prev)
                    .min(u64::from(Self::MAX_INTERVAL_MS)) as u32;

                // Mirror Ema::update's shift-based smoothing (matrix/analog.rs).
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_wrap,
                    reason = "interval_ema_ms ≤ MAX_INTERVAL_MS, fits i32 losslessly"
                )]
                let mut ema = self.interval_ema_ms as i32;
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_wrap,
                    reason = "interval_ms ≤ MAX_INTERVAL_MS, fits i32 losslessly"
                )]
                let sample = interval_ms as i32;
                ema += (sample - ema) >> Self::EMA_SHIFT;
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_wrap,
                    reason = "MAX_INTERVAL_MS fits i32 losslessly"
                )]
                let max_interval = Self::MAX_INTERVAL_MS as i32;
                ema = ema.clamp(0, max_interval);
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_sign_loss,
                    reason = "ema clamped to 0..=MAX_INTERVAL_MS above"
                )]
                let ema = ema as u32;
                self.interval_ema_ms = ema;
                self.has_interval = true;
            }
            self.last_press_ms = Some(event.timestamp_ms);

            let state = RippleState {
                origin_row: event.pos.row,
                origin_col: event.pos.col,
                age_ticks: 0,
                color: hsv_to_rgb(Hsv::new(self.hue_for_pace(), 255, 255)),
            };
            let slot = match self.ripples.iter_mut().find(|s| s.is_none()) {
                Some(free) => free,
                None => &mut self.ripples[0],
            };
            *slot = Some(state);
        }
    }
}

/// RGB control commands mapped to [`Action::Rgb`](crate::action::Action::Rgb).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RgbCommand {
    /// Toggle RGB on/off.
    Toggle,
    /// Switch to next animation.
    Next,
    /// Switch to previous animation.
    Prev,
    /// Increase master brightness.
    BrightUp,
    /// Decrease master brightness.
    BrightDown,
    /// Increase base hue.
    HueUp,
    /// Decrease base hue.
    HueDown,
    /// Increase base saturation.
    SatUp,
    /// Decrease base saturation.
    SatDown,
    /// Increase animation speed.
    SpeedUp,
    /// Decrease animation speed.
    SpeedDown,
}

const BRIGHTNESS_STEP: u8 = 17;
const HSV_STEP: u8 = 8;
const SPEED_STEP: u8 = 8;
const ANIMATION_COUNT: u8 = 7;

/// Manages LED state, brightness, and animation selection.
///
/// Board crates own this struct and call [`tick`](Self::tick) on a timer.
/// The current LED state is read via [`buffer`](Self::buffer) and sent
/// to hardware (I2C/SPI/WS2812).
pub struct RgbController<const LEDS: usize> {
    buffer: [Rgb; LEDS],
    enabled: bool,
    brightness: u8,
    current_animation: u8,
    base_color: Hsv,
    speed: u8,
}

impl<const LEDS: usize> RgbController<LEDS> {
    /// Creates a new controller with all LEDs off, full brightness, animation 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: [Rgb::BLACK; LEDS],
            enabled: true,
            brightness: 255,
            current_animation: 0,
            base_color: Hsv::new(0, 255, 255),
            speed: 128,
        }
    }

    /// Advances the animation by one frame and applies master brightness.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "animation time deliberately wraps at u32::MAX milliseconds"
    )]
    pub fn tick<A: Animation>(&mut self, elapsed_ms: u32, animation: &mut A) {
        if !self.enabled {
            self.buffer = [Rgb::BLACK; LEDS];
            return;
        }
        animation.set_base_color(self.base_color);
        // `speed` is a master rate multiplier: 128 is 1×, 0 freezes, 255 is ~2×.
        // Widen before scaling: a u32 product saturates after only a few hours.
        let scaled_ms = (u64::from(elapsed_ms) * u64::from(self.speed)).wrapping_div(128) as u32;
        animation.tick(LEDS, scaled_ms, &mut self.buffer);
        if self.brightness < 255 {
            for led in &mut self.buffer {
                led.r = scale(led.r, self.brightness);
                led.g = scale(led.g, self.brightness);
                led.b = scale(led.b, self.brightness);
            }
        }
    }

    /// Forwards a key event to the current animation for reactive effects.
    pub fn on_key_event<A: Animation>(&mut self, event: KeyEvent, animation: &mut A) {
        if self.enabled {
            animation.on_key_event(event, LEDS, &mut self.buffer);
        }
    }

    /// Returns the current LED color buffer.
    #[must_use]
    pub fn buffer(&self) -> &[Rgb] {
        &self.buffer
    }

    /// Returns the rendered frame for board-specific output limiting.
    ///
    /// Apply limits after `tick`; the next animation frame replaces these edits.
    pub fn buffer_mut(&mut self) -> &mut [Rgb] {
        &mut self.buffer
    }

    /// Returns whether RGB output is enabled.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables RGB output.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.buffer = [Rgb::BLACK; LEDS];
        }
    }

    /// Returns the current master brightness (0–255).
    #[must_use]
    pub fn brightness(&self) -> u8 {
        self.brightness
    }

    /// Sets the master brightness (0–255).
    pub fn set_brightness(&mut self, brightness: u8) {
        self.brightness = brightness;
    }

    /// Returns the index of the currently selected animation.
    #[must_use]
    pub fn current_animation(&self) -> u8 {
        self.current_animation
    }

    /// Advances to the next animation, wrapping around.
    #[expect(
        clippy::integer_division_remainder_used,
        reason = "wrapping animation index"
    )]
    pub fn next_animation(&mut self) {
        self.current_animation = (self.current_animation + 1) % ANIMATION_COUNT;
    }

    /// Goes to the previous animation, wrapping around.
    pub fn prev_animation(&mut self) {
        if self.current_animation == 0 {
            self.current_animation = ANIMATION_COUNT - 1;
        } else {
            self.current_animation -= 1;
        }
    }

    /// Returns the base HSV color.
    ///
    /// This is pushed into the animation on every [`tick`](Self::tick), so for
    /// colour-bearing animations it — not the colour the animation was built
    /// with — is what reaches the LEDs.
    #[must_use]
    pub fn base_color(&self) -> Hsv {
        self.base_color
    }

    /// Sets the base HSV color applied to colour-bearing animations.
    pub fn set_base_color(&mut self, hsv: Hsv) {
        self.base_color = hsv;
    }

    /// Returns the animation speed. 128 is 1×; 0 freezes; 255 is ~2×.
    #[must_use]
    pub fn speed(&self) -> u8 {
        self.speed
    }

    /// Sets the master animation rate multiplier.
    pub fn set_speed(&mut self, speed: u8) {
        self.speed = speed;
    }

    /// Applies an [`RgbCommand`] to this controller.
    pub fn handle_command(&mut self, cmd: RgbCommand) {
        match cmd {
            RgbCommand::Toggle => self.set_enabled(!self.enabled),
            RgbCommand::Next => self.next_animation(),
            RgbCommand::Prev => self.prev_animation(),
            RgbCommand::BrightUp => {
                self.brightness = self.brightness.saturating_add(BRIGHTNESS_STEP);
            }
            RgbCommand::BrightDown => {
                self.brightness = self.brightness.saturating_sub(BRIGHTNESS_STEP);
            }
            RgbCommand::HueUp => {
                self.base_color.h = self.base_color.h.wrapping_add(HSV_STEP);
            }
            RgbCommand::HueDown => {
                self.base_color.h = self.base_color.h.wrapping_sub(HSV_STEP);
            }
            RgbCommand::SatUp => {
                self.base_color.s = self.base_color.s.saturating_add(HSV_STEP);
            }
            RgbCommand::SatDown => {
                self.base_color.s = self.base_color.s.saturating_sub(HSV_STEP);
            }
            RgbCommand::SpeedUp => {
                self.speed = self.speed.saturating_add(SPEED_STEP);
            }
            RgbCommand::SpeedDown => {
                self.speed = self.speed.saturating_sub(SPEED_STEP);
            }
        }
    }
}

impl<const LEDS: usize> Default for RgbController<LEDS> {
    fn default() -> Self {
        Self::new()
    }
}

/// Async LED hardware driver — abstracts over WS2812, IS31FL3733, etc.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait LedDriver {
    /// Driver error type.
    type Error;
    /// Writes the LED color buffer to hardware.
    async fn write(&mut self, leds: &[Rgb]) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::event::KeyPos;

    #[test]
    fn hsv_pure_red() {
        let rgb = hsv_to_rgb(Hsv::new(0, 255, 255));
        assert_eq!(rgb, Rgb::new(255, 0, 0));
    }

    #[test]
    fn hsv_pure_green() {
        let rgb = hsv_to_rgb(Hsv::new(86, 255, 255));
        assert_eq!(rgb.r, 0);
        assert_eq!(rgb.b, 0);
        assert_eq!(rgb.g, 255);
    }

    #[test]
    fn hsv_pure_blue() {
        let rgb = hsv_to_rgb(Hsv::new(172, 255, 255));
        assert_eq!(rgb.r, 0);
        assert_eq!(rgb.g, 0);
        assert_eq!(rgb.b, 255);
    }

    #[test]
    fn hsv_white() {
        let rgb = hsv_to_rgb(Hsv::new(0, 0, 255));
        assert_eq!(rgb, Rgb::new(255, 255, 255));
    }

    #[test]
    fn hsv_black() {
        let rgb = hsv_to_rgb(Hsv::new(0, 0, 0));
        assert_eq!(rgb, Rgb::BLACK);
    }

    #[test]
    fn hsv_zero_saturation_is_greyscale() {
        let rgb = hsv_to_rgb(Hsv::new(123, 0, 128));
        assert_eq!(rgb, Rgb::new(128, 128, 128));
    }

    #[test]
    fn solid_color_fills_buffer() {
        let mut anim = SolidColor { color: Rgb::RED };
        let mut buf = [Rgb::BLACK; 4];
        anim.tick(4, 0, &mut buf);
        assert!(buf.iter().all(|&c| c == Rgb::RED));
    }

    #[test]
    fn rainbow_cycle_changes_on_tick() {
        let mut anim = RainbowCycle {
            speed: 10,
            offset: 0,
        };
        let mut buf1 = [Rgb::BLACK; 4];
        let mut buf2 = [Rgb::BLACK; 4];
        anim.tick(4, 0, &mut buf1);
        anim.tick(4, 1000, &mut buf2);
        assert_ne!(buf1, buf2);
    }

    #[test]
    fn rainbow_cycle_varies_across_leds() {
        let mut anim = RainbowCycle {
            speed: 1,
            offset: 0,
        };
        let mut buf = [Rgb::BLACK; 4];
        anim.tick(4, 0, &mut buf);
        assert_ne!(buf[0], buf[2]);
    }

    #[test]
    fn rainbow_cycle_single_led() {
        let mut anim = RainbowCycle {
            speed: 1,
            offset: 0,
        };
        let mut buf = [Rgb::BLACK; 1];
        anim.tick(1, 0, &mut buf);
        assert_ne!(buf[0], Rgb::BLACK);
    }

    #[test]
    fn breathing_changes_over_time() {
        let mut anim = Breathing {
            color: Rgb::WHITE,
            speed: 10,
            phase: 0,
        };
        let mut buf1 = [Rgb::BLACK; 2];
        let mut buf2 = [Rgb::BLACK; 2];
        anim.tick(2, 0, &mut buf1);
        anim.tick(2, 5000, &mut buf2);
        assert_ne!(buf1, buf2);
    }

    #[test]
    fn breathing_all_leds_same() {
        let mut anim = Breathing {
            color: Rgb::RED,
            speed: 5,
            phase: 0,
        };
        let mut buf = [Rgb::BLACK; 4];
        anim.tick(4, 500, &mut buf);
        assert!(buf.windows(2).all(|w| {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else {
                return true;
            };
            a == b
        }));
    }

    #[test]
    fn reactive_press_lights_on_event() {
        let mut anim = ReactivePress::new(Rgb::WHITE, 1);
        let mut buf = [Rgb::BLACK; 4];
        let event = KeyEvent::press(KeyPos::new(0, 1), 0);
        anim.on_key_event(event, 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        assert_ne!(buf[1], Rgb::BLACK);
    }

    #[test]
    fn reactive_press_decays_on_tick() {
        let mut anim = ReactivePress::new(Rgb::WHITE, 50);
        let mut buf = [Rgb::BLACK; 4];
        let event = KeyEvent::press(KeyPos::new(0, 0), 0);
        anim.on_key_event(event, 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        let first = buf[0];
        anim.tick(4, 10, &mut buf);
        let second = buf[0];
        assert!(second.r < first.r || second.g < first.g || second.b < first.b);
    }

    #[test]
    fn reactive_press_release_does_not_light() {
        let mut anim = ReactivePress::new(Rgb::WHITE, 1);
        let mut buf = [Rgb::BLACK; 4];
        let event = KeyEvent::release(KeyPos::new(0, 0), 0);
        anim.on_key_event(event, 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        assert_eq!(buf[0], Rgb::BLACK);
    }

    #[test]
    fn controller_enable_disable() {
        let mut ctrl = RgbController::<4>::new();
        assert!(ctrl.enabled());
        ctrl.set_enabled(false);
        assert!(!ctrl.enabled());
        assert!(ctrl.buffer().iter().all(|&c| c == Rgb::BLACK));
    }

    #[test]
    fn controller_tick_disabled_clears_buffer() {
        let mut ctrl = RgbController::<4>::new();
        let mut anim = SolidColor { color: Rgb::RED };
        ctrl.tick(0, &mut anim);
        assert!(ctrl.buffer().iter().all(|&c| c == Rgb::RED));
        ctrl.set_enabled(false);
        ctrl.tick(10, &mut anim);
        assert!(ctrl.buffer().iter().all(|&c| c == Rgb::BLACK));
    }

    #[test]
    fn controller_brightness() {
        let mut ctrl = RgbController::<4>::new();
        ctrl.set_brightness(128);
        assert_eq!(ctrl.brightness(), 128);
        let mut anim = SolidColor { color: Rgb::WHITE };
        ctrl.tick(0, &mut anim);
        // The controller pushes its base color (default hue 0: red) into the
        // animation each tick, so this is full red at half brightness.
        for led in ctrl.buffer() {
            assert_eq!(*led, Rgb::new(128, 0, 0));
        }
    }

    #[test]
    fn controller_full_brightness_no_dimming() {
        let mut ctrl = RgbController::<2>::new();
        ctrl.set_brightness(255);
        let hsv = Hsv::new(20, 200, 255);
        ctrl.set_base_color(hsv);
        let mut anim = SolidColor { color: Rgb::BLACK };
        ctrl.tick(0, &mut anim);
        assert_eq!(ctrl.buffer().first(), Some(&hsv_to_rgb(hsv)));
    }

    #[test]
    fn controller_next_animation_cycles() {
        let mut ctrl = RgbController::<1>::new();
        assert_eq!(ctrl.current_animation(), 0);
        for i in 1..ANIMATION_COUNT {
            ctrl.next_animation();
            assert_eq!(ctrl.current_animation(), i);
        }
        ctrl.next_animation();
        assert_eq!(ctrl.current_animation(), 0);
    }

    #[test]
    fn controller_prev_animation_wraps() {
        let mut ctrl = RgbController::<1>::new();
        ctrl.prev_animation();
        assert_eq!(ctrl.current_animation(), ANIMATION_COUNT - 1);
    }

    #[test]
    fn controller_handle_toggle() {
        let mut ctrl = RgbController::<1>::new();
        ctrl.handle_command(RgbCommand::Toggle);
        assert!(!ctrl.enabled());
        ctrl.handle_command(RgbCommand::Toggle);
        assert!(ctrl.enabled());
    }

    #[test]
    fn controller_handle_bright_up_saturates() {
        let mut ctrl = RgbController::<1>::new();
        ctrl.handle_command(RgbCommand::BrightUp);
        assert_eq!(ctrl.brightness(), 255);
    }

    #[test]
    fn hue_command_changes_the_pixels_a_solid_animation_emits() {
        let mut ctrl = RgbController::<1>::new();
        let mut anim = SolidColor { color: Rgb::BLACK };
        ctrl.tick(0, &mut anim);
        let before = ctrl.buffer()[0];
        for _ in 0..8 {
            ctrl.handle_command(RgbCommand::HueUp);
        }
        ctrl.tick(0, &mut anim);
        assert_ne!(ctrl.buffer()[0], before, "HueUp never reached the LEDs");
    }

    #[test]
    fn speed_command_changes_how_far_an_animation_advances() {
        // Breathing is a pure function of accumulated (elapsed × speed) time,
        // so a slower master speed must land on a different phase.
        let mut fast = RgbController::<1>::new();
        let mut slow = RgbController::<1>::new();
        for _ in 0..8 {
            slow.handle_command(RgbCommand::SpeedDown);
        }
        let mk = || Breathing {
            color: Rgb::new(255, 255, 255),
            speed: 16,
            phase: 0,
        };
        let (mut a, mut b) = (mk(), mk());
        fast.tick(100, &mut a);
        slow.tick(100, &mut b);
        assert_ne!(
            fast.buffer()[0],
            slow.buffer()[0],
            "SpeedDown never reached the animation"
        );
    }

    #[test]
    fn controller_speed_scaling_keeps_advancing_after_long_uptimes() {
        #[derive(Default)]
        struct Clock(u32);
        impl Animation for Clock {
            fn tick(&mut self, _: usize, elapsed_ms: u32, _: &mut [Rgb]) {
                self.0 = elapsed_ms;
            }
        }

        let mut ctrl = RgbController::<0>::new();
        let mut clock = Clock::default();
        for (speed, elapsed_ms, expected) in [
            (0, u32::MAX, 0),
            (64, 60_000_000, 30_000_000),
            (128, 60_000_000, 60_000_000),
            (128, u32::MAX, u32::MAX),
            (255, u32::MAX, 4_261_412_862),
        ] {
            ctrl.set_speed(speed);
            ctrl.tick(elapsed_ms, &mut clock);
            assert_eq!(clock.0, expected);
        }
    }

    #[test]
    fn controller_handle_bright_down() {
        let mut ctrl = RgbController::<1>::new();
        ctrl.handle_command(RgbCommand::BrightDown);
        assert_eq!(ctrl.brightness(), 255 - BRIGHTNESS_STEP);
    }

    #[test]
    fn controller_bright_down_saturates_at_zero() {
        let mut ctrl = RgbController::<1>::new();
        ctrl.set_brightness(5);
        ctrl.handle_command(RgbCommand::BrightDown);
        assert_eq!(ctrl.brightness(), 0);
    }

    #[test]
    fn controller_handle_next_prev() {
        let mut ctrl = RgbController::<1>::new();
        ctrl.handle_command(RgbCommand::Next);
        assert_eq!(ctrl.current_animation(), 1);
        ctrl.handle_command(RgbCommand::Prev);
        assert_eq!(ctrl.current_animation(), 0);
    }

    #[test]
    fn controller_default_matches_new() {
        let a = RgbController::<4>::new();
        let b = RgbController::<4>::default();
        assert_eq!(a.buffer(), b.buffer());
        assert_eq!(a.enabled(), b.enabled());
        assert_eq!(a.brightness(), b.brightness());
        assert_eq!(a.current_animation(), b.current_animation());
        assert_eq!(a.base_color(), b.base_color());
        assert_eq!(a.speed(), b.speed());
    }

    #[test]
    fn controller_on_key_event_forwards_when_enabled() {
        let mut ctrl = RgbController::<4>::new();
        let mut anim = ReactivePress::new(Rgb::WHITE, 1);
        let event = KeyEvent::press(KeyPos::new(0, 0), 0);
        ctrl.on_key_event(event, &mut anim);
        assert_eq!(anim.brightness[0], 255);
    }

    #[test]
    fn controller_on_key_event_ignored_when_disabled() {
        let mut ctrl = RgbController::<4>::new();
        ctrl.set_enabled(false);
        let mut anim = ReactivePress::new(Rgb::WHITE, 1);
        let event = KeyEvent::press(KeyPos::new(0, 0), 0);
        ctrl.on_key_event(event, &mut anim);
        assert_eq!(anim.brightness[0], 0);
    }

    #[test]
    fn rgb_color_constants() {
        assert_eq!(Rgb::BLACK, Rgb::new(0, 0, 0));
        assert_eq!(Rgb::WHITE, Rgb::new(255, 255, 255));
        assert_eq!(Rgb::RED, Rgb::new(255, 0, 0));
        assert_eq!(Rgb::GREEN, Rgb::new(0, 255, 0));
        assert_eq!(Rgb::BLUE, Rgb::new(0, 0, 255));
    }

    #[test]
    fn reactive_press_out_of_bounds_key_does_not_panic() {
        let mut anim = ReactivePress::new(Rgb::WHITE, 1);
        let mut buf = [Rgb::BLACK; 4];
        let event = KeyEvent::press(KeyPos::new(15, 15), 0);
        anim.on_key_event(event, 4, &mut buf);
        anim.tick(4, 0, &mut buf);
    }

    #[test]
    fn breathing_at_zero_speed_is_stable() {
        let mut anim = Breathing {
            color: Rgb::WHITE,
            speed: 0,
            phase: 0,
        };
        let mut buf1 = [Rgb::BLACK; 2];
        let mut buf2 = [Rgb::BLACK; 2];
        anim.tick(2, 0, &mut buf1);
        anim.tick(2, 1000, &mut buf2);
        assert_eq!(buf1, buf2);
    }

    #[test]
    fn controller_zero_brightness_produces_black() {
        let mut ctrl = RgbController::<2>::new();
        ctrl.set_brightness(0);
        let mut anim = SolidColor { color: Rgb::WHITE };
        ctrl.tick(0, &mut anim);
        assert!(ctrl.buffer().iter().all(|&c| c == Rgb::BLACK));
    }

    /// Sum of a color's channels — a proxy for rendered brightness.
    fn brightness_sum(c: Rgb) -> u16 {
        u16::from(c.r) + u16::from(c.g) + u16::from(c.b)
    }

    #[test]
    fn heatmap_press_increases_brightness() {
        let mut anim = Heatmap::new(40, 2);
        let mut buf = [Rgb::BLACK; 4];
        anim.tick(4, 0, &mut buf);
        assert_eq!(buf[1], Rgb::BLACK);
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 1), 0), 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        assert_eq!(buf[1], Rgb::new(0, 25, 40));
    }

    #[test]
    fn heatmap_lingers_across_ticks() {
        let mut anim = Heatmap::new(200, 2);
        let mut buf = [Rgb::BLACK; 4];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        // A single press must stay lit for several ticks — accumulation, not
        // flash-and-fade.
        for _ in 0..5 {
            anim.tick(4, 0, &mut buf);
            assert!(brightness_sum(buf[0]) > 0);
        }
    }

    #[test]
    fn heatmap_repeated_presses_accumulate() {
        let mut anim = Heatmap::new(40, 2);
        let mut buf = [Rgb::BLACK; 4];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        let single = anim.heat[0];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        assert!(anim.heat[0] > single);
    }

    #[test]
    fn heatmap_untouched_key_stays_black() {
        let mut anim = Heatmap::new(40, 2);
        let mut buf = [Rgb::BLACK; 4];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        assert_eq!(buf[2], Rgb::BLACK);
        assert_eq!(anim.heat[2], 0);
    }

    const RIPPLE_TEST_POSITIONS: [(u8, u8); 4] = [(0, 0), (0, 1), (0, 2), (0, 3)];

    #[test]
    fn ripple_press_lights_origin_immediately() {
        let mut anim = Ripple::new(RIPPLE_TEST_POSITIONS, 1, 3, 1);
        let mut buf = [Rgb::BLACK; 4];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        assert_ne!(buf[0], Rgb::BLACK);
        assert_eq!(buf[1], Rgb::BLACK);
    }

    #[test]
    fn ripple_expands_outward_over_ticks() {
        let mut anim = Ripple::new(RIPPLE_TEST_POSITIONS, 1, 3, 1);
        let mut buf = [Rgb::BLACK; 4];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        anim.tick(4, 0, &mut buf); // radius 0: lights index 0
        anim.tick(4, 0, &mut buf); // radius 1: lights index 1, not 0
        assert_eq!(buf[0], Rgb::BLACK);
        assert_ne!(buf[1], Rgb::BLACK);
    }

    #[test]
    fn ripple_fades_out_past_max_radius() {
        let mut anim = Ripple::new(RIPPLE_TEST_POSITIONS, 1, 1, 1);
        let mut buf = [Rgb::BLACK; 4];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        anim.tick(4, 0, &mut buf); // radius 0
        anim.tick(4, 0, &mut buf); // radius 1 (== max_radius, still visible)
        anim.tick(4, 0, &mut buf); // radius 2 > max_radius: ripple cleared
        assert!(buf.iter().all(|&c| c == Rgb::BLACK));
    }

    #[test]
    fn ripple_release_does_not_start_a_ripple() {
        let mut anim = Ripple::new(RIPPLE_TEST_POSITIONS, 1, 3, 1);
        let mut buf = [Rgb::BLACK; 4];
        anim.on_key_event(KeyEvent::release(KeyPos::new(0, 0), 0), 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        assert!(buf.iter().all(|&c| c == Rgb::BLACK));
    }

    #[test]
    fn ripple_beyond_max_ripples_does_not_panic() {
        let mut anim = Ripple::new(RIPPLE_TEST_POSITIONS, 1, 3, 1);
        let mut buf = [Rgb::BLACK; 4];
        for col in 0..5u8 {
            anim.on_key_event(KeyEvent::press(KeyPos::new(0, col), 0), 4, &mut buf);
        }
        anim.tick(4, 0, &mut buf);
    }

    #[test]
    fn ripple_expands_in_two_dimensions() {
        // A 2x2 grid: origin at (0,0). Euclidean distance (via isqrt) floors
        // to 1 for all three neighbors, including the diagonal one — proving
        // this isn't flattened onto a single row like the old flat(16) hack.
        let positions: [(u8, u8); 4] = [(0, 0), (0, 1), (1, 0), (1, 1)];
        let mut anim = Ripple::new(positions, 1, 3, 1);
        let mut buf = [Rgb::BLACK; 4];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut buf);
        anim.tick(4, 0, &mut buf); // radius 0: only origin (0,0)
        anim.tick(4, 0, &mut buf); // radius 1: (0,1), (1,0), (1,1) all match
        assert_eq!(buf[0], Rgb::BLACK);
        assert_ne!(buf[1], Rgb::BLACK);
        assert_ne!(buf[2], Rgb::BLACK);
        assert_ne!(buf[3], Rgb::BLACK);
    }

    #[test]
    fn isqrt_matches_known_squares() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(8), 2); // floors: 2^2=4 <= 8 < 9=3^2
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(18), 4); // floors: 4^2=16 <= 18 < 25=5^2
        assert_eq!(isqrt(u32::from(u16::MAX)), 255);
    }

    #[test]
    fn ripple_diverges_from_chebyshev_at_larger_radii() {
        // At (3,3), Chebyshev distance is 3 but true Euclidean distance is
        // sqrt(18)≈4.24, floor 4 — proving the ring now follows real
        // circular geometry rather than the old square/diamond metric.
        let positions: [(u8, u8); 2] = [(0, 0), (3, 3)];
        let mut anim = Ripple::new(positions, 1, 5, 1);
        let mut buf = [Rgb::BLACK; 2];
        anim.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 2, &mut buf);
        for _ in 0..=3 {
            anim.tick(2, 0, &mut buf); // radii 0..3: Chebyshev would light here
            assert_eq!(buf[1], Rgb::BLACK, "must not light at the Chebyshev radius");
        }
        anim.tick(2, 0, &mut buf); // radius 4: true Euclidean floor(sqrt(18))
        assert_ne!(buf[1], Rgb::BLACK);
    }

    #[test]
    fn ripple_thickness_softens_the_ring_edge() {
        // With thickness 1 (a hard ring), the cell one unit past the exact
        // radius is fully dark. With thickness 2, that same cell should
        // glow, dimmer than the cell exactly on the ring.
        let mut hard = Ripple::new(RIPPLE_TEST_POSITIONS, 1, 3, 1);
        let mut hard_buf = [Rgb::BLACK; 4];
        hard.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut hard_buf);
        hard.tick(4, 0, &mut hard_buf); // radius 0
        assert_eq!(hard_buf[1], Rgb::BLACK);

        let mut soft = Ripple::new(RIPPLE_TEST_POSITIONS, 1, 3, 2);
        let mut soft_buf = [Rgb::BLACK; 4];
        soft.on_key_event(KeyEvent::press(KeyPos::new(0, 0), 0), 4, &mut soft_buf);
        soft.tick(4, 0, &mut soft_buf); // radius 0
        assert_ne!(soft_buf[1], Rgb::BLACK);
        assert!(brightness_sum(soft_buf[1]) < brightness_sum(soft_buf[0]));
    }

    #[test]
    fn ripple_color_reflects_typing_pace() {
        let positions: [(u8, u8); 1] = [(0, 0)];

        let mut fast = Ripple::new(positions, 1, 3, 1);
        fast.on_key_event(
            KeyEvent::press(KeyPos::new(0, 0), 0),
            1,
            &mut [Rgb::BLACK; 1],
        );
        fast.on_key_event(
            KeyEvent::press(KeyPos::new(0, 0), 50),
            1,
            &mut [Rgb::BLACK; 1],
        );
        let fast_color = fast.ripples[1].unwrap().color;

        let mut slow = Ripple::new(positions, 1, 3, 1);
        slow.on_key_event(
            KeyEvent::press(KeyPos::new(0, 0), 0),
            1,
            &mut [Rgb::BLACK; 1],
        );
        slow.on_key_event(
            KeyEvent::press(KeyPos::new(0, 0), 2000),
            1,
            &mut [Rgb::BLACK; 1],
        );
        let slow_color = slow.ripples[1].unwrap().color;

        assert!(
            fast_color.r > slow_color.r,
            "fast presses should run hotter/redder"
        );
        assert!(
            slow_color.b > fast_color.b,
            "slow presses should run cooler/bluer"
        );
    }

    #[test]
    fn wpm_rainbow_fast_presses_increase_speed() {
        let mut anim = WpmRainbow::new();
        let mut buf = [Rgb::BLACK; 4];
        let pos = KeyPos::new(0, 0);
        let mut t: u64 = 0;
        // Repeated fast presses (100ms apart) converge the EMA toward a high WPM.
        for _ in 0..30 {
            t += 100;
            anim.on_key_event(KeyEvent::press(pos, t), 4, &mut buf);
            anim.tick(4, 0, &mut buf);
        }
        assert!(anim.current_wpm() > 60);
        assert!(anim.inner.speed > WpmRainbow::IDLE_SPEED);
    }

    #[test]
    fn wpm_rainbow_idle_settles_to_ambient_floor() {
        let mut anim = WpmRainbow::new();
        let mut buf = [Rgb::BLACK; 4];
        let pos = KeyPos::new(0, 0);
        anim.on_key_event(KeyEvent::press(pos, 0), 4, &mut buf);
        anim.on_key_event(KeyEvent::press(pos, 100), 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        assert!(anim.inner.speed > WpmRainbow::IDLE_SPEED);

        for _ in 0..=WpmRainbow::IDLE_TICKS {
            anim.tick(4, 0, &mut buf);
        }
        assert_eq!(anim.inner.speed, WpmRainbow::IDLE_SPEED);
    }

    #[test]
    fn wpm_rainbow_first_keystroke_does_not_peg_full_speed() {
        let mut anim = WpmRainbow::new();
        let mut buf = [Rgb::BLACK; 4];
        let pos = KeyPos::new(0, 0);
        anim.on_key_event(KeyEvent::press(pos, 0), 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        assert_eq!(anim.current_wpm(), 40);
        assert_eq!(anim.max_wpm(), 40);
        assert_eq!(anim.inner.speed, WpmRainbow::IDLE_SPEED);
    }

    #[test]
    fn wpm_rainbow_first_press_has_no_prior_interval() {
        let mut anim = WpmRainbow::new();
        let mut buf = [Rgb::BLACK; 4];
        let pos = KeyPos::new(0, 0);
        anim.on_key_event(KeyEvent::press(pos, 12_345), 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        // No prior press means no interval was folded in — still the seeded default.
        assert_eq!(anim.current_wpm(), 40);
    }

    #[test]
    fn wpm_rainbow_extremely_long_gap_clamps_interval() {
        let mut anim = WpmRainbow::new();
        let mut buf = [Rgb::BLACK; 4];
        let pos = KeyPos::new(0, 0);
        anim.on_key_event(KeyEvent::press(pos, 0), 4, &mut buf);
        // Gap far beyond MAX_INTERVAL_MS (60_000ms) — hours in ms.
        let huge_gap_ms: u64 = 10 * 60 * 60 * 1000;
        anim.on_key_event(KeyEvent::press(pos, huge_gap_ms), 4, &mut buf);
        anim.tick(4, 0, &mut buf);
        // Clamped to a sane low WPM, not wrapped/truncated into garbage.
        assert!(anim.current_wpm() <= 40);
    }

    #[test]
    fn wpm_rainbow_release_does_not_affect_state() {
        let mut anim = WpmRainbow::new();
        let mut buf = [Rgb::BLACK; 4];
        let pos = KeyPos::new(0, 0);
        let ema_before = anim.interval_ema_ms;
        anim.on_key_event(KeyEvent::release(pos, 12_345), 4, &mut buf);
        assert_eq!(anim.last_press_ms, None);
        assert_eq!(anim.interval_ema_ms, ema_before);
    }

    #[test]
    fn wpm_rainbow_max_wpm_ratchets_up_never_down() {
        let mut anim = WpmRainbow::new();
        let mut buf = [Rgb::BLACK; 4];
        let pos = KeyPos::new(0, 0);
        let mut t: u64 = 0;
        // Fast presses (50ms apart) push max_wpm_seen up.
        for _ in 0..20 {
            t += 50;
            anim.on_key_event(KeyEvent::press(pos, t), 4, &mut buf);
            anim.tick(4, 0, &mut buf);
        }
        let peak = anim.max_wpm();
        assert!(peak > 40);

        // Type much slower afterward — max_wpm_seen must not decrease.
        for _ in 0..20 {
            t += 2_000;
            anim.on_key_event(KeyEvent::press(pos, t), 4, &mut buf);
            anim.tick(4, 0, &mut buf);
        }
        assert_eq!(anim.max_wpm(), peak);
    }

    #[test]
    fn wpm_rainbow_speed_scales_relative_to_personal_best() {
        let mut anim = WpmRainbow::new();
        let mut buf = [Rgb::BLACK; 4];
        let pos = KeyPos::new(0, 0);
        let mut t: u64 = 0;
        // Establish a personal best at ~120 wpm (100ms interval).
        for _ in 0..30 {
            t += 100;
            anim.on_key_event(KeyEvent::press(pos, t), 4, &mut buf);
            anim.tick(4, 0, &mut buf);
        }
        let full_speed = anim.inner.speed;
        assert!(
            full_speed >= 230,
            "expected near-max speed at personal best, got {full_speed}"
        );

        // Settle into typing at roughly half that pace (200ms interval ~ 60wpm).
        for _ in 0..30 {
            t += 200;
            anim.on_key_event(KeyEvent::press(pos, t), 4, &mut buf);
            anim.tick(4, 0, &mut buf);
        }
        let half_speed = anim.inner.speed;
        // max_wpm_seen is unchanged (still the earlier personal best), so this
        // proves the scaling tracks the observed max, not a fixed reference WPM.
        assert!(half_speed < full_speed);
        assert!(
            (90..=160).contains(&half_speed),
            "expected roughly half of full speed, got {half_speed}"
        );
    }
}
