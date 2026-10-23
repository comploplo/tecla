//! Production NOS75 GPIO scanning: idle rows are inputs, selected row is low.
use crate::{scan, COLS, ROWS};
use core::convert::Infallible;
use embassy_stm32::gpio::{Flex, Input, Pull, Speed};
use embassy_time::{block_for, Duration};
use tecla::matrix::{AsyncMatrixScanner, MatrixState};

/// Matrix pins in the production firmware's table order.
pub struct Nos75Matrix<'d> {
    rows: [Flex<'d>; ROWS],
    cols: [Input<'d>; COLS],
}

impl<'d> Nos75Matrix<'d> {
    /// Take row/column pins; release every row to input/pull-up before scanning.
    #[must_use]
    pub fn new(mut rows: [Flex<'d>; ROWS], cols: [Input<'d>; COLS]) -> Self {
        for row in &mut rows {
            row.set_as_input(Pull::Up);
        }
        Self { rows, cols }
    }

    /// Read one complete matrix without yielding (approximately 78 µs settling).
    pub fn scan_now(&mut self) -> MatrixState<ROWS, COLS> {
        let Self { rows, cols } = self;
        scan::scan(
            |row, selected| {
                if selected {
                    rows[row].set_low(); // Preload before enabling the output.
                    rows[row].set_as_output(Speed::Low);
                } else {
                    rows[row].set_as_input(Pull::Up);
                }
            },
            |col| cols[col].is_low(),
            |us| block_for(Duration::from_micros(us)),
        )
    }
}

impl AsyncMatrixScanner<ROWS, COLS> for Nos75Matrix<'_> {
    type Error = Infallible;
    async fn scan(&mut self) -> Result<MatrixState<ROWS, COLS>, Self::Error> {
        Ok(self.scan_now())
    }
}
