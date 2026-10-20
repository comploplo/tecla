//! ZSA Navigator trackball protocol.
//!
//! The Navigator is two chips behind one I2C address, which is why a naive read
//! of it returns nothing but zeros:
//!
//! 1. An **SC18IS606** I2C-to-SPI bridge, the only thing actually on the I2C
//!    bus. It exists so the module can hang off a TRRS jack on a split
//!    keyboard, which carries I2C but not SPI.
//! 2. A **PAW3805EK** motion sensor on the far side of that bridge's SPI,
//!    selected by [`sc18is606::SS_SENSOR`].
//!
//! Every sensor access is therefore an I2C write of a
//! [`spi_frame`] to the bridge, a short settling delay, and an I2C read of the
//! bridge's SPI receive buffer.
//!
//! This module is the wire format only — frame layout, register map, and the
//! decode arithmetic — so it can be tested on the host. The I2C transport lives
//! in the board crate, next to the bus it drives.
//!
//! Protocol established from ZSA's own QMK module (`zsa/qmk_modules`,
//! `navigator_trackball`), which is GPL-2.0-or-later; only the register
//! numbers and sequence are used here, not their code.

/// 7-bit I2C address of the bridge.
///
/// ZSA's QMK module writes this as `0x50` because QMK uses 8-bit addresses;
/// shifted down for the `embedded-hal` convention it is `0x28`.
pub const NAVIGATOR_ADDR: u8 = 0x28;

/// SC18IS606 I2C-to-SPI bridge commands.
pub mod sc18is606 {
    /// Function ID selecting slave 0 — the sensor's chip select.
    pub const SS_SENSOR: u8 = 0x01;
    /// Function ID for "configure the SPI port".
    pub const CONFIG_SPI: u8 = 0xF0;
    /// SPI port setup: MSB first, mode 3, 155 kHz.
    pub const SPI_CONFIG: u8 = 0xDC;
    /// Function ID for "clear interrupt".
    pub const CLEAR_INT: u8 = 0xF1;
    /// Function ID for "read the bridge's own device ID".
    pub const GET_ID: u8 = 0xFE;
}

/// PAW3805EK motion sensor registers, as addressed through the bridge.
pub mod paw3805ek {
    /// Value [`REG_PRODUCT_ID`] returns on a healthy SPI link.
    pub const PRODUCT_ID: u8 = 0x31;

    /// Product ID — reads [`PRODUCT_ID`], and is the link's health check.
    pub const REG_PRODUCT_ID: u8 = 0x00;
    /// Motion status; [`MOTION_BIT`] set means deltas are waiting.
    pub const REG_MOTION: u8 = 0x02;
    /// Low byte of the X delta.
    pub const REG_DELTA_X_LO: u8 = 0x03;
    /// Low byte of the Y delta.
    pub const REG_DELTA_Y_LO: u8 = 0x04;
    /// High byte of the X delta (16-bit mode only).
    pub const REG_DELTA_X_HI: u8 = 0x11;
    /// High byte of the Y delta (16-bit mode only).
    pub const REG_DELTA_Y_HI: u8 = 0x12;
    /// Software reset.
    pub const REG_RESET: u8 = 0x06;
    /// Write-protect gate: most writes are ignored until it is unlocked.
    pub const REG_WRITE_PROTECT: u8 = 0x09;
    /// Orientation and motion-data width.
    pub const REG_ORIENTATION: u8 = 0x19;
    /// Counts-per-inch, X axis.
    pub const REG_CPI_X: u8 = 0x0D;
    /// Counts-per-inch, Y axis.
    pub const REG_CPI_Y: u8 = 0x0E;

    /// OR into a register number to make the access a write.
    pub const WRITE_BIT: u8 = 0x80;
    /// Set in [`REG_MOTION`] when there is motion to read.
    pub const MOTION_BIT: u8 = 0x80;

    /// Magic value that unlocks [`REG_WRITE_PROTECT`].
    pub const WRITE_PROTECT_DISABLE: u8 = 0x5A;
    /// Re-locks [`REG_WRITE_PROTECT`].
    pub const WRITE_PROTECT_ENABLE: u8 = 0x00;
    /// Value written to [`REG_RESET`] to reset the sensor.
    pub const RESET: u8 = 0x80;
    /// Orientation with 16-bit motion data.
    pub const ORIENTATION_16BIT: u8 = 0x30;
}

/// Default CPI: the sensor's maximum.
///
/// Run the sensor as finely as it goes and shape the result in firmware with
/// [`DEFAULT_CURVE`]. Resolution thrown away at the sensor cannot be recovered
/// later, whereas too much of it is something a transfer function can spend.
pub const CPI_DEFAULT: u8 = CPI_MAX;
/// Largest CPI the sensor accepts.
pub const CPI_MAX: u8 = 125;
/// CPI change per adjustment step.
pub const CPI_STEP: u8 = 5;

