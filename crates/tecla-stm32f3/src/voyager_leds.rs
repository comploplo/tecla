//! Combines the Voyager's two IS31FL3731 chips into one 52-LED
//! [`LedDriver`], plus the matrix-position → LED-index
//! table used to route reactive animations to the right physical key.
//!
//! `LED_CHANNELS` (which chip, and which of its 144 PWM registers, drives a
//! given LED's R/G/B) is taken from ZSA's own QMK source for this board —
//! `keyboards/zsa/voyager/voyager.c`'s `g_is31fl3731_leds` table, cross-
//! referenced by LED index against `keyboard.json`'s `rgb_matrix.layout`
//! matrix tags, with each `Cn_m` location resolved to a register offset via
//! `drivers/led/issi/is31fl3731-mono.h`'s `(n-1)*16 + (m-1)` addressing. Not a
//! guess.

use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::rgb::{LedDriver, Rgb};

use crate::is31fl3731::Is31fl3731;

/// Total per-key RGB LED count (26 per half).
pub const LEDS: usize = 52;

const ROWS: usize = 12;
const COLS: usize = 7;

/// LED index (0..52) for every populated matrix position, `None` for a
/// spacer slot with no physical key. Indices follow ZSA's physical LED order,
/// including N before M on the right bottom row despite its later matrix row.
#[rustfmt::skip]
pub const LED_INDEX: [[Option<u8>; COLS]; ROWS] = [
    // row 0
    [None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)],
    // row 1
    [None, Some(6), Some(7), Some(8), Some(9), Some(10), Some(11)],
    // row 2
    [None, Some(12), Some(13), Some(14), Some(15), Some(16), Some(17)],
    // row 3
    [None, Some(18), Some(19), Some(20), Some(21), Some(22), None],
    // row 4
    [None, None, None, None, Some(23), None, None],
    // row 5
    [Some(24), Some(25), None, None, None, None, None],
    // row 6
    [Some(26), Some(27), Some(28), Some(29), Some(30), Some(31), None],
    // row 7
    [Some(32), Some(33), Some(34), Some(35), Some(36), Some(37), None],
    // row 8
    [Some(38), Some(39), Some(40), Some(41), Some(42), Some(43), None],
    // row 9
    [None, Some(45), Some(46), Some(47), Some(48), Some(49), None],
    // row 10
    [None, None, Some(44), None, None, None, None],
    // row 11
    [None, None, None, None, None, Some(50), Some(51)],
];

// Every index 0..LEDS appears in LED_INDEX exactly once — catches a
// mistyped/duplicated/skipped index in the table above at compile time (see
// this crate's `voyager.rs` example for the same `const _: () = assert!`
// pattern used for other board-shape invariants).
const fn indices_are_a_permutation_of_0_to_leds() -> bool {
    let mut seen = [false; LEDS];
    let mut r = 0;
    while r < ROWS {
        let mut c = 0;
        while c < COLS {
            if let Some(idx) = LED_INDEX[r][c] {
                let idx = idx as usize;
                if idx >= LEDS || seen[idx] {
                    return false;
                }
                seen[idx] = true;
            }
            c += 1;
        }
        r += 1;
    }
    let mut i = 0;
    while i < LEDS {
        if !seen[i] {
            return false;
        }
        i += 1;
    }
    true
}

const _: () = assert!(
    indices_are_a_permutation_of_0_to_leds(),
    "LED_INDEX must assign every index 0..LEDS exactly once"
);

#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "ROWS (12) and COLS (7) fit u8 trivially"
)]
const fn invert_led_index() -> [(u8, u8); LEDS] {
    let mut positions = [(0u8, 0u8); LEDS];
    let mut r = 0;
    while r < ROWS {
        let mut c = 0;
        while c < COLS {
            if let Some(idx) = LED_INDEX[r][c] {
                positions[idx as usize] = (r as u8, c as u8);
            }
            c += 1;
        }
        r += 1;
    }
    positions
}

