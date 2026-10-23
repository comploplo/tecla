//! NOS75 animations over the LED order recovered from its production image.
//!
//! Matrix and key coordinates: dump SHA256 4da91e79d3dda0ed0432c23f58e38ed1d5da196bde1f837a03b3db9d7e9eb222,
//! g_led_config at 0x0800DB00 (SHA256 671c98f423663fd8a9b4688b1be01d9faaf65b858cd97481a3a78f58d5240b0f).
//! Electrical RGB channels belong to the board's IS31FL3733 driver.

use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::rgb::{
    Animation, Breathing, Heatmap, Hsv, RainbowCycle, ReactivePress, Rgb, RgbCommand,
    RgbController, Ripple, SolidColor, WpmRainbow,
};

/// Production physical LED count, including eight side LEDs.
pub const LED_COUNT: usize = 89;
/// Number of built-in modes, in the core controller's order.
pub const MODE_COUNT: u8 = 7;
/// Sum of RGB channel values allowed per frame, assuming 20 mA per full-scale channel.
/// This models a 350 mA ceiling; actual current depends on unmeasured ISET/GCC.
pub const CHANNEL_BUDGET: u32 = 350 * 255 / 20;
/// A matrix position with no physical LED.
pub const NO_LED: u8 = 255;

/// Production matrix-to-LED table; values are driver frame indices, not keycodes.
pub const MATRIX_TO_LED: [[u8; 17]; 6] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 255, 255, 255, 255],
    [
        14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 255, 13, 43,
    ],
    [
        29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 28, 255, 57,
    ],
    [
        44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 255, 56, 255, 255, 255,
    ],
    [
        58, 255, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 255, 69, 70, 255, 255,
    ],
    [
        71, 72, 73, 255, 255, 255, 74, 255, 255, 75, 76, 255, 77, 78, 79, 80, 255,
    ],
];

/// Production physical (x, y) key coordinates. Stock side entries are (0, 0).
pub const LED_POSITIONS: [(u8, u8); LED_COUNT] = [
    (0, 0),
    (12, 0),
    (22, 0),
    (32, 0),
    (42, 0),
    (55, 0),
    (65, 0),
    (75, 0),
    (85, 0),
    (97, 0),
    (107, 0),
    (117, 0),
    (127, 0),
    (140, 0),
    (0, 10),
    (10, 10),
    (20, 10),
    (30, 10),
    (40, 10),
    (50, 10),
    (60, 10),
    (70, 10),
    (80, 10),
    (90, 10),
    (100, 10),
    (110, 10),
    (120, 10),
    (130, 10),
    (152, 10),
    (0, 20),
    (15, 20),
    (25, 20),
    (35, 20),
    (45, 20),
    (55, 20),
    (65, 20),
    (75, 20),
    (85, 20),
    (95, 20),
    (105, 20),
    (115, 20),
    (125, 20),
    (135, 20),
    (152, 20),
    (0, 30),
    (17, 30),
    (27, 30),
    (37, 30),
    (47, 30),
    (57, 30),
    (67, 30),
    (77, 30),
    (87, 30),
    (97, 30),
    (107, 30),
    (117, 30),
    (127, 30),
    (152, 30),
    (0, 40),
    (22, 40),
    (32, 40),
    (42, 40),
    (52, 40),
    (62, 40),
    (72, 40),
    (82, 40),
    (92, 40),
    (102, 40),
    (112, 40),
    (122, 40),
    (140, 40),
    (0, 50),
    (12, 50),
    (25, 50),
    (37, 50),
    (100, 50),
    (112, 50),
    (120, 50),
    (130, 50),
    (140, 50),
    (150, 50),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
];

/// Returns a physical frame index for a wired matrix position.
#[must_use]
pub fn led_index(pos: KeyPos) -> Option<usize> {
    MATRIX_TO_LED
        .get(usize::from(pos.row))
        .and_then(|row| row.get(usize::from(pos.col)))
        .copied()
        .filter(|&index| index != NO_LED)
        .map(usize::from)
}

const fn ripple_positions() -> [(u8, u8); LED_COUNT] {
    let mut positions = [(0, 0); LED_COUNT];
    let mut index = 0;
    while index < 81 {
        // Ripple uses (row, col); production geometry is stored as (x, y).
        positions[index] = (LED_POSITIONS[index].1, LED_POSITIONS[index].0);
        index += 1;
    }
    // Cosmetic mirrored sidebar geometry: stock supplies no side coordinates.
    // The pairings 84/85, 83/86, 82/87, 81/88 come from stock's side-pair table.
    positions[81] = (40, 0);
    positions[82] = (30, 0);
    positions[83] = (20, 0);
    positions[84] = (10, 0);
    positions[85] = (10, 152);
    positions[86] = (20, 152);
    positions[87] = (30, 152);
    positions[88] = (40, 152);
    positions
}

// Keep only the active effect: storing all seven wastes scarce F072 RAM.
enum Effect {
    Solid(SolidColor),
    Rainbow(RainbowCycle),
    Breathing(Breathing),
    Reactive(ReactivePress),
    Wpm(WpmRainbow),
    Heatmap(Heatmap),
    Ripple(Ripple<LED_COUNT>),
}

