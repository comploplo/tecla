//! NOS75 electrical scan sequence, also exercised with fake pins on the host.
use tecla::matrix::MatrixState;

/// Production matrix row count.
pub const ROWS: usize = 6;
/// Production matrix column count (including unwired positions).
pub const COLS: usize = 17;

/// Select one row low, read active-low columns, then release to input/pull-up.
///
/// All rows must initially be input/pull-up. Delays are conservative bring-up
/// values, not a recovered stock timing calibration.
pub fn scan(
    mut select: impl FnMut(usize, bool),
    mut is_low: impl FnMut(usize) -> bool,
    mut settle_us: impl FnMut(u64),
) -> MatrixState<ROWS, COLS> {
    let mut state = MatrixState::new();
    for row in 0..ROWS {
        select(row, true);
        settle_us(10);
        for col in 0..COLS {
            state.set(row, col, is_low(col));
        }
        select(row, false);
        settle_us(3);
    }
    state
}
