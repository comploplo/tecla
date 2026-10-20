//! Host-only discovery root for mutation testing the actual firmware modules.
#![allow(
    dead_code,
    missing_docs,
    reason = "mutation discovery, not a firmware entry point"
)]

#[path = "../../keyboards/tecla/asterisk_v1_4_chromatica/controls.rs"]
mod chromatica;
#[path = "../../keyboards/tecla/voyager_tonnetz/scan_guard.rs"]
mod scan_guard;
#[path = "../../keyboards/tecla/voyager_tonnetz/instrument.rs"]
mod voyager;

fn main() {}