impl Effect {
    fn new(index: u8) -> Self {
        debug_assert!(index < MODE_COUNT);
        match index {
            0 => Self::Solid(SolidColor { color: Rgb::WHITE }),
            1 => Self::Rainbow(RainbowCycle {
                speed: 8,
                offset: 0,
            }),
            2 => Self::Breathing(Breathing {
                color: Rgb::WHITE,
                speed: 12,
                phase: 64,
            }),
            3 => Self::Reactive(ReactivePress::new(Rgb::WHITE, 8)),
            4 => Self::Wpm(WpmRainbow::new()),
            5 => Self::Heatmap(Heatmap::new(64, 2)),
            _ => Self::Ripple(Ripple::new(ripple_positions(), 1, 180, 8)),
        }
    }

    fn animation_mut(&mut self) -> &mut dyn Animation {
        match self {
            Self::Solid(animation) => animation,
            Self::Rainbow(animation) => animation,
            Self::Breathing(animation) => animation,
            Self::Reactive(animation) => animation,
            Self::Wpm(animation) => animation,
            Self::Heatmap(animation) => animation,
            Self::Ripple(animation) => animation,
        }
    }
}

impl Animation for Effect {
    fn tick(&mut self, count: usize, elapsed_ms: u32, buffer: &mut [Rgb]) {
        self.animation_mut().tick(count, elapsed_ms, buffer);
    }

    fn on_key_event(&mut self, event: KeyEvent, count: usize, buffer: &mut [Rgb]) {
        self.animation_mut().on_key_event(event, count, buffer);
    }

    fn set_base_color(&mut self, hsv: Hsv) {
        self.animation_mut().set_base_color(hsv);
    }
}

/// One active animation and one frame buffer; no heap or saved mode caches.
pub struct Lighting {
    controller: RgbController<LED_COUNT>,
    effect: Effect,
}

impl Lighting {
    /// Starts a slow rainbow with conservative brightness.
    #[must_use]
    pub fn new() -> Self {
        let mut controller = RgbController::new();
        controller.next_animation();
        controller.set_brightness(64);
        controller.set_speed(96);
        controller.set_base_color(Hsv::new(128, 220, 255));
        Self {
            controller,
            effect: Effect::new(1),
        }
    }

    /// Applies RGB controls; changing mode resets only that effect's history.
    pub fn command(&mut self, command: RgbCommand) {
        let previous = self.mode();
        self.controller.handle_command(command);
        let current = self.mode();
        if current != previous {
            self.effect = Effect::new(current);
        }
    }

    /// Routes a physical key event to the effect using its required geometry.
    pub fn on_key_event(&mut self, mut event: KeyEvent) {
        if !self.enabled() {
            return;
        }
        let Some(index) = led_index(event.pos) else {
            return;
        };
        if matches!(self.effect, Effect::Reactive(_) | Effect::Heatmap(_)) {
            // Those two core effects index state by pos.flat(16).
            event.pos = KeyPos::new(
                u8::try_from(index / 16).unwrap_or(0),
                u8::try_from(index % 16).unwrap_or(0),
            );
        } else if matches!(self.effect, Effect::Ripple(_)) {
            let (x, y) = LED_POSITIONS[index];
            event.pos = KeyPos::new(y, x);
        }
        self.controller.on_key_event(event, &mut self.effect);
    }

    /// Renders and power-limits a frame after master brightness is applied.
    pub fn tick(&mut self, elapsed_ms: u32) -> &[Rgb] {
        self.controller.tick(elapsed_ms, &mut self.effect);
        limit_current(self.controller.buffer_mut());
        self.controller.buffer()
    }

    /// Current mode: solid, rainbow, breathing, reactive, WPM, heatmap, ripple.
    #[must_use]
    pub fn mode(&self) -> u8 {
        self.controller.current_animation()
    }
    /// Whether output is enabled.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.controller.enabled()
    }
    /// Master brightness (0–255).
    #[cfg(test)]
    #[must_use]
    pub fn brightness(&self) -> u8 {
        self.controller.brightness()
    }
    /// Core master speed multiplier (128 is 1×).
    #[cfg(test)]
    #[must_use]
    pub fn speed(&self) -> u8 {
        self.controller.speed()
    }
    /// Base color for the solid, breathing and reactive effects.
    #[cfg(test)]
    #[must_use]
    pub fn base_color(&self) -> Hsv {
        self.controller.base_color()
    }
}

impl Default for Lighting {
    fn default() -> Self {
        Self::new()
    }
}

/// Scales a whole frame uniformly to the conservative channel-sum budget.
/// This is an assumed electrical budget, not a measured current calibration.
pub fn limit_current(buffer: &mut [Rgb]) {
    let total: u32 = buffer
        .iter()
        .map(|rgb| u32::from(rgb.r) + u32::from(rgb.g) + u32::from(rgb.b))
        .sum();
    if total <= CHANNEL_BUDGET {
        return;
    }
    for rgb in buffer {
        rgb.r = scale_budget(rgb.r, total);
        rgb.g = scale_budget(rgb.g, total);
        rgb.b = scale_budget(rgb.b, total);
    }
}

fn scale_budget(channel: u8, total: u32) -> u8 {
    u8::try_from(u32::from(channel) * CHANNEL_BUDGET / total).unwrap_or(0)
}
