//! ZSA Navigator trackball driver over I2C.
//!
//! Talks to the SC18IS606 bridge on the Voyager's TRRS bus, which relays to the
//! PAW3805EK sensor over SPI. The wire format — frames, registers, decode — is
//! [`tecla::devices::mouse::navigator`], unit-tested on the host; this is the
//! transport and the polling loop.
//!
//! # Wiring
//!
//! Nothing to wire: the module plugs into the TRRS jacks and joins the same
//! I2C1 the MCP23018 expander and the LED drivers already share.
//!
//! # Usage
//!
//! Takes the bus mutex itself, not a
//! [`SharedI2c`](crate::shared_i2c::SharedI2c) handle, because a sensor's
//! write/read pair must hold the lock throughout to prevent interleaved access.
//!
//! ```text
//! let mut ball = Navigator::new(bus);
//! ball.init().await?;
//! spawner.spawn(pointing_task(ball, report_ch)).unwrap();
//! ```

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::{I2c, Operation};
use tecla::devices::mouse::navigator::{
    self as proto, paw3805ek, CONFIG_ID_CHECK_INDEX, CONFIG_SEQUENCE, FRAME_LEN, NAVIGATOR_ADDR,
    RESPONSE_INDEX,
};
use tecla::devices::mouse::pointing::AsyncPointingScanner;
use tecla::devices::mouse::transfer::{SubPixel, TransferCurve};
use tecla::pipeline::event::PointingEvent;

/// Settling time per frame byte before the bridge's reply can be read.
const SETTLE_US_PER_BYTE: u64 = 15;
/// Pause after reconfiguring the bridge's SPI port.
const BRIDGE_SETTLE_MS: u64 = 10;
/// Pause after each bring-up step, covering the sensor's reset time.
const CONFIG_STEP_MS: u64 = 1;
/// Motion poll interval.
const POLL_MS: u64 = 7;
/// Retry interval when the module is absent or the link has dropped.
const PROBE_MS: u64 = 1_000;

/// Why the trackball could not be brought up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigatorError<E> {
    /// The I2C bridge did not respond.
    Bus(E),
    /// The bridge responded but the sensor behind it did not identify itself.
    ///
    /// Carries what the product-ID register actually returned. This is the
    /// failure that distinguishes "no module plugged in" from "module plugged
    /// in but its SPI link is broken" — on I2C alone the two look identical.
    SensorId(u8),
}

/// A ZSA Navigator trackball on a shared I2C bus.
///
/// Takes the bus mutex directly rather than a
/// [`SharedI2c`](crate::shared_i2c::SharedI2c) handle, because one sensor
/// access is a *write then read* pair that must not be interleaved.
pub struct Navigator<'a, M: RawMutex, BUS> {
    bus: &'a Mutex<M, BUS>,
    cpi: u8,
    initialised: bool,
    curve: TransferCurve,
    sub_pixel: SubPixel,
}

impl<'a, M: RawMutex, BUS: I2c> Navigator<'a, M, BUS> {
    /// Wraps a shared I2C bus as a trackball. Call [`init`](Self::init) first.
    #[must_use]
    pub const fn new(bus: &'a Mutex<M, BUS>) -> Self {
        Self {
            bus,
            cpi: proto::CPI_DEFAULT,
            initialised: false,
            curve: proto::DEFAULT_CURVE,
            sub_pixel: SubPixel::new(),
        }
    }

    /// Replaces the speed-to-gain curve.
    ///
    /// The carried sub-pixel remainder is dropped along with it — it was
    /// measured under the old gain and means nothing under the new one.
    pub const fn set_curve(&mut self, curve: TransferCurve) {
        self.curve = curve;
        self.sub_pixel.reset();
    }

    /// Runs one SPI transaction through the bridge, returning the sensor's reply.
    ///
    /// Write the frame, wait for the bridge to clock it out on SPI, then read
    /// the receive buffer back. The delay is not optional: read too early and
    /// the buffer still holds the previous transaction.
    ///
    /// The bus lock is held across all three steps. A per-transaction lock
    /// would let the matrix scan or the RGB frame writes slip in during the
    /// settle delay and leave the bridge's buffer holding someone else's
    /// traffic — which reads back as the pointer jumping, not as lag.
    async fn spi_tx(&mut self, frame: [u8; FRAME_LEN]) -> Result<u8, BUS::Error> {
        let mut buf = frame;
        let mut bus = self.bus.lock().await;
        bus.transaction(NAVIGATOR_ADDR, &mut [Operation::Write(&frame)])
            .await?;
        Timer::after(Duration::from_micros(SETTLE_US_PER_BYTE * FRAME_LEN as u64)).await;
        bus.transaction(NAVIGATOR_ADDR, &mut [Operation::Read(&mut buf)])
            .await?;
        Ok(buf[RESPONSE_INDEX])
    }

    /// Reads one sensor register.
    async fn read_reg(&mut self, reg: u8) -> Result<u8, BUS::Error> {
        self.spi_tx(proto::read_frame(reg)).await
    }