/// Speed-to-gain curve for the Navigator at [`CPI_DEFAULT`].
///
/// Anchored to two observations on hardware: at CPI 40 slow control was too
/// coarse but fast flicks felt right, and at CPI 100 slow control was good but
/// fast was unusable. So the curve holds unity gain for slow movement — the
/// full benefit of running the sensor at [`CPI_MAX`], roughly three times the
/// resolution of ZSA's default — and falls to about a third for fast movement,
/// which lands a flick back where CPI 40 had it.
///
/// Speeds are counts per poll at the driver's poll interval; change one and
/// the other has to move with it.
pub const DEFAULT_CURVE: crate::devices::mouse::transfer::TransferCurve =
    crate::devices::mouse::transfer::TransferCurve {
        slow_speed: 8,
        slow_gain: 256,
        fast_speed: 64,
        fast_gain: 82,
    };

/// Bytes in one bridge frame: chip select, register, data.
pub const FRAME_LEN: usize = 3;

/// Builds the bridge frame that performs one SPI register access.
///
/// For a read, `data` is a dummy byte clocked out so the sensor has something
/// to answer on; the reply lands in [`RESPONSE_INDEX`] of the read-back buffer.
#[must_use]
pub const fn spi_frame(reg: u8, data: u8) -> [u8; FRAME_LEN] {
    [sc18is606::SS_SENSOR, reg, data]
}

/// Frame that writes `value` to `reg`.
#[must_use]
pub const fn write_frame(reg: u8, value: u8) -> [u8; FRAME_LEN] {
    spi_frame(reg | paw3805ek::WRITE_BIT, value)
}

/// Frame that reads `reg`.
#[must_use]
pub const fn read_frame(reg: u8) -> [u8; FRAME_LEN] {
    spi_frame(reg, 0x00)
}

/// Index into the bridge's read-back buffer holding the sensor's reply.
///
/// The bridge does not echo the chip-select byte, so the buffer is the SPI
/// bytes received while the frame's register and data bytes went out; the
/// sensor answers during the second of those.
pub const RESPONSE_INDEX: usize = 1;

/// Frame configuring the bridge's SPI port. Sent to the bridge, not the sensor,
/// so it is two bytes and carries no chip select.
#[must_use]
pub const fn bridge_config_frame() -> [u8; 2] {
    [sc18is606::CONFIG_SPI, sc18is606::SPI_CONFIG]
}

/// The sensor bring-up sequence, in order.
///
/// The product-ID read at index 1 is not decorative: it is the only check that
/// the SPI link behind the bridge actually works, since every access up to that
/// point would look successful on I2C even with nothing attached.
pub const CONFIG_SEQUENCE: [[u8; FRAME_LEN]; 5] = [
    spi_frame(paw3805ek::REG_RESET, paw3805ek::RESET),
    read_frame(paw3805ek::REG_PRODUCT_ID),
    write_frame(
        paw3805ek::REG_WRITE_PROTECT,
        paw3805ek::WRITE_PROTECT_DISABLE,
    ),
    write_frame(paw3805ek::REG_ORIENTATION, paw3805ek::ORIENTATION_16BIT),
    write_frame(
        paw3805ek::REG_WRITE_PROTECT,
        paw3805ek::WRITE_PROTECT_ENABLE,
    ),
];

/// Index in [`CONFIG_SEQUENCE`] whose reply must be [`paw3805ek::PRODUCT_ID`].
pub const CONFIG_ID_CHECK_INDEX: usize = 1;

/// The CPI sequence, which must be bracketed by write-protect unlock/lock.
#[must_use]
pub const fn cpi_sequence(cpi: u8) -> [[u8; FRAME_LEN]; 4] {
    [
        write_frame(
            paw3805ek::REG_WRITE_PROTECT,
            paw3805ek::WRITE_PROTECT_DISABLE,
        ),
        write_frame(paw3805ek::REG_CPI_X, cpi),
        write_frame(paw3805ek::REG_CPI_Y, cpi),
        write_frame(
            paw3805ek::REG_WRITE_PROTECT,
            paw3805ek::WRITE_PROTECT_ENABLE,
        ),
    ]
}

/// Whether the motion register says deltas are waiting.
#[must_use]
pub const fn motion_pending(motion_register: u8) -> bool {
    motion_register & paw3805ek::MOTION_BIT != 0
}

