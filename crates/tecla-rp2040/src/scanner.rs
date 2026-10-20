//! GPIO matrix scanner for RP2040.
//!
//! [`GpioMatrixScanner`] implements [`AsyncMatrixScanner`] using embassy-rp
//! GPIO. Rows are driven low one at a time; columns are read with internal
//! pull-ups. A column reads low when the switch at that row/col intersection
//! is pressed (active-low matrix).

use embassy_rp::gpio::{Drive, Flex, Input, Output, Pull};
use embassy_time::Timer;
use tecla::matrix::capacitive::CapacitiveFilter;
use tecla::matrix::{AsyncMatrixScanner, MatrixState};

/// Direct-pin scanner for keyboards where each key has its own GPIO pin.
///
/// Unlike [`GpioMatrixScanner`], there is no row/column multiplexing — each
/// key is wired directly to a separate input pin with an internal pull-up.
/// A pressed key pulls the pin low.
///
/// The scanner maps keys into a `1 × KEYS` matrix (single row).
///
/// # Usage
///
/// ```rust,ignore
/// use embassy_rp::gpio::{AnyPin, Input, Pull};
/// use tecla_rp2040::scanner::DirectPinScanner;
///
/// let pins = [
///     Input::new(AnyPin::from(p.PIN_0), Pull::Up),
///     Input::new(AnyPin::from(p.PIN_1), Pull::Up),
///     Input::new(AnyPin::from(p.PIN_2), Pull::Up),
/// ];
/// let scanner = DirectPinScanner::new(pins);
/// ```
pub struct DirectPinScanner<'d, const KEYS: usize> {
    pins: [Input<'d>; KEYS],
}

impl<'d, const KEYS: usize> DirectPinScanner<'d, KEYS> {
    /// Creates a new scanner from an array of input pins.
    ///
    /// Each pin must have pull-ups enabled. A low reading means pressed.
    #[must_use]
    pub fn new(pins: [Input<'d>; KEYS]) -> Self {
        Self { pins }
    }
}

impl<'d, const KEYS: usize> AsyncMatrixScanner<1, KEYS> for DirectPinScanner<'d, KEYS> {
    type Error = core::convert::Infallible;

    async fn scan(&mut self) -> Result<MatrixState<1, KEYS>, Self::Error> {
        let mut state = MatrixState::new();
        for (i, pin) in self.pins.iter().enumerate() {
            state.set(0, i, pin.is_low());
        }
        Ok(state)
    }
}

/// Capacitive touch scanner for keyboards with touch-sensitive pads.
///
/// Measures discharge time per pin: charge all pins high, switch each to
/// input sequentially, count cycles until low. A finger adds capacitance,
/// increasing the count. Calibrates at boot (4 untouched rounds), then uses
/// per-pad hysteresis, two-scan edge confirmation and guarded idle drift
/// tracking. Maps keys into a `1 × KEYS` matrix (single row).
pub struct CapTouchScanner<'d, const KEYS: usize> {
    pins: [Flex<'d>; KEYS],
    filter: CapacitiveFilter<KEYS>,
}

impl<'d, const KEYS: usize> CapTouchScanner<'d, KEYS> {
    const CHARGE_CYCLES: u32 = 1250;
    const MAX_DISCHARGE_COUNT: u32 = 100_000;
    const CALIBRATION_ROUNDS: usize = 4;

