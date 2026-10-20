//! Audio feedback — tone generation, melodies, and buzzer driver trait.
//!
//! Board crates map [`Action::Audio`](crate::action::Action::Audio) variants
//! to [`AudioCommand`]s, which an [`AudioController`] dispatches to a
//! hardware `BuzzerDriver`. Three built-in melodies ([`STARTUP`],
//! [`LAYER_CHANGE`], [`ERROR`]) cover the common cases; custom melodies are
//! just `&'static [Tone]` slices.
//!
//! See also: [`haptic`](crate::haptic) for vibration feedback,
//! [`rgb`](crate::rgb) for LED animations.

/// A single tone with frequency and duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tone {
    /// Frequency in Hz (0 = rest/silence).
    pub frequency_hz: u16,
    /// Duration in milliseconds.
    pub duration_ms: u16,
}

impl Tone {
    /// Creates a new tone.
    #[must_use]
    pub const fn new(frequency_hz: u16, duration_ms: u16) -> Self {
        Self {
            frequency_hz,
            duration_ms,
        }
    }

    /// A silent rest of 100 ms.
    pub const REST: Self = Self {
        frequency_hz: 0,
        duration_ms: 100,
    };
}

/// A static slice of tones forming a melody.
pub type Melody = &'static [Tone];

/// Startup jingle (C5–E5–G5).
pub const STARTUP: &[Tone] = &[
    Tone::new(523, 100),
    Tone::new(659, 100),
    Tone::new(784, 150),
];

/// Single-tone layer-change feedback.
pub const LAYER_CHANGE: &[Tone] = &[Tone::new(440, 50)];

/// Two-tone error feedback.
pub const ERROR: &[Tone] = &[Tone::new(200, 200), Tone::new(150, 200)];

/// Computes a fractional PWM clock divider for a target tone frequency.
///
/// Returns the divider in 1/16ths (a 12.4 fixed-point value), which is the
/// representation RP2040-style PWM peripherals use: 8 integer bits plus 4
/// fractional. Solves `divider = sys_clk / (freq * top)`.
///
/// The result is clamped to `16..=4095` (1.0..=255.9375). A frequency below
/// the representable range clamps to the lowest available pitch rather than
/// wrapping to an unrelated one, and `freq == 0` (a rest) returns 1.0 rather
/// than dividing by zero — callers should gate the output off for rests
/// instead of relying on the divider.
///
/// Lives here rather than in a board crate so it is testable on the host;
/// board crates convert the result to their HAL's fixed-point type.
#[must_use]
pub const fn pwm_divider_16ths(sys_clk_hz: u32, top: u16, freq: u16) -> u16 {
    if freq == 0 || top == 0 {
        return 16;
    }
    let sixteenths = (sys_clk_hz as u64 * 16) / (freq as u64 * top as u64);
    if sixteenths < 16 {
        16
    } else if sixteenths > 4095 {
        4095
    } else {
        sixteenths as u16
    }
}

/// Audio control commands mapped to [`Action::Audio`](crate::action::Action::Audio).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioCommand {
    /// Play a single tone.
    PlayTone(Tone),
    /// Play a melody (static slice of tones).
    PlayMelody(&'static [Tone]),
    /// Stop all audio playback.
    Stop,
    /// Toggle audio on/off.
    Toggle,
}

/// Async buzzer hardware driver.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait BuzzerDriver {
    /// Driver error type.
    type Error;
    /// Play a tone at the given frequency for the given duration.
    async fn play_tone(&mut self, freq: u16, duration_ms: u16) -> Result<(), Self::Error>;
    /// Stop all audio output.
    async fn stop(&mut self) -> Result<(), Self::Error>;
}

/// Manages audio playback state (enabled/disabled).
pub struct AudioController {
    enabled: bool,
}

impl AudioController {
    /// Creates a new controller with audio enabled.
    #[must_use]
    pub const fn new() -> Self {
        Self { enabled: true }
    }

    /// Returns whether audio output is enabled.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables audio output.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Toggles audio output on/off.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    /// Processes a command, returning `Some(cmd)` if audio is enabled (except Toggle).
    pub fn handle_command(&mut self, cmd: AudioCommand) -> Option<AudioCommand> {
        match cmd {
            AudioCommand::Toggle => {
                self.toggle();
                None
            }
            other => {
                if self.enabled {
                    Some(other)
                } else {
                    None
                }
            }
        }
    }
}

