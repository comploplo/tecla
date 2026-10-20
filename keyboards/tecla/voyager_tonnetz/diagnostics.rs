/// Transport and scan context attached to an explicitly requested snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticStatus {
    /// Monotonic milliseconds at the instrument update.
    pub now_ms: u64,
    /// Whether this instrument update accepted fresh physical gestures.
    pub ready: bool,
    /// Age of the last successful right scan; MAX means none has succeeded.
    pub right_age_ms: u64,
    /// Latest right-hand scan completed successfully.
    pub right_ok: bool,
    /// USB generation and configured bit used for this update.
    pub usb_session: u32,
    /// Post-transmission MIDI recovery/cleanup readiness.
    pub midi_ready: bool,
    /// This was the first recovered scan, which consumes held gestures silently.
    pub recovered: bool,
    /// Overflow or note-output failure occurred during this update.
    pub tx_fault: bool,
    /// Pending snapshots overwritten or abandoned by the serial transport.
    pub dropped: u32,
    /// Instrument update duration, including harmonic planning, in microseconds.
    pub update_us: u32,
}