/// Real `(row, col)` per LED index — the inverse of [`LED_INDEX`], derived
/// from it (not hand-duplicated) so the two can never drift apart. Feeds
/// [`Ripple`](tecla::rgb::Ripple), which needs genuine board geometry rather
/// than the `pos.flat(16)` convention [`remap_for_rgb`] produces for
/// index-equality-only animations like `Heatmap`/`ReactivePress`.
pub const LED_POSITIONS: [(u8, u8); LEDS] = invert_led_index();

/// Per-LED hardware wiring: `(chip, r_reg, g_reg, b_reg)`, indexed by the same
/// LED index [`LED_INDEX`] assigns. `chip` is 0 for the left driver (`0x74`),
/// 1 for the right (`0x77`); the three registers are PWM-table offsets
/// (`0x00..0x90`) on that chip. See the module doc comment for provenance.
#[rustfmt::skip]
const LED_CHANNELS: [(u8, u8, u8, u8); LEDS] = [
    (0, 0x11, 0x01, 0x32), (0, 0x12, 0x02, 0x22), (0, 0x13, 0x03, 0x23),
    (0, 0x14, 0x04, 0x24), (0, 0x15, 0x05, 0x25), (0, 0x16, 0x06, 0x26),
    (0, 0x17, 0x07, 0x27), (0, 0x70, 0x60, 0x80), (0, 0x71, 0x61, 0x81),
    (0, 0x72, 0x62, 0x82), (0, 0x73, 0x63, 0x83), (0, 0x74, 0x64, 0x84),
    (0, 0x75, 0x65, 0x85), (0, 0x19, 0x09, 0x3A), (0, 0x1A, 0x0A, 0x2A),
    (0, 0x1B, 0x0B, 0x2B), (0, 0x1C, 0x0C, 0x2C), (0, 0x1D, 0x0D, 0x2D),
    (0, 0x1E, 0x0E, 0x2E), (0, 0x1F, 0x0F, 0x2F), (0, 0x78, 0x68, 0x88),
    (0, 0x79, 0x69, 0x89), (0, 0x7A, 0x6A, 0x8A), (0, 0x7B, 0x6B, 0x8B),
    (0, 0x7C, 0x6C, 0x8C), (0, 0x7D, 0x6D, 0x8D),
    (1, 0x16, 0x06, 0x26), (1, 0x15, 0x05, 0x25), (1, 0x14, 0x04, 0x24),
    (1, 0x13, 0x03, 0x23), (1, 0x12, 0x02, 0x22), (1, 0x11, 0x01, 0x32),
    (1, 0x74, 0x64, 0x84), (1, 0x73, 0x63, 0x83), (1, 0x72, 0x62, 0x82),
    (1, 0x71, 0x61, 0x81), (1, 0x70, 0x60, 0x80), (1, 0x17, 0x07, 0x27),
    (1, 0x1D, 0x0D, 0x2D), (1, 0x1C, 0x0C, 0x2C), (1, 0x1B, 0x0B, 0x2B),
    (1, 0x1A, 0x0A, 0x2A), (1, 0x19, 0x09, 0x3A), (1, 0x75, 0x65, 0x85),
    (1, 0x7B, 0x6B, 0x8B), (1, 0x7A, 0x6A, 0x8A), (1, 0x79, 0x69, 0x89),
    (1, 0x78, 0x68, 0x88), (1, 0x1F, 0x0F, 0x2F), (1, 0x1E, 0x0E, 0x2E),
    (1, 0x7D, 0x6D, 0x8D), (1, 0x7C, 0x6C, 0x8C),
];

/// Remaps a raw [`KeyEvent`] so its position's `.flat(16)` equals the key's
/// LED index — the fixed convention [`ReactivePress`](tecla::rgb::ReactivePress)
/// and [`Heatmap`](tecla::rgb::Heatmap) use to index their per-key buffers.
/// Positions with no LED (matrix spacers) map to an out-of-range sentinel so
/// they're silently ignored rather than aliasing onto a real LED's slot.
#[must_use]
pub fn remap_for_rgb(event: KeyEvent) -> KeyEvent {
    let led_idx = LED_INDEX
        .get(usize::from(event.pos.row))
        .and_then(|row| row.get(usize::from(event.pos.col)))
        .copied()
        .flatten();
    let mut remapped = event;
    remapped.pos = match led_idx {
        Some(idx) => KeyPos::new(idx / 16, idx % 16),
        None => KeyPos::new(255, 255),
    };
    remapped
}

