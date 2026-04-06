//! Haptic feedback — vibration patterns and haptic driver trait.
//!
//! Board crates map [`Action::Haptic`](crate::action::Action::Haptic) variants
//! to [`HapticCommand`]s, which a [`HapticController`] dispatches to a
//! hardware `HapticDriver` (e.g. DRV2605L). Built-in effects include
//! [`Click`](HapticEffect::Click), [`Bump`](HapticEffect::Bump), and
//! [`DoubleClick`](HapticEffect::DoubleClick); arbitrary strength/duration
//! via [`Custom`](HapticEffect::Custom).
//!
//! See also: [`audio`](crate::audio) for buzzer feedback,
//! [`rgb`](crate::rgb) for LED animations.

/// A haptic feedback effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HapticEffect {
    /// Short sharp click.
    Click,
    /// Soft bump.
    Bump,
    /// Two rapid clicks.
    DoubleClick,
    /// Custom effect with configurable strength and duration.
    Custom {
        /// Vibration strength (0–255).
        strength: u8,
        /// Duration in milliseconds.
        duration_ms: u16,
    },
}

/// Haptic control commands mapped to [`Action::Haptic`](crate::action::Action::Haptic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HapticCommand {
    /// Play a haptic effect.
    Play(HapticEffect),
    /// Toggle haptic feedback on/off.
    Toggle,
}

/// Async haptic hardware driver (DRV2605L-style).
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait HapticDriver {
    /// Driver error type.
    type Error;
    /// Play the given haptic effect.
    async fn play(&mut self, effect: &HapticEffect) -> Result<(), Self::Error>;
    /// Stop all haptic output.
    async fn stop(&mut self) -> Result<(), Self::Error>;
}

/// Manages haptic feedback state (enabled/disabled).
pub struct HapticController {
    enabled: bool,
}

impl HapticController {
    /// Creates a new controller with haptic feedback enabled.
    #[must_use]
    pub const fn new() -> Self {
        Self { enabled: true }
    }

    /// Returns whether haptic feedback is enabled.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables haptic feedback.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Toggles haptic feedback on/off.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    /// Processes a command, returning the effect to play if enabled.
    pub fn handle_command(&mut self, cmd: HapticCommand) -> Option<HapticEffect> {
        match cmd {
            HapticCommand::Toggle => {
                self.toggle();
                None
            }
            HapticCommand::Play(effect) => {
                if self.enabled {
                    Some(effect)
                } else {
                    None
                }
            }
        }
    }
}

impl Default for HapticController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haptic_effect_variants_distinct() {
        assert_ne!(HapticEffect::Click, HapticEffect::Bump);
        assert_ne!(HapticEffect::Bump, HapticEffect::DoubleClick);
    }

    #[test]
    fn haptic_effect_custom() {
        let e = HapticEffect::Custom {
            strength: 128,
            duration_ms: 50,
        };
        assert!(matches!(e, HapticEffect::Custom { .. }));
    }

    #[test]
    fn haptic_command_variants_distinct() {
        assert_ne!(
            HapticCommand::Play(HapticEffect::Click),
            HapticCommand::Toggle
        );
    }

    #[test]
    fn controller_default_enabled() {
        let ctrl = HapticController::new();
        assert!(ctrl.enabled());
    }

    #[test]
    fn controller_toggle() {
        let mut ctrl = HapticController::new();
        ctrl.toggle();
        assert!(!ctrl.enabled());
        ctrl.toggle();
        assert!(ctrl.enabled());
    }

    #[test]
    fn controller_set_enabled() {
        let mut ctrl = HapticController::new();
        ctrl.set_enabled(false);
        assert!(!ctrl.enabled());
    }

    #[test]
    fn handle_command_toggle_returns_none() {
        let mut ctrl = HapticController::new();
        assert!(ctrl.handle_command(HapticCommand::Toggle).is_none());
        assert!(!ctrl.enabled());
    }

    #[test]
    fn handle_command_play_when_enabled() {
        let mut ctrl = HapticController::new();
        let result = ctrl.handle_command(HapticCommand::Play(HapticEffect::Click));
        assert_eq!(result, Some(HapticEffect::Click));
    }

    #[test]
    fn handle_command_play_when_disabled() {
        let mut ctrl = HapticController::new();
        ctrl.set_enabled(false);
        assert!(ctrl
            .handle_command(HapticCommand::Play(HapticEffect::Click))
            .is_none());
    }

    #[test]
    fn handle_command_bump_when_enabled() {
        let mut ctrl = HapticController::new();
        assert_eq!(
            ctrl.handle_command(HapticCommand::Play(HapticEffect::Bump)),
            Some(HapticEffect::Bump)
        );
    }

    #[test]
    fn handle_command_double_click_when_enabled() {
        let mut ctrl = HapticController::new();
        assert_eq!(
            ctrl.handle_command(HapticCommand::Play(HapticEffect::DoubleClick)),
            Some(HapticEffect::DoubleClick)
        );
    }

    #[test]
    fn handle_command_custom_when_enabled() {
        let mut ctrl = HapticController::new();
        let custom = HapticEffect::Custom {
            strength: 200,
            duration_ms: 100,
        };
        assert_eq!(
            ctrl.handle_command(HapticCommand::Play(custom)),
            Some(custom)
        );
    }

    #[test]
    fn default_matches_new() {
        let a = HapticController::new();
        let b = HapticController::default();
        assert_eq!(a.enabled(), b.enabled());
    }
}
