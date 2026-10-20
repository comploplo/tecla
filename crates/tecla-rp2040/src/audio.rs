//! PWM buzzer driver for RP2040.
//!
//! Drives a passive piezo buzzer or small speaker from one PWM channel, which
//! is what [`Action::Audio`](tecla::action::Action::Audio) needs on the other
//! end of an [`audio_task`](tecla::task::audio_task).
//!
//! # Wiring
//!
//! Buzzer between the PWM pin and ground. A passive buzzer is required — an
//! active one has its own oscillator and ignores the frequency entirely.
//!
//! # Usage
//!
//! ```text
//! let buzzer = PwmBuzzer::new_a(p.PWM_SLICE3, p.PIN_6);
//! spawner.spawn(audio_task(audio_ch.receiver(), buzzer)).unwrap();
//! ```

use embassy_rp::pwm::{Config, Pwm, Slice};
use embassy_rp::Peri;
use embassy_time::{Duration, Timer};
use tecla::audio::BuzzerDriver;

/// RP2040 system clock, used to derive the PWM divider for a target tone.
const SYS_CLK_HZ: u32 = 125_000_000;

/// The counter top used for every tone.
///
/// Fixed rather than computed so the duty cycle stays at a constant 50%
/// across the whole frequency range; only the clock divider changes.
///
/// 4000, not 1000: the divider maxes out at 255.9375, so `TOP` sets the lowest
/// reachable pitch at `sys_clk / (255.9375 * TOP)`. At 1000 that floor is
/// 488Hz — which puts the built-in 440Hz LAYER_CHANGE tone and the 150/200Hz
/// ERROR tones out of range, playing them sharp by up to two semitones. At
/// 4000 the floor is ~122Hz and every built-in melody is in range.
const TOP: u16 = 4000;

/// A passive buzzer driven by one RP2040 PWM channel.
pub struct PwmBuzzer<'d> {
    pwm: Pwm<'d>,
}

impl<'d> PwmBuzzer<'d> {
    /// Creates a buzzer on channel A of `slice`.
    #[must_use]
    pub fn new_a<T: Slice>(
        slice: Peri<'d, T>,
        pin: Peri<'d, impl embassy_rp::pwm::ChannelAPin<T>>,
    ) -> Self {
        Self {
            pwm: Pwm::new_output_a(slice, pin, silent_config()),
        }
    }

    /// Creates a buzzer on channel B of `slice`.
    #[must_use]
    pub fn new_b<T: Slice>(
        slice: Peri<'d, T>,
        pin: Peri<'d, impl embassy_rp::pwm::ChannelBPin<T>>,
    ) -> Self {
        Self {
            pwm: Pwm::new_output_b(slice, pin, silent_config()),
        }
    }
}

/// A disabled PWM config — the buzzer is silent until a tone is played.
fn silent_config() -> Config {
    let mut c = Config::default();
    c.top = TOP;
    c.compare_a = 0;
    c.compare_b = 0;
    c.enable = false;
    c
}

/// Wraps [`tecla::audio::pwm_divider_16ths`] in the HAL's fixed-point type.
///
/// The arithmetic and its clamping live in core, where they are unit-tested on
/// the host — this crate only compiles for `thumbv6m-none-eabi`, so tests
/// living here would never run.
fn divider_for(freq: u16) -> fixed::FixedU16<fixed::types::extra::U4> {
    fixed::FixedU16::from_bits(tecla::audio::pwm_divider_16ths(SYS_CLK_HZ, TOP, freq))
}

impl BuzzerDriver for PwmBuzzer<'_> {
    type Error = core::convert::Infallible;

    async fn play_tone(&mut self, freq: u16, duration_ms: u16) -> Result<(), Self::Error> {
        if freq == 0 {
            // A rest: stay silent for the duration rather than emitting DC.
            self.stop().await?;
            Timer::after(Duration::from_millis(u64::from(duration_ms))).await;
            return Ok(());
        }

        let mut c = Config::default();
        c.top = TOP;
        c.divider = divider_for(freq);
        // 50% duty — a square wave, the loudest a piezo will go.
        c.compare_a = TOP / 2;
        c.compare_b = TOP / 2;
        c.enable = true;
        self.pwm.set_config(&c);

        Timer::after(Duration::from_millis(u64::from(duration_ms))).await;
        self.stop().await
    }

    async fn stop(&mut self) -> Result<(), Self::Error> {
        self.pwm.set_config(&silent_config());
        Ok(())
    }
}