/// Assembles a signed 16-bit delta from its two register reads.
#[must_use]
pub const fn delta(low: u8, high: u8) -> i16 {
    #[expect(
        clippy::as_conversions,
        reason = "two's-complement reinterpret of the assembled register pair"
    )]
    {
        (((high as u16) << 8) | (low as u16)) as i16
    }
}

/// Raises CPI by one step, saturating at [`CPI_MAX`].
#[must_use]
pub const fn cpi_increase(cpi: u8) -> u8 {
    if cpi <= CPI_MAX - CPI_STEP {
        cpi + CPI_STEP
    } else {
        CPI_MAX
    }
}

/// Lowers CPI by one step, never below one step.
#[must_use]
pub const fn cpi_decrease(cpi: u8) -> u8 {
    if cpi > CPI_STEP {
        cpi - CPI_STEP
    } else {
        CPI_STEP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_matches_qmks_eight_bit_form() {
        // ZSA's module defines 0x50; embedded-hal wants it 7-bit.
        assert_eq!(NAVIGATOR_ADDR, 0x50 >> 1);
        assert_eq!(NAVIGATOR_ADDR, 0x28);
    }

    #[test]
    fn frames_carry_the_chip_select_first() {
        assert_eq!(read_frame(0x03), [sc18is606::SS_SENSOR, 0x03, 0x00]);
    }

    #[test]
    fn write_frames_set_the_write_bit_and_reads_do_not() {
        assert_eq!(write_frame(0x09, 0x5A), [sc18is606::SS_SENSOR, 0x89, 0x5A]);
        assert_eq!(read_frame(0x09)[1] & paw3805ek::WRITE_BIT, 0);
    }

    #[test]
    fn config_sequence_checks_the_product_id() {
        // The link check must be a *read* of the product-ID register, or it
        // proves nothing about the sensor behind the bridge.
        let frame = CONFIG_SEQUENCE[CONFIG_ID_CHECK_INDEX];
        assert_eq!(frame, read_frame(paw3805ek::REG_PRODUCT_ID));
    }

    #[test]
    fn config_sequence_re_locks_write_protection() {
        let last = CONFIG_SEQUENCE[CONFIG_SEQUENCE.len() - 1];
        assert_eq!(
            last,
            write_frame(
                paw3805ek::REG_WRITE_PROTECT,
                paw3805ek::WRITE_PROTECT_ENABLE
            )
        );
    }

    #[test]
    fn cpi_sequence_is_bracketed_by_unlock_and_lock() {
        let seq = cpi_sequence(40);
        assert_eq!(seq[0][2], paw3805ek::WRITE_PROTECT_DISABLE);
        assert_eq!(seq[3][2], paw3805ek::WRITE_PROTECT_ENABLE);
        assert_eq!(seq[1], write_frame(paw3805ek::REG_CPI_X, 40));
        assert_eq!(seq[2], write_frame(paw3805ek::REG_CPI_Y, 40));
    }

    #[test]
    fn motion_bit_decoding() {
        assert!(motion_pending(0x80));
        assert!(motion_pending(0xFF));
        assert!(!motion_pending(0x00));
        assert!(!motion_pending(0x7F));
    }

    #[test]
    fn delta_assembles_little_endian_and_signs() {
        assert_eq!(delta(0x01, 0x00), 1);
        assert_eq!(delta(0x00, 0x01), 256);
        assert_eq!(delta(0xFF, 0xFF), -1);
        // A pure low-byte negative must stay negative once the high byte is
        // included — this is where an unsigned assembly would read as +65280.
        assert_eq!(delta(0x00, 0xFF), -256);
        assert_eq!(delta(0x00, 0x80), i16::MIN);
        assert_eq!(delta(0xFF, 0x7F), i16::MAX);
    }

    #[test]
    fn cpi_steps_saturate_at_both_ends() {
        // A mid-range value, not CPI_DEFAULT — stepping behaviour should not
        // change meaning just because the default moves to one of the limits.
        let mid = CPI_MAX / 2;
        assert_eq!(cpi_increase(mid), mid + CPI_STEP);
        assert_eq!(cpi_decrease(mid), mid - CPI_STEP);
        assert_eq!(cpi_increase(CPI_MAX), CPI_MAX);
        assert_eq!(cpi_increase(CPI_MAX - 1), CPI_MAX);
        assert_eq!(cpi_decrease(CPI_STEP), CPI_STEP);
        assert_eq!(cpi_decrease(0), CPI_STEP);
    }

    #[test]
    fn default_cpi_runs_the_sensor_at_full_resolution() {
        // The curve exists so the sensor can be run wide open; if this ever
        // drops below the maximum, resolution is being thrown away upstream of
        // anything firmware can recover.
        assert_eq!(CPI_DEFAULT, CPI_MAX);
    }
}
