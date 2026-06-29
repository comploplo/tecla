//! ZSA Voyager matrix scanner.
//!
//! The Voyager is a wired split with a single STM32F303 on the left half:
//!
//! - **Left half** — scanned directly over STM32 GPIO. Six row-strobe outputs
//!   (PB10–PB15) are driven **high** one at a time; seven column inputs
//!   (PA0,1,2,3,6,7 and PB0) have pull-downs, so a pressed key reads **high**.
//!   Fills logical matrix rows 0–5.
//! - **Right half** — scanned through an [`Mcp23018`] on I2C1. Port A drives
//!   seven select lines (active-low); port B's low six bits read back (active-
//!   low). The right half is wired transposed relative to the left, so the
//!   raw reads are transposed into logical matrix rows 6–11.
//!
//! This mirrors QMK's `keyboards/zsa/voyager/matrix.c`. If the expander can't
//! be reached the left half keeps working and the right half reports no keys.
//!
//! Pin assignments and the active-high/transpose details are hardware facts
//! taken from QMK — verify against a real board on first bring-up.

use embassy_stm32::gpio::{Input, Output};
use embassy_time::Timer;
use embedded_hal_async::i2c::I2c;
use tecla::matrix::{AsyncMatrixScanner, MatrixState};

use crate::mcp23018::{Mcp23018, Port};

/// Total logical matrix dimensions (both halves combined).
pub const ROWS: usize = 12;
/// Number of matrix columns.
pub const COLS: usize = 7;

const ROWS_PER_HAND: usize = 6;
/// Port B output mask that holds both status-LED bits off (active-low LEDs on
/// bits 6 and 7) while leaving the low six bits as column inputs.
const PORTB_LEDS_OFF: u8 = 0b1100_0000;
/// Settle time after selecting a right-hand row before reading columns.
const RIGHT_SETTLE_US: u64 = 50;

/// Transposes the seven raw port-B reads into the right half's 6×7 block.
///
/// `right_raw[s]` is the (already active-high) 6-bit column read taken while
/// select line `s` was active. Logical right-hand row `r` (0–5) maps to matrix
/// row `11 - r`; logical column `c` (0–6) reads from `right_raw[6 - c]`.
fn assemble_right_half(right_raw: &[u8; COLS]) -> [[bool; COLS]; ROWS_PER_HAND] {
    let mut block = [[false; COLS]; ROWS_PER_HAND];
    for (r, row) in block.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (right_raw[6 - c] >> r) & 1 != 0;
        }
    }
    block
}

/// Voyager matrix scanner: STM32 GPIO left half + MCP23018 right half.
pub struct VoyagerScanner<'d, I2C> {
    left_rows: [Output<'d>; ROWS_PER_HAND],
    left_cols: [Input<'d>; COLS],
    mcp: Mcp23018<I2C>,
    mcp_ok: bool,
    /// Last good right-half column read, held across I2C errors so a transient
    /// bus glitch can't momentarily release held right-hand keys (which would
    /// break the host's key auto-repeat).
    last_right_raw: [u8; COLS],
}

impl<'d, I2C: I2c> VoyagerScanner<'d, I2C> {
    /// Builds the scanner and initialises the expander.
    ///
    /// `left_rows` are PB10–PB15 as outputs idle-low; `left_cols` are the seven
    /// column inputs with pull-downs. `mcp` wraps the I2C bus to the expander.
    ///
    /// The expander is *not* initialised here — doing blocking I2C in the
    /// firmware's `main` before the async executor starts can wedge USB. Instead
    /// the first [`scan`](Self::scan) lazily inits the expander (and retries on
    /// every scan while it's offline), so the left half and USB come up
    /// regardless of the right half.
    pub fn new(
        left_rows: [Output<'d>; ROWS_PER_HAND],
        left_cols: [Input<'d>; COLS],
        mcp: Mcp23018<I2C>,
    ) -> Self {
        Self {
            left_rows,
            left_cols,
            mcp,
            mcp_ok: false,
            last_right_raw: [0u8; COLS],
        }
    }

    /// Port A all outputs (row select); port B low six inputs with pull-ups,
    /// top two outputs. This mirrors QMK's `mcp23018_set_config`, which writes
    /// the same mask to both IODIR and GPPU.
    async fn init_expander(mcp: &mut Mcp23018<I2C>) -> Result<(), I2C::Error> {
        mcp.set_config(Port::A, 0b0000_0000).await?;
        mcp.set_config(Port::B, 0b0011_1111).await?;
        Ok(())
    }

    /// Reads the right half through the expander. Returns `Err` on any I2C
    /// failure so the caller can mark the expander offline.
    async fn scan_right(&mut self) -> Result<[u8; COLS], I2C::Error> {
        let mut right_raw = [0u8; COLS];
        for (step, slot) in right_raw.iter_mut().enumerate() {
            // Active-low select on port A; keep both LED bits off on port B.
            self.mcp
                .set_output(Port::A, 0b0111_1111 & !(1 << step))
                .await?;
            self.mcp.set_output(Port::B, PORTB_LEDS_OFF).await?;
            Timer::after_micros(RIGHT_SETTLE_US).await;
            let rx = self.mcp.read_port(Port::B).await?;
            // Low six bits are columns, active-low → invert to active-high.
            *slot = !rx & 0b0011_1111;
        }
        Ok(right_raw)
    }
}

impl<'d, I2C: I2c> AsyncMatrixScanner<ROWS, COLS> for VoyagerScanner<'d, I2C> {
    type Error = core::convert::Infallible;

    async fn scan(&mut self) -> Result<MatrixState<ROWS, COLS>, Self::Error> {
        let mut state = MatrixState::new();

        // Left half: active-high strobe, pull-down inputs (pressed = high).
        for (r, row_pin) in self.left_rows.iter_mut().enumerate() {
            row_pin.set_high();
            Timer::after_micros(10).await;
            for (c, col_pin) in self.left_cols.iter().enumerate() {
                state.set(r, c, col_pin.is_high());
            }
            row_pin.set_low();
        }

        // Right half via the expander; retry init if it had dropped offline.
        if !self.mcp_ok {
            self.mcp_ok = Self::init_expander(&mut self.mcp).await.is_ok();
        }
        if self.mcp_ok {
            match self.scan_right().await {
                Ok(right_raw) => self.last_right_raw = right_raw,
                // Keep `last_right_raw` (hold state) and re-init next scan.
                // ponytail: during a sustained I2C outage a physically-released
                // key stays "held" until the bus recovers — acceptable vs.
                // dropping held keys on every glitch.
                Err(_) => self.mcp_ok = false,
            }
        }
        let block = assemble_right_half(&self.last_right_raw);
        for (r, row) in block.iter().enumerate() {
            for (c, &pressed) in row.iter().enumerate() {
                state.set(11 - r, c, pressed);
            }
        }

        Ok(state)
    }
}
