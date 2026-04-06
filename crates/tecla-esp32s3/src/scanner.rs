//! GPIO matrix scanner for ESP32-S3.
//!
//! [`GpioMatrixScanner`] implements [`AsyncMatrixScanner`] using esp-hal
//! GPIO. Rows are driven low one at a time; columns are read with internal
//! pull-ups. A column reads low when the switch at that row/col intersection
//! is pressed (active-low matrix).

use embassy_time::Timer;
use esp_hal::gpio::{Input, Output};
use tecla::matrix::{AsyncMatrixScanner, MatrixState};

/// Direct-pin scanner for keyboards where each key has its own GPIO pin.
///
/// Unlike [`GpioMatrixScanner`], there is no row/column multiplexing — each
/// key is wired directly to a separate input pin with an internal pull-up.
/// A pressed key pulls the pin low.
///
/// The scanner maps keys into a `1 × KEYS` matrix (single row).
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

/// Row-strobe / column-read GPIO matrix scanner.
///
/// # Wiring
///
/// - **Rows** — output pins, idle high. Each row is briefly driven low during
///   its scan slot. Pass them as [`Output`] pins initialised to
///   [`esp_hal::gpio::Level::High`].
/// - **Columns** — input pins with internal pull-ups enabled. A pressed key
///   pulls the column low, so `is_low()` → `true` means pressed.
///
/// # Usage
///
/// ```rust,ignore
/// use esp_hal::gpio::{Input, Level, Output, Pull};
/// use tecla_esp32s3::scanner::GpioMatrixScanner;
///
/// let rows = [
///     Output::new(peripherals.GPIO0, Level::High),
///     Output::new(peripherals.GPIO1, Level::High),
/// ];
/// let cols = [
///     Input::new(peripherals.GPIO4, Pull::Up),
///     Input::new(peripherals.GPIO5, Pull::Up),
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