/// PWM channels per IS31FL3731 — one brightness byte each.
const PWM_BYTES: usize = 144;

/// The Voyager's combined per-key RGB driver: two [`Is31fl3731`] chips (one
/// per half) presented as one 52-LED [`LedDriver`].
pub struct VoyagerLeds<I2C> {
    left: Is31fl3731<I2C>,
    right: Is31fl3731<I2C>,
    chips_ok: bool,
    /// Last frame actually sent, so an unchanged one can be skipped.
    ///
    /// Retain all 288 PWM values so unchanged chunks cost no shared-bus time.
    last_frame: Option<([u8; PWM_BYTES], [u8; PWM_BYTES])>,
}

impl<I2C> VoyagerLeds<I2C> {
    /// Combines an already-constructed left/right chip pair.
    ///
    /// The chips are not touched here — see [`write`](LedDriver::write), which
    /// initialises them lazily and re-initialises them if they go away.
    pub fn new(left: Is31fl3731<I2C>, right: Is31fl3731<I2C>) -> Self {
        Self {
            left,
            right,
            chips_ok: false,
            last_frame: None,
        }
    }
}

impl<I2C: embedded_hal_async::i2c::I2c> VoyagerLeds<I2C> {
    /// Powers up and initializes both chips.
    ///
    /// # Errors
    /// Returns the underlying I2C error if either chip fails to initialize.
    pub async fn init(&mut self) -> Result<(), I2C::Error> {
        self.chips_ok = false;
        // A chip that has just been (re)initialised is PWM-zeroed, so any
        // cached frame no longer describes what is on the LEDs.
        self.last_frame = None;
        self.left.init().await?;
        self.right.init().await?;
        self.chips_ok = true;
        Ok(())
    }
}

impl<I2C: embedded_hal_async::i2c::I2c> LedDriver for VoyagerLeds<I2C> {
    type Error = I2C::Error;

    /// Writes one frame, bringing the chips up first if they are not running.
    ///
    /// The right-hand chip loses power whenever the TRRS link is opened —
    /// unplugging the halves, or inserting a Navigator into the chain — and
    /// comes back in its power-on state: shut down, LED-enable bits clear, PWM
    /// zeroed. A one-shot `init()` at boot leaves it dark forever after that.
    /// So a failed write drops back to uninitialised and the next frame retries,
    /// which mirrors the lazy-init tolerance
    /// [`VoyagerScanner`](crate::scanner::VoyagerScanner) already has for the
    /// expander on the same cable.
    async fn write(&mut self, leds: &[Rgb]) -> Result<(), Self::Error> {
        if !self.chips_ok {
            self.init().await?;
        }
        let mut left_pwm = [0u8; PWM_BYTES];
        let mut right_pwm = [0u8; PWM_BYTES];
        for (i, &color) in leds.iter().enumerate().take(LEDS) {
            let (chip, r, g, b) = LED_CHANNELS[i];
            let buf = if chip == 0 {
                &mut left_pwm
            } else {
                &mut right_pwm
            };
            buf[usize::from(r)] = color.r;
            buf[usize::from(g)] = color.g;
            buf[usize::from(b)] = color.b;
        }
        if self.last_frame == Some((left_pwm, right_pwm)) {
            return Ok(());
        }

        let previous = self.last_frame.as_ref();
        if let Err(e) = self
            .left
            .write_frame_diff(&left_pwm, previous.map(|(left, _)| left))
            .await
        {
            self.chips_ok = false;
            self.last_frame = None;
            return Err(e);
        }
        if let Err(e) = self
            .right
            .write_frame_diff(&right_pwm, previous.map(|(_, right)| right))
            .await
        {
            self.chips_ok = false;
            // Left landed, right did not — cache nothing rather than a frame
            // only half of the hardware is showing.
            self.last_frame = None;
            return Err(e);
        }
        self.last_frame = Some((left_pwm, right_pwm));
        Ok(())
    }
}
