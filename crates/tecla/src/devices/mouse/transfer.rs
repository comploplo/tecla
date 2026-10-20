//! Pointing transfer function — device counts to pointer movement.
//!
//! A high-resolution sensor produces far more counts per unit of travel than a
//! pointer should move one-to-one. This maps device speed to a gain, so the
//! sensor's resolution is spent where it is useful — fine control when moving
//! slowly — without the same resolution making fast movement unusable.
//!
//! # Fixed point
//!
//! Gain is in 1/[`GAIN_ONE`] units, so `256` is unity and `128` is half. All
//! arithmetic is integer; there is no FPU on the targets this runs on.
//!
//! # Sub-pixel accumulation
//!
//! Any gain below unity truncates: a slow movement producing 3 counts at a
//! gain of 0.375 is 1.125 pointer units, and integer division throws the 0.125
//! away every single poll. [`SubPixel`] keeps that remainder and carries it
//! forward, so slow movement accumulates instead of vanishing. Without it, a
//! sub-unity gain would give a dead zone at exactly the speeds the curve is
//! meant to make precise.
//!
//! The carry uses floor division rather than truncation toward zero, so the
//! residual stays in `0..GAIN_ONE` regardless of sign. Truncating division
//! would bias every negative axis toward zero and the pointer would drift.

/// Gain denominator: this value means unity gain.
pub const GAIN_ONE: u16 = 256;

/// A two-breakpoint speed-to-gain curve.
///
/// Below `slow_speed` the gain is flat at `slow_gain`; above `fast_speed` it is
/// flat at `fast_gain`; between them it interpolates linearly. Flat ends
/// matter: an unbounded curve makes the pointer's behaviour at the extremes
/// depend on sensor noise at one end and on how hard you flicked at the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferCurve {
    /// Speed (counts per poll) at or below which `slow_gain` applies.
    pub slow_speed: u16,
    /// Gain in 1/[`GAIN_ONE`] units for slow movement.
    pub slow_gain: u16,
    /// Speed (counts per poll) at or above which `fast_gain` applies.
    pub fast_speed: u16,
    /// Gain in 1/[`GAIN_ONE`] units for fast movement.
    pub fast_gain: u16,
}

impl TransferCurve {
    /// Gain for a given device speed, in 1/[`GAIN_ONE`] units.
    #[must_use]
    #[expect(
        clippy::as_conversions,
        reason = "widening u16 to i32 for interpolation, narrowing a clamped result back"
    )]
    pub const fn gain_at(&self, speed: u16) -> u16 {
        if speed <= self.slow_speed {
            return self.slow_gain;
        }
        if speed >= self.fast_speed || self.fast_speed <= self.slow_speed {
            return self.fast_gain;
        }
        let span = (self.fast_speed - self.slow_speed) as i32;
        let travelled = (speed - self.slow_speed) as i32;
        let slow = self.slow_gain as i32;
        let fast = self.fast_gain as i32;
        // Interpolate signed: the curve may descend as well as climb.
        (slow + ((fast - slow) * travelled) / span) as u16
    }
}

/// Carries the fractional remainder of a scaled delta between polls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SubPixel {
    residual_x: i32,
    residual_y: i32,
}

