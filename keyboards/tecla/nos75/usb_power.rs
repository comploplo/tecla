// USB bus power state shared by the hardware handler and host tests.
#[derive(Default)]
pub struct UsbPower {
    configured: bool,
    suspended: bool,
}

impl UsbPower {
    pub fn reset(&mut self) {
        self.configured = false;
        self.suspended = false;
    }
    pub fn enabled(&mut self, enabled: bool) {
        if !enabled {
            self.reset();
        }
    }
    pub fn configured(&mut self, configured: bool) {
        self.configured = configured;
    }
    pub fn suspended(&mut self, suspended: bool) {
        self.suspended = suspended;
    }
    pub const fn lighting_allowed(&self) -> bool {
        self.configured && !self.suspended
    }
}
