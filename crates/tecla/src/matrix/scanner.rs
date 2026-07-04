//! Matrix scanner abstraction.
//!
//! [`MatrixScanner`] is the hardware boundary trait. Implementations read
//! physical switch state via GPIO and return a [`MatrixState`].

/// The raw pressed/released state of every switch in the matrix.
///
/// Internally stored as a row-major 2-D boolean array. All access goes
/// through safe [`get`](MatrixState::get) / [`set`](MatrixState::set)
/// methods — no direct indexing.
pub type MatrixState<const ROWS: usize, const COLS: usize> =
    crate::matrix::state::Matrix<bool, ROWS, COLS>;

impl<const ROWS: usize, const COLS: usize> MatrixState<ROWS, COLS> {
    /// Creates a new [`MatrixState`] with all keys released.
    #[must_use]
    pub const fn new() -> Self {
        Self::from_data([[false; COLS]; ROWS])
    }
}

impl<const ROWS: usize, const COLS: usize> Default for MatrixState<ROWS, COLS> {
    fn default() -> Self {
        Self::new()
    }
}

/// Hardware abstraction for reading the key switch matrix.
///
/// Implementations drive row/column GPIO lines and return the raw
/// [`MatrixState`] each scan cycle.
pub trait MatrixScanner<const ROWS: usize, const COLS: usize> {
    /// The error type produced if the scan fails.
    type Error;

    /// Performs one full matrix scan and returns the current raw state.
    fn scan(&mut self) -> Result<MatrixState<ROWS, COLS>, Self::Error>;
}

/// Async hardware abstraction for reading the key switch matrix.
///
/// Like [`MatrixScanner`] but yields until a scan completes — typically
/// waiting for a GPIO interrupt or a polled timer. Used by [`crate::task::run`].
///
/// `async fn` in traits is used intentionally here — tecla targets single-threaded
/// Embassy executors where `Send` bounds on futures are not required.
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait AsyncMatrixScanner<const ROWS: usize, const COLS: usize> {
    /// The error type produced if the scan fails.
    type Error;

    /// Waits for the next matrix scan and returns the raw state.
    async fn scan(&mut self) -> Result<MatrixState<ROWS, COLS>, Self::Error>;
}

/// Row-strobe / column-read GPIO matrix scanner using `embedded-hal`.
pub struct GpioMatrixScanner<R, C, const ROWS: usize, const COLS: usize> {
    rows: [R; ROWS],
    cols: [C; COLS],
}

impl<R, C, const ROWS: usize, const COLS: usize> GpioMatrixScanner<R, C, ROWS, COLS>
where
    R: embedded_hal::digital::OutputPin,
    C: embedded_hal::digital::InputPin,
{
    /// Creates a new scanner from row output pins and column input pins.
    #[must_use]
    pub fn new(rows: [R; ROWS], cols: [C; COLS]) -> Self {
        Self { rows, cols }
    }
}

impl<R, C, const ROWS: usize, const COLS: usize> MatrixScanner<ROWS, COLS>
    for GpioMatrixScanner<R, C, ROWS, COLS>
where
    R: embedded_hal::digital::OutputPin,
    C: embedded_hal::digital::InputPin,
{
    type Error = R::Error;

    fn scan(&mut self) -> Result<MatrixState<ROWS, COLS>, Self::Error> {
        let mut state = MatrixState::new();
        for (r, row_pin) in self.rows.iter_mut().enumerate() {
            row_pin.set_low()?;
            for (c, col_pin) in self.cols.iter_mut().enumerate() {
                if let Ok(pressed) = col_pin.is_low() {
                    state.set(r, c, pressed);
                }
            }
            row_pin.set_high()?;
        }
        Ok(state)
    }
}

#[cfg(feature = "embassy")]
impl<R, C, const ROWS: usize, const COLS: usize> AsyncMatrixScanner<ROWS, COLS>
    for GpioMatrixScanner<R, C, ROWS, COLS>
where
    R: embedded_hal::digital::OutputPin,
    C: embedded_hal::digital::InputPin,
{
    type Error = R::Error;

    async fn scan(&mut self) -> Result<MatrixState<ROWS, COLS>, Self::Error> {
        let mut state = MatrixState::new();
        for (r, row_pin) in self.rows.iter_mut().enumerate() {
            row_pin.set_low()?;
            embassy_time::Timer::after_micros(10).await;
            for (c, col_pin) in self.cols.iter_mut().enumerate() {
                if let Ok(pressed) = col_pin.is_low() {
                    state.set(r, c, pressed);
                }
            }
            row_pin.set_high()?;
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_matrix_state_has_all_keys_released() {
        let state = MatrixState::<4, 12>::new();
        for (_, _, pressed) in state.iter() {
            assert!(!pressed);
        }
    }

    #[test]
    fn default_matrix_state_equals_new() {
        let a = MatrixState::<3, 6>::new();
        let b = MatrixState::<3, 6>::default();
        assert_eq!(a, b);
    }

    #[test]
    fn new_matrix_state_set_and_get_round_trips() {
        let mut state = MatrixState::<4, 12>::new();
        state.set(2, 5, true);
        assert_eq!(state.get(2, 5), Some(true));
    }
}
