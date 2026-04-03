//! Matrix scanner abstraction.
//!
//! [`MatrixScanner`] is the hardware boundary trait. Implementations read
//! physical switch state via GPIO and return a [`MatrixState`].

use crate::event::KeyPos;

/// The raw pressed/released state of every switch in the matrix.
///
/// Internally stored as a row-major 2-D boolean array. All access goes
/// through safe [`get`](MatrixState::get) / [`set`](MatrixState::set)
/// methods — no direct indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixState<const ROWS: usize, const COLS: usize> {
    data: [[bool; COLS]; ROWS],
}

impl<const ROWS: usize, const COLS: usize> MatrixState<ROWS, COLS> {
    /// Creates a new [`MatrixState`] with all keys released.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            data: [[false; COLS]; ROWS],
        }
    }

    /// Returns the pressed state of the key at `(row, col)`, or `None` if
    /// the position is out of bounds.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<bool> {
        self.data.get(row).and_then(|r| r.get(col)).copied()
    }

    /// Returns the pressed state of the key at `pos`, or `None` if out of
    /// bounds.
    #[must_use]
    pub fn get_pos(&self, pos: KeyPos) -> Option<bool> {
        self.get(usize::from(pos.row), usize::from(pos.col))
    }

    /// Sets the state of the key at `(row, col)`.
    ///
    /// Returns `true` if the position was valid, `false` if out of bounds.
    pub fn set(&mut self, row: usize, col: usize, pressed: bool) -> bool {
        if let Some(r) = self.data.get_mut(row) {
            if let Some(cell) = r.get_mut(col) {
                *cell = pressed;
                return true;
            }
        }
        false
    }

    /// Iterates over every `(row, col, pressed)` triple in row-major order.
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize, bool)> + '_ {
        self.data.iter().enumerate().flat_map(|(r, row)| {
            row.iter()
                .enumerate()
                .map(move |(c, &pressed)| (r, c, pressed))
        })
    }

    /// Iterates over `(KeyPos, pressed)` pairs in row-major order.
    pub fn iter_pos(&self) -> impl Iterator<Item = (KeyPos, bool)> + '_ {
        self.iter().map(|(r, c, p)| {
            let row = r.try_into().unwrap_or(u8::MAX);
            let col = c.try_into().unwrap_or(u8::MAX);
            (KeyPos::new(row, col), p)
        })
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
    fn set_valid_position_returns_true() {
        let mut state = MatrixState::<4, 12>::new();
        assert!(state.set(0, 0, true));
    }

    #[test]
    fn set_out_of_bounds_row_returns_false() {
        let mut state = MatrixState::<4, 12>::new();
        assert!(!state.set(4, 0, true));
    }

    #[test]
    fn set_out_of_bounds_col_returns_false() {
        let mut state = MatrixState::<4, 12>::new();
        assert!(!state.set(0, 12, true));
    }

    #[test]
    fn set_and_get_round_trips() {
        let mut state = MatrixState::<4, 12>::new();
        state.set(2, 5, true);
        assert_eq!(state.get(2, 5), Some(true));
    }

    #[test]
    fn set_false_clears_previously_set_key() {
        let mut state = MatrixState::<4, 12>::new();
        state.set(1, 1, true);
        state.set(1, 1, false);
        assert_eq!(state.get(1, 1), Some(false));
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let state = MatrixState::<4, 12>::new();
        assert_eq!(state.get(4, 0), None);
        assert_eq!(state.get(0, 12), None);
    }

    #[test]
    fn get_pos_matches_get_with_row_and_col() {
        let mut state = MatrixState::<4, 12>::new();
        state.set(2, 3, true);
        let pos = KeyPos::new(2, 3);
        assert_eq!(state.get_pos(pos), state.get(2, 3));
    }

    #[test]
    fn get_pos_out_of_bounds_returns_none() {
        let state = MatrixState::<4, 12>::new();
        assert_eq!(state.get_pos(KeyPos::new(4, 0)), None);
    }

    #[test]
    fn iter_yields_rows_times_cols_entries() {
        let state = MatrixState::<3, 5>::new();
        assert_eq!(state.iter().count(), 15);
    }

    #[test]
    fn iter_yields_set_key_as_pressed() {
        let mut state = MatrixState::<3, 5>::new();
        state.set(1, 2, true);
        let found = state.iter().find(|&(r, c, _)| r == 1 && c == 2);
        assert_eq!(found, Some((1, 2, true)));
    }

    #[test]
    fn equality_reflects_state_differences() {
        let mut a = MatrixState::<2, 2>::new();
        let b = MatrixState::<2, 2>::new();
        assert_eq!(a, b);
        a.set(0, 0, true);
        assert_ne!(a, b);
    }

    #[test]
    fn iter_pos_yields_rows_times_cols_entries() {
        let state = MatrixState::<3, 5>::new();
        assert_eq!(state.iter_pos().count(), 15);
    }

    #[test]
    fn iter_pos_yields_correct_key_pos() {
        let mut state = MatrixState::<3, 5>::new();
        state.set(1, 2, true);
        let found = state.iter_pos().find(|&(pos, _)| pos == KeyPos::new(1, 2));
        assert_eq!(found, Some((KeyPos::new(1, 2), true)));
    }

    #[test]
    fn iter_pos_all_released_on_new_state() {
        let state = MatrixState::<2, 3>::new();
        assert!(state.iter_pos().all(|(_, pressed)| !pressed));
    }
}