impl SubPixel {
    /// A fresh accumulator with no carried remainder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            residual_x: 0,
            residual_y: 0,
        }
    }

    /// Applies `curve` to one raw sample, returning pointer movement.
    ///
    /// Speed is the L1 norm (`|dx| + |dy|`) rather than the Euclidean one — it
    /// needs no square root, and the difference between them is a constant
    /// factor the curve's own constants absorb.
    pub fn apply(&mut self, curve: &TransferCurve, dx: i16, dy: i16) -> (i16, i16) {
        let speed = dx.unsigned_abs().saturating_add(dy.unsigned_abs());
        let gain = i32::from(curve.gain_at(speed));
        let out_x = Self::scale(&mut self.residual_x, dx, gain);
        let out_y = Self::scale(&mut self.residual_y, dy, gain);
        (out_x, out_y)
    }

    /// Discards the carried remainder.
    ///
    /// Worth doing when the pointer has been idle: a remainder left over from
    /// minutes ago is not part of the movement happening now.
    pub const fn reset(&mut self) {
        self.residual_x = 0;
        self.residual_y = 0;
    }

    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "result is clamped into i16 before narrowing"
    )]
    fn scale(residual: &mut i32, delta: i16, gain: i32) -> i16 {
        let scaled = i32::from(delta)
            .saturating_mul(gain)
            .saturating_add(*residual);
        let denominator = i32::from(GAIN_ONE);
        // Euclidean division floors for a positive divisor, so the residual is
        // always in 0..GAIN_ONE and neither sign is biased toward zero.
        let whole = scaled.div_euclid(denominator);
        *residual = scaled.rem_euclid(denominator);
        whole.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURVE: TransferCurve = TransferCurve {
        slow_speed: 8,
        slow_gain: 256,
        fast_speed: 64,
        fast_gain: 82,
    };

    #[test]
    fn gain_is_flat_outside_the_breakpoints() {
        assert_eq!(CURVE.gain_at(0), 256);
        assert_eq!(CURVE.gain_at(8), 256);
        assert_eq!(CURVE.gain_at(64), 82);
        assert_eq!(CURVE.gain_at(u16::MAX), 82);
    }

    #[test]
    fn gain_interpolates_monotonically_between_them() {
        let mid = CURVE.gain_at(36);
        assert!(mid < 256 && mid > 82, "midpoint gain was {mid}");
        // Monotonic across the whole ramp — a non-monotonic transfer function
        // feels like the pointer surging.
        let mut previous = u16::MAX;
        for speed in 0..=200u16 {
            let g = CURVE.gain_at(speed);
            assert!(
                g <= previous,
                "gain rose at speed {speed}: {previous} -> {g}"
            );
            previous = g;
        }
    }

    #[test]
    fn degenerate_curve_does_not_divide_by_zero() {
        let flat = TransferCurve {
            slow_speed: 32,
            slow_gain: 256,
            fast_speed: 32,
            fast_gain: 128,
        };
        assert_eq!(flat.gain_at(31), 256);
        // Both breakpoints coincide, so the slow branch wins the tie. The value
        // matters less than it being defined and not a divide-by-zero.
        assert_eq!(flat.gain_at(32), 256);
        assert_eq!(flat.gain_at(33), 128);
    }

    #[test]
    fn unity_gain_passes_through_untouched() {
        let unity = TransferCurve {
            slow_speed: u16::MAX,
            slow_gain: GAIN_ONE,
            fast_speed: u16::MAX,
            fast_gain: GAIN_ONE,
        };
        let mut acc = SubPixel::new();
        assert_eq!(acc.apply(&unity, 7, -13), (7, -13));
        assert_eq!(acc.apply(&unity, -1, 1), (-1, 1));
    }

    #[test]
    fn slow_movement_accumulates_instead_of_vanishing() {
        // A single count at 0.25 gain rounds to zero. Four of them must not.
        let quarter = TransferCurve {
            slow_speed: u16::MAX,
            slow_gain: 64,
            fast_speed: u16::MAX,
            fast_gain: 64,
        };
        let mut acc = SubPixel::new();
        let total: i32 = (0..4).map(|_| i32::from(acc.apply(&quarter, 1, 0).0)).sum();
        assert_eq!(total, 1, "four quarter-gain counts must yield one unit");
    }

    #[test]
    fn residual_does_not_bias_negative_movement() {
        // Truncating division would round -0.25 toward zero every poll and the
        // pointer would drift right. Equal travel each way must cancel exactly.
        let quarter = TransferCurve {
            slow_speed: u16::MAX,
            slow_gain: 64,
            fast_speed: u16::MAX,
            fast_gain: 64,
        };
        let mut acc = SubPixel::new();
        let mut net = 0i32;
        for _ in 0..64 {
            net += i32::from(acc.apply(&quarter, 1, 0).0);
        }
        for _ in 0..64 {
            net += i32::from(acc.apply(&quarter, -1, 0).0);
        }
        assert_eq!(net, 0, "equal travel in both directions must cancel");
    }

    #[test]
    fn fast_movement_is_compressed_not_clipped() {
        let mut acc = SubPixel::new();
        // 300 counts at the fast gain of 82/256 is ~96 — scaled down, which is
        // the point of the curve.
        let (x, _) = acc.apply(&CURVE, 300, 0);
        assert!(x < 300 && x > 0, "should be compressed, got {x}");

        // Compression must stay proportional rather than saturating at some
        // ceiling: three times the input gives roughly three times the output,
        // and it passes the old 8-bit limit on the way.
        acc.reset();
        let (big, _) = acc.apply(&CURVE, 900, 0);
        assert!(big > 127, "must not clip at the 8-bit ceiling, got {big}");
        assert!(
            (big - x * 3).abs() <= 2,
            "expected ~3x for 3x the input, got {x} then {big}"
        );
    }

    #[test]
    fn reset_clears_carried_remainder() {
        let quarter = TransferCurve {
            slow_speed: u16::MAX,
            slow_gain: 64,
            fast_speed: u16::MAX,
            fast_gain: 64,
        };
        let mut acc = SubPixel::new();
        let _ = acc.apply(&quarter, 3, 3);
        acc.reset();
        assert_eq!(acc, SubPixel::new());
    }

    #[test]
    fn extreme_input_saturates_rather_than_wrapping() {
        let boost = TransferCurve {
            slow_speed: u16::MAX,
            slow_gain: u16::MAX,
            fast_speed: u16::MAX,
            fast_gain: u16::MAX,
        };
        let mut acc = SubPixel::new();
        let (x, y) = acc.apply(&boost, i16::MAX, i16::MIN);
        assert_eq!(x, i16::MAX);
        assert_eq!(y, i16::MIN);
    }
}
