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

// ── DRV2605L ──────────────────────────────────────────────────────────────────
//
// Register map and waveform mapping for the TI DRV2605L, the usual haptic
// driver on a keyboard. Kept here rather than in a board crate so it is
// testable on the host: a board crate only compiles for its MCU target, so
// tests living there would never run.

/// DRV2605L I2C device address (fixed by the part).
pub const DRV2605L_ADDR: u8 = 0x5A;

/// DRV2605L register addresses used by the driver.
pub mod drv2605l {
    /// Mode register: standby bit and trigger mode.
    pub const REG_MODE: u8 = 0x01;
    /// Fires a waveform when MODE is set to internal trigger.
    pub const REG_GO: u8 = 0x0C;
    /// First of the eight waveform-sequence slots.
    pub const REG_WAVEFORM_SEQ0: u8 = 0x04;
    /// Real-time playback amplitude, used for `Custom` effects.
    pub const REG_RTP_INPUT: u8 = 0x02;
    /// Library selection (1–5 ERM, 6 LRA).
    pub const REG_LIBRARY: u8 = 0x03;
    /// Feedback control: ERM vs LRA selection.
    pub const REG_FEEDBACK: u8 = 0x1A;

    /// MODE value for internal-trigger waveform playback.
    pub const MODE_INTERNAL_TRIGGER: u8 = 0x00;
    /// MODE value for real-time playback (amplitude written to RTP_INPUT).
    pub const MODE_REALTIME: u8 = 0x05;
    /// MODE value for standby (lowest power).
    pub const MODE_STANDBY: u8 = 0x40;
    /// Terminates a waveform sequence.
    pub const WAVEFORM_END: u8 = 0x00;
    /// ERM library 1 — the general-purpose click library.
    pub const LIBRARY_ERM_A: u8 = 0x01;
}

/// How a [`HapticEffect`] is realised on a DRV2605L.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drv2605lPlayback {
    /// Fire up to two ROM waveform effects in sequence, then GO.
    ///
    /// The values are effect IDs from the DRV2605L's internal library;
    /// `None` in the second slot means a single-effect sequence.
    Waveform {
        /// First waveform slot.
        first: u8,
        /// Second waveform slot, if the effect is a double.
        second: Option<u8>,
    },
    /// Drive the actuator directly at a fixed amplitude for a duration.
    RealTime {
        /// Amplitude written to `REG_RTP_INPUT`.
        amplitude: u8,
        /// How long to hold it, in milliseconds.
        duration_ms: u16,
    },
}

impl HapticEffect {
    /// Maps this effect onto the DRV2605L's playback modes.
    ///
    /// The waveform IDs are from the DRV2605L ROM library:
    /// 1 = "Strong Click 100%", 7 = "Soft Bump 100%", 10 = "Double Click 100%".
    #[must_use]
    pub const fn drv2605l_playback(self) -> Drv2605lPlayback {
        match self {
            Self::Click => Drv2605lPlayback::Waveform {
                first: 1,
                second: None,
            },
            Self::Bump => Drv2605lPlayback::Waveform {
                first: 7,
                second: None,
            },
            // Waveform 10 is a double click on its own; no second slot needed.
            Self::DoubleClick => Drv2605lPlayback::Waveform {
                first: 10,
                second: None,
            },
            Self::Custom {
                strength,
                duration_ms,
            } => Drv2605lPlayback::RealTime {
                amplitude: strength,
                duration_ms,
            },
        }
    }
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
    fn haptic_effect_custom() {
        let e = HapticEffect::Custom {
            strength: 128,
            duration_ms: 50,
        };
        assert!(matches!(e, HapticEffect::Custom { .. }));
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

    // ── DRV2605L mapping ─────────────────────────────────────────────────────

    #[test]
    fn builtin_effects_use_rom_waveforms_not_realtime() {
        // ROM waveforms are tuned per-actuator by the part; driving them via
        // real-time playback instead would feel wrong and waste power.
        for effect in [
            HapticEffect::Click,
            HapticEffect::Bump,
            HapticEffect::DoubleClick,
        ] {
            assert!(
                matches!(
                    effect.drv2605l_playback(),
                    Drv2605lPlayback::Waveform { .. }
                ),
                "{effect:?} should map to a ROM waveform"
            );
        }
    }

    #[test]
    fn each_builtin_effect_maps_to_a_distinct_waveform() {
        let ids: [u8; 3] = [
            HapticEffect::Click,
            HapticEffect::Bump,
            HapticEffect::DoubleClick,
        ]
        .map(|e| match e.drv2605l_playback() {
            Drv2605lPlayback::Waveform { first, .. } => first,
            Drv2605lPlayback::RealTime { .. } => 0,
        });
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[0], ids[2]);
        // Waveform 0 terminates a sequence — using it as an effect plays nothing.
        assert!(
            ids.iter().all(|&id| id != drv2605l::WAVEFORM_END),
            "no effect may map to the sequence terminator"
        );
    }

    #[test]
    fn waveform_ids_are_within_the_rom_library() {
        // The DRV2605L ROM libraries define effects 1..=123.
        for effect in [
            HapticEffect::Click,
            HapticEffect::Bump,
            HapticEffect::DoubleClick,
        ] {
            if let Drv2605lPlayback::Waveform { first, second } = effect.drv2605l_playback() {
                assert!((1..=123).contains(&first), "{effect:?}: waveform {first}");
                if let Some(s) = second {
                    assert!((1..=123).contains(&s), "{effect:?}: waveform {s}");
                }
            }
        }
    }

    #[test]
    fn custom_effects_use_realtime_playback_and_keep_their_parameters() {
        let e = HapticEffect::Custom {
            strength: 200,
            duration_ms: 45,
        };
        assert_eq!(
            e.drv2605l_playback(),
            Drv2605lPlayback::RealTime {
                amplitude: 200,
                duration_ms: 45,
            }
        );
    }

    #[test]
    fn a_zero_strength_custom_effect_stays_zero() {
        // Silence must not be quietly turned into a buzz.
        assert_eq!(
            HapticEffect::Custom {
                strength: 0,
                duration_ms: 10,
            }
            .drv2605l_playback(),
            Drv2605lPlayback::RealTime {
                amplitude: 0,
                duration_ms: 10,
            }
        );
    }

    #[test]
    fn mode_constants_are_distinct() {
        // Writing the wrong one leaves the part in standby and nothing buzzes.
        let modes = [
            drv2605l::MODE_INTERNAL_TRIGGER,
            drv2605l::MODE_REALTIME,
            drv2605l::MODE_STANDBY,
        ];
        for i in 0..modes.len() {
            for j in (i + 1)..modes.len() {
                assert_ne!(modes[i], modes[j]);
            }
        }
    }
}
