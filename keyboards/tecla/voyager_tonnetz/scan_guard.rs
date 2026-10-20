/// Instrument-only freshness check; typing keeps the scanner's cached right hand.
pub struct ScanGuard {
    last_success_ms: Option<u64>,
}

impl ScanGuard {
    pub const fn new() -> Self {
        Self {
            last_success_ms: None,
        }
    }

    /// Only a complete successful right-hand scan renews the deadline.
    pub fn succeeded(&mut self, at_ms: u64) {
        self.last_success_ms = Some(at_ms);
    }

    pub fn is_fresh(&self, now_ms: u64) -> bool {
        self.last_success_ms
            .is_some_and(|at| now_ms.saturating_sub(at) < 100)
    }
}
