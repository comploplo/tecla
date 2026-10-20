//! Matrix scanner abstraction.
//!
//! [`MatrixScanner`] is the hardware boundary trait. Implementations read
//! physical switch state via GPIO and return a [`MatrixState`].

/// The raw pressed/released state of every switch in the matrix.
///
/// Internally stored as a row-major 2-D boolean array. All access goes
/// through safe `get` / `set`
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

    // ── GpioMatrixScanner ────────────────────────────────────────────────────
    //
    // Fake pins that record the strobe sequence, so the row-strobe /
    // column-read protocol itself is exercised on the host.

    use core::cell::RefCell;

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct FakeError;

    impl embedded_hal::digital::Error for FakeError {
        fn kind(&self) -> embedded_hal::digital::ErrorKind {
            embedded_hal::digital::ErrorKind::Other
        }
    }

    /// A row pin. Records how many times it was strobed low.
    struct FakeRow<'a> {
        index: usize,
        /// Which row is currently strobed low, shared by every pin.
        active: &'a RefCell<Option<usize>>,
        strobes: &'a RefCell<[u8; 3]>,
        fail: bool,
    }

    impl embedded_hal::digital::ErrorType for FakeRow<'_> {
        type Error = FakeError;
    }

    impl embedded_hal::digital::OutputPin for FakeRow<'_> {
        fn set_low(&mut self) -> Result<(), FakeError> {
            if self.fail {
                return Err(FakeError);
            }
            *self.active.borrow_mut() = Some(self.index);
            self.strobes.borrow_mut()[self.index] += 1;
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), FakeError> {
            if self.fail {
                return Err(FakeError);
            }
            *self.active.borrow_mut() = None;
            Ok(())
        }
    }

    /// A column pin. Reads low (pressed) only for the `(row, col)` pairs in
    /// `pressed`, and only while that row is the one strobed low.
    struct FakeCol<'a> {
        index: usize,
        active: &'a RefCell<Option<usize>>,
        pressed: &'a [(usize, usize)],
    }

    impl embedded_hal::digital::ErrorType for FakeCol<'_> {
        type Error = FakeError;
    }

    impl embedded_hal::digital::InputPin for FakeCol<'_> {
        fn is_low(&mut self) -> Result<bool, FakeError> {
            let row = self.active.borrow().ok_or(FakeError)?;
            Ok(self.pressed.contains(&(row, self.index)))
        }

        fn is_high(&mut self) -> Result<bool, FakeError> {
            self.is_low().map(|low| !low)
        }
    }

    #[test]
    fn gpio_scan_reports_exactly_the_pressed_keys() {
        let active = RefCell::new(None);
        let strobes = RefCell::new([0u8; 3]);
        let pressed = [(0usize, 1usize), (2, 0)];

        let rows = [0, 1, 2].map(|index| FakeRow {
            index,
            active: &active,
            strobes: &strobes,
            fail: false,
        });
        let cols = [0, 1].map(|index| FakeCol {
            index,
            active: &active,
            pressed: &pressed,
        });

        let mut scanner = GpioMatrixScanner::new(rows, cols);
        // Fully qualified: with the `embassy` feature on, AsyncMatrixScanner
        // also has a `scan` method on this type.
        let state = MatrixScanner::scan(&mut scanner).expect("scan failed");

        for r in 0..3 {
            for c in 0..2 {
                assert_eq!(
                    state.get(r, c),
                    Some(pressed.contains(&(r, c))),
                    "wrong state at ({r},{c})"
                );
            }
        }
        // Every row strobed exactly once, and the last one was released.
        assert_eq!(*strobes.borrow(), [1, 1, 1]);
        assert_eq!(*active.borrow(), None, "row left strobed low after scan");
    }

    #[test]
    fn gpio_scan_propagates_row_pin_errors() {
        let active = RefCell::new(None);
        let strobes = RefCell::new([0u8; 3]);
        let pressed: [(usize, usize); 0] = [];

        let rows = [0, 1, 2].map(|index| FakeRow {
            index,
            active: &active,
            strobes: &strobes,
            fail: true,
        });
        let cols = [0, 1].map(|index| FakeCol {
            index,
            active: &active,
            pressed: &pressed,
        });

        let mut scanner = GpioMatrixScanner::new(rows, cols);
        assert_eq!(MatrixScanner::scan(&mut scanner), Err(FakeError));
    }
}