    /// Configures the bridge's SPI port, then brings up the sensor.
    ///
    /// Returns [`NavigatorError::SensorId`] if the sensor does not report
    /// [`paw3805ek::PRODUCT_ID`], rather than carrying on and silently
    /// reporting no motion forever.
    pub async fn init(&mut self) -> Result<(), NavigatorError<BUS::Error>> {
        self.initialised = false;

        self.bus
            .lock()
            .await
            .write(NAVIGATOR_ADDR, &proto::bridge_config_frame())
            .await
            .map_err(NavigatorError::Bus)?;
        Timer::after_millis(BRIDGE_SETTLE_MS).await;

        for (i, &frame) in CONFIG_SEQUENCE.iter().enumerate() {
            let response = self.spi_tx(frame).await.map_err(NavigatorError::Bus)?;
            Timer::after_millis(CONFIG_STEP_MS).await;
            if i == CONFIG_ID_CHECK_INDEX && response != paw3805ek::PRODUCT_ID {
                return Err(NavigatorError::SensorId(response));
            }
        }

        self.apply_cpi().await.map_err(NavigatorError::Bus)?;
        self.initialised = true;
        Ok(())
    }

    async fn apply_cpi(&mut self) -> Result<(), BUS::Error> {
        for frame in proto::cpi_sequence(self.cpi) {
            self.spi_tx(frame).await?;
        }
        Ok(())
    }

    /// Current CPI setting.
    #[must_use]
    pub const fn cpi(&self) -> u8 {
        self.cpi
    }

    /// Raises the CPI one step and pushes it to the sensor.
    pub async fn increase_cpi(&mut self) -> Result<(), BUS::Error> {
        self.cpi = proto::cpi_increase(self.cpi);
        self.apply_cpi().await
    }

    /// Lowers the CPI one step and pushes it to the sensor.
    pub async fn decrease_cpi(&mut self) -> Result<(), BUS::Error> {
        self.cpi = proto::cpi_decrease(self.cpi);
        self.apply_cpi().await
    }

    /// Reads the accumulated motion, clearing it on the sensor.
    async fn read_motion(&mut self) -> Result<(i16, i16), BUS::Error> {
        let x_lo = self.read_reg(paw3805ek::REG_DELTA_X_LO).await?;
        let y_lo = self.read_reg(paw3805ek::REG_DELTA_Y_LO).await?;
        let x_hi = self.read_reg(paw3805ek::REG_DELTA_X_HI).await?;
        let y_hi = self.read_reg(paw3805ek::REG_DELTA_Y_HI).await?;
        Ok((proto::delta(x_lo, x_hi), proto::delta(y_lo, y_hi)))
    }

    /// Polls once, returning unscaled sensor counts if motion is pending.
    ///
    /// Reads and clears the same accumulated motion as [`poll`](Self::poll),
    /// without applying the pointer transfer curve or its sub-pixel remainder.
    /// Use this for controls whose response should not depend on pointer speed.
    pub async fn poll_raw(&mut self) -> Result<Option<(i16, i16)>, BUS::Error> {
        let status = self.read_reg(paw3805ek::REG_MOTION).await?;
        if !proto::motion_pending(status) {
            return Ok(None);
        }
        self.read_motion().await.map(Some)
    }

    /// Polls once, returning pointer movement if the ball moved.
    ///
    /// The raw sensor delta is passed through the transfer curve, so this is
    /// pointer units and not device counts. `Ok(None)` means the ball did not
    /// move; an error means the link dropped.
    pub async fn poll(&mut self) -> Result<Option<(i16, i16)>, BUS::Error> {
        let Some((raw_dx, raw_dy)) = self.poll_raw().await? else {
            return Ok(None);
        };
        match self.sub_pixel.apply(&self.curve, raw_dx, raw_dy) {
            // Motion entirely absorbed into the sub-pixel remainder. Reporting
            // it would emit an all-zero HID report that moves nothing.
            (0, 0) => Ok(None),
            moved => Ok(Some(moved)),
        }
    }
}

impl<M: RawMutex, BUS: I2c> AsyncPointingScanner for Navigator<'_, M, BUS> {
    /// Polls until the ball actually moves.
    ///
    /// A dropped link is not fatal here: the module lives on a cable that can
    /// be unplugged, so this re-runs [`init`](Self::init) on the probe interval
    /// until it comes back rather than ending the pointing task for good.
    async fn scan(&mut self) -> PointingEvent {
        loop {
            // At the top of the loop, not only on the idle path. With the delay
            // in the no-motion arm alone, a moving ball made this return
            // instantly, the caller re-entered immediately, and the poll ran
            // back-to-back at bus speed — five bus round-trips per iteration
            // with the mutex held, starving the matrix scan exactly while the
            // ball was being used. USB HID polls at 1ms; there is nothing to
            // gain from sampling faster than the sensor is read.
            Timer::after_millis(POLL_MS).await;

            if !self.initialised && self.init().await.is_err() {
                Timer::after_millis(PROBE_MS).await;
                continue;
            }

            match self.poll().await {
                Ok(Some((dx, dy))) => {
                    // Passed through at full width: `pointing_event_to_report`
                    // clamps to the HID report's i8, so narrowing here would
                    // only duplicate it.
                    return PointingEvent {
                        dx,
                        dy,
                        buttons: 0,
                        scroll: 0,
                    };
                }
                Ok(None) => {}
                Err(_) => {
                    self.initialised = false;
                    Timer::after_millis(PROBE_MS).await;
                }
            }
        }
    }
}
