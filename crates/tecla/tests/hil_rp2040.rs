//! HIL integration test for RP2040 — runs firmware in Renode via Podman.
//!
//! Ignored by default (requires container runtime + cross-compiled firmware).
//! Run via: `just hil-rp2040`

#![cfg(feature = "hil")]
#![allow(clippy::tests_outside_test_module, reason = "integration test")]

use std::path::PathBuf;
use tecla::assert_hil_ok;
use tecla::hil::{HilConfig, Platform, RenodeRunner};

fn firmware_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/thumbv6m-none-eabi/release/examples/hil_keyboard");
    p
}

#[test]
#[ignore = "requires container runtime and cross-compiled firmware"]
fn rp2040_keyboard_processor_boots() {
    let config = HilConfig::new(Platform::rp2040(), firmware_path());
    let runner = RenodeRunner::new(config);
    let output = runner.run();
    assert_hil_ok!(output);
}
