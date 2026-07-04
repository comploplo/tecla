//! Shared row-major matrix storage backing [`super::scanner::MatrixState`]
//! and [`super::analog::AnalogMatrixState`].
//!
//! The two public types differ only in cell type (`bool` vs `u8`) — this
//! module holds the one copy of `get`/`get_pos`/`set`/`iter`/`iter_pos`
//! shared between them. `new()` stays per-type (see [`Matrix::from_data`]).

use crate::pipeline::event::KeyPos;

/// Generic row-major 2-D matrix storage. All access goes through safe
/// [`get`](Matrix::get) / [`set`](Matrix::set) methods — no direct indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix<T, const ROWS: usize, const COLS: usize> {
    data: [[T; COLS]; ROWS],
}

impl<T: Copy, const ROWS: usize, const COLS: usize> Matrix<T, ROWS, COLS> {
    /// Builds a [`Matrix`] directly from a caller-supplied array.
    ///
    /// A generic zero value can't be produced in a `const fn` on stable Rust
    /// without an unstable const-`Default` bound, so this just moves the
    /// array in; each concrete alias (`MatrixState`, `AnalogMatrixState`)
    /// builds its own `const fn new()` on top of this.
    pub(crate) const fn from_data(data: [[T; COLS]; ROWS]) -> Self {
        Self { data }
    }

    /// Returns the value of the cell at `(row, col)`, or `None` if the
    /// position is out of bounds.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<T> {
        self.data.get(row).and_then(|r| r.get(col)).copied()
    }

    /// Returns the value of the cell at `pos`, or `None` if out of bounds.
    #[must_use]
    pub fn get_pos(&self, pos: KeyPos) -> Option<T> {
        self.get(usize::from(pos.row), usize::from(pos.col))
    }

    /// Sets the value of the cell at `(row, col)`.
    ///
    /// Returns `true` if the position was valid, `false` if out of bounds.
    pub fn set(&mut self, row: usize, col: usize, value: T) -> bool {
        if let Some(r) = self.data.get_mut(row) {
            if let Some(cell) = r.get_mut(col) {
                *cell = value;
                return true;
            }
        }
        false
    }

    /// Iterates over every `(row, col, value)` triple in row-major order.
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize, T)> + '_ {
        self.data
            .iter()
            .enumerate()
            .flat_map(|(r, row)| row.iter().enumerate().map(move |(c, &value)| (r, c, value)))
    }

    /// Iterates over `(KeyPos, value)` pairs in row-major order.
    pub fn iter_pos(&self) -> impl Iterator<Item = (KeyPos, T)> + '_ {
        self.iter().map(|(r, c, v)| {
            let row = r.try_into().unwrap_or(u8::MAX);
            let col = c.try_into().unwrap_or(u8::MAX);
            (KeyPos::new(row, col), v)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix<const ROWS: usize, const COLS: usize>() -> Matrix<u8, ROWS, COLS> {
        Matrix::from_data([[0; COLS]; ROWS])
    }

    #[test]
    fn new_matrix_has_all_cells_zeroed() {
        let m = matrix::<4, 12>();
        for (_, _, value) in m.iter() {
            assert_eq!(value, 0);
        }
    }

    #[test]
    fn set_valid_position_returns_true() {
        let mut m = matrix::<4, 12>();
        assert!(m.set(0, 0, 1));
    }

    #[test]
    fn set_out_of_bounds_row_returns_false() {
        let mut m = matrix::<4, 12>();
        assert!(!m.set(4, 0, 1));
    }

    #[test]
    fn set_out_of_bounds_col_returns_false() {
        let mut m = matrix::<4, 12>();
        assert!(!m.set(0, 12, 1));
    }

    #[test]
    fn set_and_get_round_trips() {
        let mut m = matrix::<4, 12>();
        m.set(2, 5, 200);
        assert_eq!(m.get(2, 5), Some(200));
    }

    #[test]
    fn set_zero_clears_previously_set_cell() {
        let mut m = matrix::<4, 12>();
        m.set(1, 1, 9);
        m.set(1, 1, 0);
        assert_eq!(m.get(1, 1), Some(0));
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let m = matrix::<4, 12>();
        assert_eq!(m.get(4, 0), None);
        assert_eq!(m.get(0, 12), None);
    }

    #[test]
    fn get_pos_matches_get_with_row_and_col() {
        let mut m = matrix::<4, 12>();
        m.set(2, 3, 77);
        let pos = KeyPos::new(2, 3);
        assert_eq!(m.get_pos(pos), m.get(2, 3));
    }

    #[test]
    fn get_pos_out_of_bounds_returns_none() {
        let m = matrix::<4, 12>();
        assert_eq!(m.get_pos(KeyPos::new(4, 0)), None);
    }

    #[test]
    fn iter_yields_rows_times_cols_entries() {
        let m = matrix::<3, 5>();
        assert_eq!(m.iter().count(), 15);
    }

    #[test]
    fn iter_yields_set_cell_value() {
        let mut m = matrix::<3, 5>();
        m.set(1, 2, 99);
        let found = m.iter().find(|&(r, c, _)| r == 1 && c == 2);
        assert_eq!(found, Some((1, 2, 99)));
    }

    #[test]
    fn equality_reflects_state_differences() {
        let mut a = matrix::<2, 2>();
        let b = matrix::<2, 2>();
        assert_eq!(a, b);
        a.set(0, 0, 1);
        assert_ne!(a, b);
    }

    #[test]
    fn iter_pos_yields_rows_times_cols_entries() {
        let m = matrix::<3, 5>();
        assert_eq!(m.iter_pos().count(), 15);
    }

    #[test]
    fn iter_pos_yields_correct_key_pos() {
        let mut m = matrix::<3, 5>();
        m.set(1, 2, 99);
        let found = m.iter_pos().find(|&(pos, _)| pos == KeyPos::new(1, 2));
        assert_eq!(found, Some((KeyPos::new(1, 2), 99)));
    }
}
