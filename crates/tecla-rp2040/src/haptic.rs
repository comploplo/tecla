//! DRV2605L haptic driver for RP2040.
//!
//! Drives a TI DRV2605L haptic controller over I²C, which is what
//! [`Action::Haptic`](tecla::action::Action::Haptic) needs on the other end of
//! a [`haptic_task`](tecla::task::haptic_task).
//!
//! # Wiring
//!
//! DRV2605L on the I²C bus at address
//! [`DRV2605L_ADDR`](tecla::haptic::DRV2605L_ADDR) (0x5A, fixed by the part),
//! with an ERM or LRA actuator on OUT+/OUT−. `EN` tied high.
//!
//! # Usage
//!
//! ```text
//! let i2c = I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, Irqs, i2c::Config::default());
//! let mut haptic = Drv2605l::new(i2c);
//! haptic.init().await?;
//! spawner.spawn(haptic_task(haptic_ch.receiver(), haptic)).unwrap();
//! ```
//!
//! The effect-to-waveform mapping lives in
//! [`tecla::haptic`](tecla::haptic::Drv2605lPlayback), where it is unit-tested
//! on the host; this module is only the I²C register writes.

use embassy_rp::i2c::Async;
use embassy_rp::i2c::{Error as I2cError, I2c, Instance};
use embassy_time::{Duration, Timer};
use tecla::haptic::{drv2605l as reg, Drv2605lPlayback, HapticDriver, HapticEffect, DRV2605L_ADDR};

/// A DRV2605L haptic controller on an RP2040 I²C bus.
pub struct Drv2605l<'d, T: Instance> {
    i2c: I2c<'d, T, Async>,
}

impl<'d, T: Instance> Drv2605l<'d, T> {
    /// Wraps an async I²C bus as a haptic driver.
    ///
    /// Call [`init`](Self::init) before playing anything: the part boots into
    /// standby and will silently ignore GO until it is taken out.
    #[must_use]
    pub const fn new(i2c: I2c<'d, T, Async>) -> Self {
        Self { i2c }
    }

    /// Brings the part out of standby and selects the ERM click library.
    ///
    /// Returns the I²C error if the DRV2605L does not acknowledge — which is
    /// how a missing or mis-wired part shows up, rather than as silence.
    pub async fn init(&mut self) -> Result<(), I2cError> {
        self.write_reg(reg::REG_MODE, reg::MODE_INTERNAL_TRIGGER)
            .await?;
        self.write_reg(reg::REG_LIBRARY, reg::LIBRARY_ERM_A).await?;
        // Feedback register bit 7 clear = ERM mode (the default actuator type).
        self.write_reg(reg::REG_FEEDBACK, 0x00).await
    }

    async fn write_reg(&mut self, register: u8, value: u8) -> Result<(), I2cError> {
        self.i2c.write_async(DRV2605L_ADDR, [register, value]).await
    }
}

impl<T: Instance> HapticDriver for Drv2605l<'_, T> {
    type Error = I2cError;

    async fn play(&mut self, effect: &HapticEffect) -> Result<(), Self::Error> {
        match effect.drv2605l_playback() {
            Drv2605lPlayback::Waveform { first, second } => {
                self.write_reg(reg::REG_MODE, reg::MODE_INTERNAL_TRIGGER)
                    .await?;
                self.write_reg(reg::REG_WAVEFORM_SEQ0, first).await?;
                self.write_reg(
                    reg::REG_WAVEFORM_SEQ0 + 1,
                    second.unwrap_or(reg::WAVEFORM_END),
                )
                .await?;
                // Terminate the sequence so stale slots are not replayed.
                if second.is_some() {
                    self.write_reg(reg::REG_WAVEFORM_SEQ0 + 2, reg::WAVEFORM_END)
                        .await?;
                }
                self.write_reg(reg::REG_GO, 1).await
            }
            Drv2605lPlayback::RealTime {
                amplitude,
                duration_ms,
            } => {
                self.write_reg(reg::REG_MODE, reg::MODE_REALTIME).await?;
                self.write_reg(reg::REG_RTP_INPUT, amplitude).await?;
                Timer::after(Duration::from_millis(u64::from(duration_ms))).await;
                // Drop the amplitude before leaving real-time mode, or the
                // actuator keeps driving until something else writes RTP.
                self.write_reg(reg::REG_RTP_INPUT, 0).await?;
                self.write_reg(reg::REG_MODE, reg::MODE_INTERNAL_TRIGGER)
                    .await
            }
        }
    }

    async fn stop(&mut self) -> Result<(), Self::Error> {
        self.write_reg(reg::REG_RTP_INPUT, 0).await?;
        self.write_reg(reg::REG_GO, 0).await
    }
}
