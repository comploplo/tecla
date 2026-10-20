//! Per-pad filtering of capacitive discharge counts, without allocation.

/// Schmitt hysteresis and two-sample edge confirmation for capacitive pads.
///
/// Supply untouched boot baselines and each pad's observed calibration range.
/// A press normally needs a count above 1.30 times baseline; release needs a
/// count at or below 1.15 times baseline. Calibration noise can widen these
/// margins slightly. Each edge needs two consecutive qualifying samples, so
/// isolated spikes are rejected without a millisecond lockout between taps.
///
/// Baselines adapt slowly only after sixteen consecutive idle samples at or
/// below the release threshold. Pressed, pending, and hysteresis-band samples
/// freeze adaptation. A pad held during boot cannot be distinguished from its
/// baseline; release it to let the idle baseline recover before playing.
pub struct CapacitiveFilter<const KEYS: usize> {
    pads: [Pad; KEYS],
}

impl<const KEYS: usize> CapacitiveFilter<KEYS> {
    /// Consecutive qualifying samples required for either edge.
    pub const CONFIRM_SAMPLES: u8 = 2;
    /// Consecutive stable idle samples required before baseline adaptation.
    pub const IDLE_SAMPLES: u8 = 16;

    /// Creates a released filter from mean discharge counts and max-minus-min
    /// calibration noise spans. All arithmetic is bounded, including bad counts.
    pub fn new(baselines: [u32; KEYS], noise_spans: [u32; KEYS]) -> Self {
        Self {
            pads: core::array::from_fn(|index| Pad {
                baseline: u64::from(baselines[index]) << 8,
                noise_span: noise_spans[index],
                pressed: false,
                pending: 0,
                idle: 0,
            }),
        }
    }

    /// Consumes one coherent scan and returns the filtered state of every pad.
    ///
    /// Timing is in scans, not milliseconds: a press or a gap must include two
    /// samples. Pads have independent counters, so a noisy pad cannot delay a
    /// chord or another pad's release.
    pub fn update(&mut self, counts: &[u32; KEYS]) -> [bool; KEYS] {
        core::array::from_fn(|index| self.pads[index].sample::<KEYS>(counts[index]))
    }
}

struct Pad {
    // Eight fractional bits let slow tracking average noise instead of chasing
    // single-count jitter. u64 also accommodates the entire u32 input range.
    baseline: u64,
    noise_span: u32,
    pressed: bool,
    pending: u8,
    idle: u8,
}

impl Pad {
    fn thresholds(&self) -> (u32, u32) {
        let baseline = (self.baseline >> 8) as u32;
        // A noisy boot sample must not make a pad permanently insensitive.
        let noise = self.noise_span.min(baseline / 8);
        // 333/256 is the existing scanner's four-round press ratio.
        let press_margin = ((u64::from(baseline) * 77) / 256) as u32;
        let release_margin = ((u64::from(baseline) * 77) / 512) as u32;
        let press_margin = press_margin.max(noise.saturating_mul(3));
        let release_margin = release_margin
            .max(noise.saturating_mul(2))
            .min(press_margin);
        (
            baseline.saturating_add(press_margin),
            baseline.saturating_add(release_margin),
        )
    }