    /// Creates a new scanner, calibrating touch thresholds at boot.
    pub fn new(mut pins: [Flex<'d>; KEYS]) -> Self {
        for pin in &mut pins {
            pin.set_pull(Pull::None);
            pin.set_drive_strength(Drive::_12mA);
        }
        let mut scanner = Self {
            pins,
            filter: CapacitiveFilter::new([0; KEYS], [0; KEYS]),
        };
        scanner.calibrate();
        scanner
    }

    fn calibrate(&mut self) {
        let mut accum = [0u64; KEYS];
        let mut minimum = [u32::MAX; KEYS];
        let mut maximum = [0u32; KEYS];
        for _ in 0..Self::CALIBRATION_ROUNDS {
            let counts = self.read_counts();
            for (index, &count) in counts.iter().enumerate() {
                accum[index] += u64::from(count);
                minimum[index] = minimum[index].min(count);
                maximum[index] = maximum[index].max(count);
            }
        }
        let baselines =
            core::array::from_fn(|index| (accum[index] / Self::CALIBRATION_ROUNDS as u64) as u32);
        let noise = core::array::from_fn(|index| maximum[index].saturating_sub(minimum[index]));
        self.filter = CapacitiveFilter::new(baselines, noise);
    }

    fn charge_all(&mut self) {
        for pin in &mut self.pins {
            pin.set_as_output();
            pin.set_high();
        }
        for _ in 0..Self::CHARGE_CYCLES {
            cortex_m::asm::nop();
        }
    }

    fn read_counts(&mut self) -> [u32; KEYS] {
        self.charge_all();
        let mut counts = [0u32; KEYS];
        for (i, pin) in self.pins.iter_mut().enumerate() {
            pin.set_as_input();
            let mut counter = 0u32;
            while counter < Self::MAX_DISCHARGE_COUNT {
                if pin.is_low() {
                    break;
                }
                counter += 1;
            }
            counts[i] = counter;
        }
        counts
    }
}

impl<'d, const KEYS: usize> AsyncMatrixScanner<1, KEYS> for CapTouchScanner<'d, KEYS> {
    type Error = core::convert::Infallible;

    async fn scan(&mut self) -> Result<MatrixState<1, KEYS>, Self::Error> {
        let counts = self.read_counts();
        let mut state = MatrixState::new();
        for (i, pressed) in self.filter.update(&counts).into_iter().enumerate() {
            state.set(0, i, pressed);
        }
        Ok(state)
    }
}

/// Row-strobe / column-read GPIO matrix scanner.
///
/// # Wiring
///
/// - **Rows** — output pins, idle high. Each row is briefly driven low during
///   its scan slot. Pass them as [`Output`] pins initialised to
///   [`embassy_rp::gpio::Level::High`].
/// - **Columns** — input pins with internal pull-ups enabled. A pressed key
///   pulls the column low, so `is_low()` → `true` means pressed.
///
/// # Usage
///
/// ```rust,ignore
/// use embassy_rp::gpio::{AnyPin, Input, Level, Output, Pull};
/// use tecla_rp2040::scanner::GpioMatrixScanner;
///
/// let rows = [
///     Output::new(AnyPin::from(p.PIN_0), Level::High),
///     Output::new(AnyPin::from(p.PIN_1), Level::High),
/// ];
/// let cols = [
///     Input::new(AnyPin::from(p.PIN_4), Pull::Up),
///     Input::new(AnyPin::from(p.PIN_5), Pull::Up),
/// ];
/// let scanner = GpioMatrixScanner::new(rows, cols);
/// ```
pub struct GpioMatrixScanner<'d, const ROWS: usize, const COLS: usize> {
    rows: [Output<'d>; ROWS],
    cols: [Input<'d>; COLS],
}

impl<'d, const ROWS: usize, const COLS: usize> GpioMatrixScanner<'d, ROWS, COLS> {
    /// Creates a new scanner from row output pins and column input pins.
    ///
    /// Rows must already be initialised to high (idle). Columns must already
    /// have pull-ups enabled.
    #[must_use]
    pub fn new(rows: [Output<'d>; ROWS], cols: [Input<'d>; COLS]) -> Self {
        Self { rows, cols }
    }
}

impl<'d, const ROWS: usize, const COLS: usize> AsyncMatrixScanner<ROWS, COLS>
    for GpioMatrixScanner<'d, ROWS, COLS>
{
    type Error = core::convert::Infallible;

    async fn scan(&mut self) -> Result<MatrixState<ROWS, COLS>, Self::Error> {
        let mut state = MatrixState::new();
        for (r, row_pin) in self.rows.iter_mut().enumerate() {
            row_pin.set_low();
            Timer::after_micros(10).await;
            for (c, col_pin) in self.cols.iter().enumerate() {
                state.set(r, c, col_pin.is_low());
            }
            row_pin.set_high();
        }
        Ok(state)
    }
}
