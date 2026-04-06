/// A USB HID consumer-control keycode (media, brightness, etc.).
///
/// Discriminants match the HID consumer usage page (0x0C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ConsumerCode {
    /// Brightness decrease.
    BrightnessDown = 0x0070,
    /// Brightness increase.
    BrightnessUp = 0x006F,
    /// Eject disc.
    Eject = 0x00B8,
    /// Fast forward.
    FastForward = 0x00B3,
    /// Mute audio.
    Mute = 0x00E2,
    /// Next track.
    NextTrack = 0x00B5,
    /// Play / pause.
    Play = 0x00CD,
    /// Previous track.
    PrevTrack = 0x00B6,
    /// Rewind.
    Rewind = 0x00B4,
    /// Stop playback.
    Stop = 0x00B7,
    /// Volume decrease.
    VolumeDown = 0x00EA,
    /// Volume increase.
    VolumeUp = 0x00E9,
    /// Open media player / media select.
    MediaSelect = 0x0183,
    /// Open mail client.
    Mail = 0x018A,
    /// Open calculator.
    Calculator = 0x0192,
    /// Open file browser / my computer.
    MyComputer = 0x0194,
    /// Web search.
    WwwSearch = 0x0221,
    /// Web home page.
    WwwHome = 0x0223,
    /// Web back.
    WwwBack = 0x0224,
    /// Web forward.
    WwwForward = 0x0225,
    /// Web stop loading.
    WwwStop = 0x0226,
    /// Web refresh.
    WwwRefresh = 0x0227,
    /// Web favorites / bookmarks.
    WwwFavorites = 0x022A,
}

impl ConsumerCode {
    /// Returns the raw HID usage ID.
    #[inline]
    #[must_use]
    #[expect(
        clippy::as_conversions,
        reason = "enum discriminant to u16 for HID usage ID"
    )]
    pub const fn usage_id(self) -> u16 {
        self as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumer_volume_up_has_correct_usage_id() {
        assert_eq!(ConsumerCode::VolumeUp.usage_id(), 0x00E9);
    }

    #[test]
    fn consumer_codes_are_distinct_from_each_other() {
        assert_ne!(
            ConsumerCode::VolumeUp.usage_id(),
            ConsumerCode::VolumeDown.usage_id()
        );
        assert_ne!(ConsumerCode::Play.usage_id(), ConsumerCode::Stop.usage_id());
    }
}
