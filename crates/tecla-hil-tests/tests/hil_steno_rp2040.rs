//! End-to-end steno stitch HIL test for RP2040.

#![cfg(feature = "hil")]
#![allow(clippy::tests_outside_test_module, reason = "integration test")]
#![allow(missing_docs, reason = "integration test")]

use hilt::{HilConfig, Platform, RenodeRunner};
use std::path::PathBuf;

fn firmware_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/thumbv6m-none-eabi/release/examples/hil_steno_stitch");
    p
}

#[test]
#[ignore = "requires container runtime and cross-compiled firmware"]
fn steno_stitch_hil_sequence() {
    let elf = firmware_path();
    assert!(
        elf.exists(),
        "firmware not found at {} -- build with: \
         cargo build -p tecla-rp2040 --example hil_steno_stitch --features hil \
         --target thumbv6m-none-eabi --release",
        elf.display()
    );

    let output = RenodeRunner::new(HilConfig::new(Platform::rp2040(), elf).timeout(10)).run();

    assert!(output.passed(), "firmware did not complete:\n{output}");
    assert!(
        output.contains("SEQ:ing fox"),
        "unexpected stitch output:\n{output}"
    );
}