impl Default for AudioController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_new() {
        let t = Tone::new(440, 100);
        assert_eq!(t.frequency_hz, 440);
        assert_eq!(t.duration_ms, 100);
    }

    #[test]
    fn tone_rest() {
        assert_eq!(Tone::REST.frequency_hz, 0);
    }

    #[test]
    fn audio_controller_default_enabled() {
        let ctrl = AudioController::new();
        assert!(ctrl.enabled());
    }

    #[test]
    fn audio_controller_toggle() {
        let mut ctrl = AudioController::new();
        ctrl.toggle();
        assert!(!ctrl.enabled());
        ctrl.toggle();
        assert!(ctrl.enabled());
    }

    #[test]
    fn audio_controller_set_enabled() {
        let mut ctrl = AudioController::new();
        ctrl.set_enabled(false);
        assert!(!ctrl.enabled());
    }

    #[test]
    fn handle_command_toggle_returns_none() {
        let mut ctrl = AudioController::new();
        assert!(ctrl.handle_command(AudioCommand::Toggle).is_none());
        assert!(!ctrl.enabled());
    }

    #[test]
    fn handle_command_stop_when_enabled() {
        let mut ctrl = AudioController::new();
        assert_eq!(
            ctrl.handle_command(AudioCommand::Stop),
            Some(AudioCommand::Stop)
        );
    }

    #[test]
    fn handle_command_stop_when_disabled() {
        let mut ctrl = AudioController::new();
        ctrl.set_enabled(false);
        assert!(ctrl.handle_command(AudioCommand::Stop).is_none());
    }

    #[test]
    fn handle_command_play_tone_when_enabled() {
        let mut ctrl = AudioController::new();
        let tone = Tone::new(440, 100);
        let result = ctrl.handle_command(AudioCommand::PlayTone(tone));
        assert_eq!(result, Some(AudioCommand::PlayTone(tone)));
    }

    #[test]
    fn handle_command_play_tone_when_disabled() {
        let mut ctrl = AudioController::new();
        ctrl.set_enabled(false);
        assert!(ctrl
            .handle_command(AudioCommand::PlayTone(Tone::new(440, 100)))
            .is_none());
    }

    #[test]
    fn handle_command_play_melody_when_enabled() {
        let mut ctrl = AudioController::new();
        let result = ctrl.handle_command(AudioCommand::PlayMelody(STARTUP));
        assert_eq!(result, Some(AudioCommand::PlayMelody(STARTUP)));
    }

    #[test]
    fn handle_command_play_melody_when_disabled() {
        let mut ctrl = AudioController::new();
        ctrl.set_enabled(false);
        assert!(ctrl
            .handle_command(AudioCommand::PlayMelody(STARTUP))
            .is_none());
    }

    #[test]
    fn default_matches_new() {
        let a = AudioController::new();
        let b = AudioController::default();
        assert_eq!(a.enabled(), b.enabled());
    }

    #[test]
    fn tone_equality() {
        assert_eq!(Tone::new(440, 100), Tone::new(440, 100));
        assert_ne!(Tone::new(440, 100), Tone::new(441, 100));
    }

    // ── PWM divider ──────────────────────────────────────────────────────────

    const RP2040_CLK: u32 = 125_000_000;
    const TOP: u16 = 4000;

    /// Inverts pwm_divider_16ths — what pitch does this divider actually give?
    fn freq_for(sixteenths: u16) -> u32 {
        ((u64::from(RP2040_CLK) * 16) / (u64::from(sixteenths) * u64::from(TOP))) as u32
    }

    #[test]
    fn divider_round_trips_to_the_requested_pitch() {
        // Error must stay well under a semitone (~6%), or it is audibly the
        // wrong note. C5 through C8.
        for freq in [523u16, 587, 659, 698, 784, 880, 988, 1047, 2093, 4186] {
            let got = freq_for(pwm_divider_16ths(RP2040_CLK, TOP, freq));
            let err = (i64::from(got) - i64::from(freq)).unsigned_abs();
            assert!(
                err * 100 / u64::from(freq) < 6,
                "{freq}Hz round-tripped to {got}Hz"
            );
        }
    }

    #[test]
    fn divider_is_clamped_to_the_representable_range() {
        // Below the range: clamp to the slowest divider (lowest pitch), never
        // wrap around to a high one.
        assert_eq!(pwm_divider_16ths(RP2040_CLK, TOP, 1), 4095);
        // Above the range: clamp to the fastest divider. Needs a frequency
        // above sys_clk / TOP to actually hit the floor.
        assert_eq!(pwm_divider_16ths(RP2040_CLK, TOP, u16::MAX), 16);
        for freq in [1u16, 100, 490, 1000, 20_000, u16::MAX] {
            let d = pwm_divider_16ths(RP2040_CLK, TOP, freq);
            assert!((16..=4095).contains(&d), "{freq}Hz produced divider {d}");
        }
    }

    #[test]
    fn rest_and_zero_top_do_not_divide_by_zero() {
        assert_eq!(pwm_divider_16ths(RP2040_CLK, TOP, 0), 16);
        assert_eq!(pwm_divider_16ths(RP2040_CLK, 0, 440), 16);
    }

    #[test]
    fn every_builtin_melody_tone_is_playable() {
        for melody in [STARTUP, LAYER_CHANGE, ERROR] {
            for tone in melody {
                if tone.frequency_hz == 0 {
                    continue;
                }
                let got = freq_for(pwm_divider_16ths(RP2040_CLK, TOP, tone.frequency_hz));
                let err = (i64::from(got) - i64::from(tone.frequency_hz)).unsigned_abs();
                assert!(
                    err * 100 / u64::from(tone.frequency_hz) < 6,
                    "builtin melody tone {}Hz would play as {got}Hz",
                    tone.frequency_hz
                );
            }
        }
    }

    #[test]
    fn a_slower_clock_needs_a_smaller_divider_for_the_same_pitch() {
        let fast = pwm_divider_16ths(125_000_000, TOP, 440);
        let slow = pwm_divider_16ths(64_000_000, TOP, 440);
        assert!(slow < fast, "slower sys_clk should divide less");
    }
}
