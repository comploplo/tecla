//! Split keyboard role and configuration.
//!
//! A split keyboard has two physical halves connected by a transport link.
//! [`SplitRole`] identifies which half the firmware is running on, and
//! [`SplitConfig`] carries per-half parameters such as column offsets for
//! key-position remapping.

/// Identifies which half of a split keyboard the firmware is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitRole {
    /// The USB-connected half that enumerates HID and runs the full pipeline.
    Primary,
    /// The half that only scans its local matrix and sends events to the primary.
    Secondary,
}

impl SplitRole {
    /// Returns `true` if this is the primary (USB-connected) half.
    #[must_use]
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::Primary)
    }

    /// Returns `true` if this is the secondary (remote) half.
    #[must_use]
    pub const fn is_secondary(self) -> bool {
        matches!(self, Self::Secondary)
    }
}

/// Configuration for one half of a split keyboard.
///
/// The `col_offset` field is used by the secondary half to remap its local
/// column indices into the unified keymap coordinate space. For example, if
/// the left half has 6 columns, the right half sets `col_offset = 6` so that
/// its column 0 maps to global column 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitConfig {
    /// Which half this firmware instance represents.
    pub role: SplitRole,
    /// Added to every [`KeyPos`](crate::event::KeyPos) column index before
    /// the event enters the pipeline. Typically `0` for the primary half and
    /// equal to the primary's column count for the secondary half.
    pub col_offset: u8,
}

impl SplitConfig {
    /// Creates a new split configuration.
    #[must_use]
    pub const fn new(role: SplitRole, col_offset: u8) -> Self {
        Self { role, col_offset }
    }

    /// Creates a primary-half configuration with no column offset.
    #[must_use]
    pub const fn primary() -> Self {
        Self::new(SplitRole::Primary, 0)
    }

    /// Creates a secondary-half configuration with the given column offset.
    #[must_use]
    pub const fn secondary(col_offset: u8) -> Self {
        Self::new(SplitRole::Secondary, col_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_role_is_primary() {
        assert!(SplitRole::Primary.is_primary());
        assert!(!SplitRole::Primary.is_secondary());
    }

    #[test]
    fn secondary_role_is_secondary() {
        assert!(SplitRole::Secondary.is_secondary());
        assert!(!SplitRole::Secondary.is_primary());
    }

    #[test]
    fn primary_config_has_zero_offset() {
        let cfg = SplitConfig::primary();
        assert_eq!(cfg.role, SplitRole::Primary);
        assert_eq!(cfg.col_offset, 0);
    }

    #[test]
    fn secondary_config_carries_offset() {
        let cfg = SplitConfig::secondary(6);
        assert_eq!(cfg.role, SplitRole::Secondary);
        assert_eq!(cfg.col_offset, 6);
    }

    #[test]
    fn new_config_stores_fields() {
        let cfg = SplitConfig::new(SplitRole::Primary, 7);
        assert_eq!(cfg.role, SplitRole::Primary);
        assert_eq!(cfg.col_offset, 7);
    }

    #[test]
    fn roles_are_not_equal() {
        assert_ne!(SplitRole::Primary, SplitRole::Secondary);
    }

    #[test]
    fn configs_equal_when_same() {
        assert_eq!(
            SplitConfig::primary(),
            SplitConfig::new(SplitRole::Primary, 0)
        );
    }

    #[test]
    fn configs_differ_by_role() {
        assert_ne!(
            SplitConfig::new(SplitRole::Primary, 6),
            SplitConfig::new(SplitRole::Secondary, 6),
        );
    }

    #[test]
    fn configs_differ_by_offset() {
        assert_ne!(SplitConfig::secondary(5), SplitConfig::secondary(6));
    }

    #[test]
    fn role_clone_equals_original() {
        let role = SplitRole::Secondary;
        #[expect(clippy::clone_on_copy, reason = "testing Clone impl")]
        let cloned = role.clone();
        assert_eq!(role, cloned);
    }

    #[test]
    fn config_clone_equals_original() {
        let cfg = SplitConfig::secondary(6);
        #[expect(clippy::clone_on_copy, reason = "testing Clone impl")]
        let cloned = cfg.clone();
        assert_eq!(cfg, cloned);
    }

    #[test]
    fn role_debug_is_implemented() {
        let _ = core::format_args!("{:?}", SplitRole::Primary);
        let _ = core::format_args!("{:?}", SplitRole::Secondary);
    }

    #[test]
    fn config_debug_is_implemented() {
        let _ = core::format_args!("{:?}", SplitConfig::primary());
    }
}
