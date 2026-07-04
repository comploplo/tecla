//! Split keyboard tile identity and configuration.
//!
//! A split (or modular tile-based) keyboard is made of one or more physical
//! pieces connected by a transport link. [`TileId`] identifies which piece
//! the firmware is running on, and [`SplitConfig`] carries per-tile
//! parameters such as the host flag and column offsets for key-position
//! remapping.

/// Identifies one tile in a multi-piece keyboard topology (a traditional
/// two-half split, or N modular tiles that snap together in varying counts
/// and arrangements). Tile 0 is host by convention for the
/// [`primary()`](SplitConfig::primary)/[`secondary()`](SplitConfig::secondary)
/// convenience constructors below; use [`tile()`](SplitConfig::tile)
/// directly for topologies with more than two pieces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileId(pub u8);

/// Configuration for one tile of a split or modular keyboard.
///
/// The `col_offset` field is used by non-host tiles to remap their local
/// column indices into the unified keymap coordinate space. For example, if
/// the host tile has 6 columns, a second tile sets `col_offset = 6` so that
/// its column 0 maps to global column 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitConfig {
    /// Which tile this firmware instance represents.
    pub tile_id: TileId,
    /// Whether this tile is USB/BLE-connected and runs the full pipeline
    /// (`true`), or only scans its local matrix and forwards events to the
    /// host tile (`false`). Exactly one tile in a topology should be host.
    pub is_host: bool,
    /// Added to every [`KeyPos`](crate::event::KeyPos) column index before
    /// the event enters the pipeline. Typically `0` for the host tile and
    /// nonzero for other tiles.
    pub col_offset: u8,
}

impl SplitConfig {
    /// Creates a new tile configuration.
    #[must_use]
    pub const fn tile(tile_id: TileId, is_host: bool, col_offset: u8) -> Self {
        Self {
            tile_id,
            is_host,
            col_offset,
        }
    }

    /// Creates a host-tile (tile 0) configuration with no column offset —
    /// the common two-half USB-connected side.
    #[must_use]
    pub const fn primary() -> Self {
        Self::tile(TileId(0), true, 0)
    }

    /// Creates a non-host tile-1 configuration with the given column offset
    /// — the common two-half remote side.
    #[must_use]
    pub const fn secondary(col_offset: u8) -> Self {
        Self::tile(TileId(1), false, col_offset)
    }

    /// Returns `true` if this tile is host (USB/BLE-connected, runs the
    /// full pipeline).
    #[must_use]
    pub const fn is_host(self) -> bool {
        self.is_host
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_ids_equal_when_same() {
        assert_eq!(TileId(3), TileId(3));
    }

    #[test]
    fn tile_ids_differ() {
        assert_ne!(TileId(0), TileId(1));
    }

    #[test]
    fn tile_stores_all_fields() {
        let cfg = SplitConfig::tile(TileId(4), true, 12);
        assert_eq!(cfg.tile_id, TileId(4));
        assert!(cfg.is_host);
        assert_eq!(cfg.col_offset, 12);
    }

    #[test]
    fn primary_is_host_tile_zero_no_offset() {
        let cfg = SplitConfig::primary();
        assert_eq!(cfg.tile_id, TileId(0));
        assert!(cfg.is_host);
        assert_eq!(cfg.col_offset, 0);
    }

    #[test]
    fn secondary_is_non_host_tile_one_with_offset() {
        let cfg = SplitConfig::secondary(6);
        assert_eq!(cfg.tile_id, TileId(1));
        assert!(!cfg.is_host);
        assert_eq!(cfg.col_offset, 6);
    }

    #[test]
    fn is_host_reads_back_true() {
        assert!(SplitConfig::primary().is_host());
    }

    #[test]
    fn is_host_reads_back_false() {
        assert!(!SplitConfig::secondary(3).is_host());
    }

    #[test]
    fn configs_equal_when_same() {
        assert_eq!(
            SplitConfig::primary(),
            SplitConfig::tile(TileId(0), true, 0)
        );
    }

    #[test]
    fn configs_differ_by_offset() {
        assert_ne!(SplitConfig::secondary(5), SplitConfig::secondary(6));
    }

    #[test]
    fn config_clone_equals_original() {
        let cfg = SplitConfig::secondary(6);
        #[expect(clippy::clone_on_copy, reason = "testing Clone impl")]
        let cloned = cfg.clone();
        assert_eq!(cfg, cloned);
    }

    #[test]
    fn config_debug_is_implemented() {
        let _ = core::format_args!("{:?}", SplitConfig::primary());
    }
}