    fn sample<const KEYS: usize>(&mut self, count: u32) -> bool {
        let (press, release) = self.thresholds();
        let transition = if self.pressed {
            count <= release
        } else {
            count > press
        };
        if transition {
            self.idle = 0;
            self.pending = self.pending.saturating_add(1);
            if self.pending >= CapacitiveFilter::<KEYS>::CONFIRM_SAMPLES {
                self.pressed = !self.pressed;
                self.pending = 0;
            }
            return self.pressed;
        }
        self.pending = 0;
        if self.pressed || count > release {
            self.idle = 0;
        } else {
            self.idle = self.idle.saturating_add(1);
            if self.idle >= CapacitiveFilter::<KEYS>::IDLE_SAMPLES {
                let sample = u64::from(count) << 8;
                if sample > self.baseline {
                    self.baseline += (sample - self.baseline) / 64;
                } else {
                    self.baseline -= (self.baseline - sample) / 64;
                }
            }
        }
        self.pressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_single_scan_spikes_and_uses_separate_edge_thresholds() {
        let mut filter = CapacitiveFilter::new([100], [0]);
        assert_eq!(filter.pads[0].thresholds(), (130, 115));
        for count in [131, 129, 132, 130, 150, 100] {
            assert_eq!(filter.update(&[count]), [false]);
        }
        assert_eq!(filter.update(&[150]), [false]);
        assert_eq!(filter.update(&[150]), [true]);
        for count in [129, 132, 116, 130, 114, 120] {
            assert_eq!(filter.update(&[count]), [true]);
        }
        assert_eq!(filter.update(&[115]), [true]);
        assert_eq!(filter.update(&[115]), [false]);
    }

    #[test]
    fn accepts_short_taps_and_repeated_two_sample_gaps() {
        let mut filter = CapacitiveFilter::new([100], [0]);
        for _ in 0..8 {
            assert_eq!(filter.update(&[160]), [false]);
            assert_eq!(filter.update(&[160]), [true]);
            assert_eq!(filter.update(&[100]), [true]);
            assert_eq!(filter.update(&[100]), [false]);
        }
    }

    #[test]
    fn pad_confirmation_is_independent_for_chords_and_releases() {
        let mut filter = CapacitiveFilter::new([100; 3], [0; 3]);
        assert_eq!(filter.update(&[150, 150, 100]), [false; 3]);
        assert_eq!(filter.update(&[150, 150, 150]), [true, true, false]);
        assert_eq!(filter.update(&[100, 150, 100]), [true, true, false]);
        assert_eq!(filter.update(&[100, 150, 150]), [false, true, false]);
        assert_eq!(filter.update(&[150, 150, 150]), [false, true, true]);
    }

    #[test]
    fn long_hold_and_pending_edges_never_teach_a_touched_baseline() {
        let mut filter = CapacitiveFilter::new([100], [0]);
        for _ in 0..32 {
            filter.update(&[100]);
        }
        let baseline = filter.pads[0].baseline;
        assert_eq!(filter.update(&[150]), [false]);
        assert_eq!(filter.pads[0].baseline, baseline);
        assert_eq!(filter.update(&[150]), [true]);
        for _ in 0..10_000 {
            assert_eq!(filter.update(&[150]), [true]);
        }
        assert_eq!(filter.update(&[110]), [true]);
        assert_eq!(filter.pads[0].baseline, baseline);
        assert_eq!(filter.update(&[150]), [true]);
        assert_eq!(filter.update(&[100]), [true]);
        assert_eq!(filter.update(&[100]), [false]);
        assert_eq!(filter.pads[0].baseline, baseline);
    }

    #[test]
    fn tracks_small_idle_drift_but_freezes_near_press_band() {
        let mut filter = CapacitiveFilter::new([100], [0]);
        for _ in 0..15 {
            assert_eq!(filter.update(&[110]), [false]);
        }
        assert_eq!(filter.pads[0].baseline, 100 << 8);
        for _ in 0..512 {
            assert_eq!(filter.update(&[110]), [false]);
        }
        assert!((109..=110).contains(&((filter.pads[0].baseline >> 8) as u32)));
        let baseline = filter.pads[0].baseline;
        for _ in 0..512 {
            assert_eq!(filter.update(&[135]), [false]);
        }
        assert_eq!(filter.pads[0].baseline, baseline);
        assert_eq!(filter.update(&[160]), [false]);
        assert_eq!(filter.update(&[160]), [true]);
    }

    #[test]
    fn contaminated_boot_recovers_only_after_the_pad_returns_idle() {
        let mut filter = CapacitiveFilter::new([300], [100]);
        for _ in 0..128 {
            assert_eq!(filter.update(&[300]), [false]);
        }
        for _ in 0..1024 {
            assert_eq!(filter.update(&[100]), [false]);
        }
        assert!((100..=101).contains(&((filter.pads[0].baseline >> 8) as u32)));
        assert_eq!(filter.update(&[160]), [false]);
        assert_eq!(filter.update(&[160]), [true]);
    }

    #[test]
    fn stuck_and_extreme_counts_are_bounded_without_false_idle_learning() {
        let mut filter = CapacitiveFilter::new([100], [u32::MAX]);
        assert_eq!(filter.update(&[u32::MAX]), [false]);
        assert_eq!(filter.update(&[u32::MAX]), [true]);
        for _ in 0..1024 {
            assert_eq!(filter.update(&[u32::MAX]), [true]);
        }
        assert_eq!(filter.pads[0].baseline, 100 << 8);
        assert_eq!(filter.update(&[0]), [true]);
        assert_eq!(filter.update(&[0]), [false]);
        let mut maximum = CapacitiveFilter::new([u32::MAX], [u32::MAX]);
        for _ in 0..1024 {
            assert_eq!(maximum.update(&[u32::MAX]), [false]);
        }
        let mut zero = CapacitiveFilter::new([0], [0]);
        assert_eq!(zero.update(&[0]), [false]);
        assert_eq!(zero.update(&[1]), [false]);
        assert_eq!(zero.update(&[1]), [true]);
        let mut empty = CapacitiveFilter::<0>::new([], []);
        assert!(empty.update(&[]).is_empty());
    }
}
