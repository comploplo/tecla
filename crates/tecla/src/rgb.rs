//! RGB animation engine — pure core logic, no hardware drivers.
//!
//! Provides color types, an [`Animation`] trait, four built-in animations,
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
const ANIMATION_COUNT: u8 = 4;

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
    pub fn tick<A: Animation>(&mut self, elapsed_ms: u32, animation: &mut A) {
        if !self.enabled {
            self.buffer = [Rgb::BLACK; LEDS];
            return;
        }
        animation.tick(LEDS, elapsed_ms, &mut self.buffer);
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
    #[must_use]
    pub fn base_color(&self) -> Hsv {
        self.base_color
    }

    /// Returns the animation speed.
    #[must_use]
    pub fn speed(&self) -> u8 {
        self.speed
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
        for led in ctrl.buffer() {
            assert!(led.r < 255);
            assert!(led.r > 0);
        }
    }

    #[test]
    fn controller_full_brightness_no_dimming() {
        let mut ctrl = RgbController::<2>::new();
        ctrl.set_brightness(255);
        let mut anim = SolidColor {
            color: Rgb::new(200, 100, 50),
        };
        ctrl.tick(0, &mut anim);
        assert_eq!(ctrl.buffer().first(), Some(&Rgb::new(200, 100, 50)));
    }

    #[test]
    fn controller_next_animation_cycles() {
        let mut ctrl = RgbController::<1>::new();
        assert_eq!(ctrl.current_animation(), 0);
        ctrl.next_animation();
        assert_eq!(ctrl.current_animation(), 1);
        ctrl.next_animation();
        ctrl.next_animation();
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
    fn rgb_default_is_black() {
        assert_eq!(Rgb::default(), Rgb::BLACK);
    }

    #[test]
    fn hsv_default_is_zero() {
        assert_eq!(Hsv::default(), Hsv::new(0, 0, 0));
    }

    #[test]
    fn rgb_command_variants_distinct() {
        assert_ne!(RgbCommand::Toggle, RgbCommand::Next);
        assert_ne!(RgbCommand::Next, RgbCommand::Prev);
        assert_ne!(RgbCommand::BrightUp, RgbCommand::BrightDown);
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
}
