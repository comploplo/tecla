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
    fn startup_melody_not_empty() {
        assert!(!STARTUP.is_empty());
    }

    #[test]
    fn layer_change_melody_not_empty() {
        assert!(!LAYER_CHANGE.is_empty());
    }

    #[test]
    fn error_melody_not_empty() {
        assert!(!ERROR.is_empty());
    }

    #[test]
    fn audio_command_variants_distinct() {
        assert_ne!(AudioCommand::Stop, AudioCommand::Toggle);
        assert_ne!(
            AudioCommand::PlayTone(Tone::new(440, 100)),
            AudioCommand::Stop
        );
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
}
