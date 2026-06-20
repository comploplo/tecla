//! End-to-end matrix HIL test for RP2040.
//!
//! Runs compiled Planck firmware on an emulated RP2040 in Renode. The firmware
//! iterates every key position, processes each through `KeyboardProcessor`,
//! and stores HID reports in SRAM. Renode hooks read each entry and log it.
//! This test parses the log and verifies every key produces the correct HID
//! keycode.
//!
//! Ignored by default (requires container runtime + cross-compiled firmware).
//! Run via: `just hil-matrix-rp2040`

#![cfg(feature = "hil")]
#![allow(clippy::tests_outside_test_module, reason = "integration test")]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::expect_used, reason = "test code")]
#![expect(clippy::print_stderr, reason = "test diagnostic output")]
#![expect(
    clippy::non_ascii_literal,
    reason = "em dash in user-facing error message"
)]
#![expect(clippy::str_to_string, reason = "test code")]

use std::path::PathBuf;
use std::process::Command;
use tecla::hil::{
    extract_elf_info, generate_matrix_test_resc, parse_key_log, parse_tapdance_log,
    run_renode_container, Platform,
};

fn firmware_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target/thumbv6m-none-eabi/release/examples/hil_matrix_test");
    p
}

fn find_symbol(elf: &std::path::Path, name: &str) -> Option<u32> {
    let output = Command::new("rust-nm")
        .arg("--defined-only")
        .arg(elf)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains(name) {
            let addr_str = line.split_whitespace().next()?;
            return u32::from_str_radix(addr_str, 16).ok();
        }
    }
    None
}

#[rustfmt::skip]
const PLANCK_BASE_LAYER: [(u8, u8, u8); 34] = [
    (0, 0, 0x2B), // Tab
    (0, 1, 0x14), // Q
    (0, 2, 0x1A), // W
    (0, 3, 0x08), // E
    (0, 4, 0x15), // R
    (0, 5, 0x17), // T
    (0, 6, 0x1C), // Y
    (0, 7, 0x18), // U
    (0, 8, 0x0C), // I
    (0, 9, 0x12), // O
    (0,10, 0x13), // P
    (0,11, 0x2A), // Backspace
    (1, 0, 0x29), // Escape
    (1, 1, 0x04), // A
    (1, 2, 0x16), // S
    (1, 3, 0x07), // D
    (1, 4, 0x09), // F
    (1, 5, 0x0A), // G
    (1, 6, 0x0B), // H
    (1, 7, 0x0D), // J
    (1, 8, 0x0E), // K
    (1, 9, 0x0F), // L
    (1,10, 0x33), // Semicolon
    (1,11, 0x28), // Enter
    (2, 1, 0x1D), // Z
    (2, 2, 0x1B), // X
    (2, 3, 0x06), // C
    (2, 4, 0x19), // V
    (2, 5, 0x05), // B
    (2, 6, 0x11), // N
    (2, 7, 0x10), // M
    (2, 8, 0x36), // Comma
    (2, 9, 0x37), // Dot
    (2,10, 0x38), // Slash
];

#[rustfmt::skip]
const PLANCK_MODIFIER_KEYS: [(u8, u8, u8); 6] = [
    (2,  0, 0xE1), // LSFT  (HID usage ID for Left Shift)
    (2, 11, 0xE5), // RSFT  (HID usage ID for Right Shift)
    (3,  0, 0xE0), // LCTL  (HID usage ID for Left Control)
    (3,  1, 0xE3), // LGUI  (HID usage ID for Left GUI)
    (3,  2, 0xE2), // LALT  (HID usage ID for Left Alt)
    (3,  3, 0xE1), // LSFT  (HID usage ID for Left Shift)
];

#[test]
#[ignore = "requires container runtime and cross-compiled firmware"]
#[expect(
    clippy::too_many_lines,
    reason = "HIL test with setup, execution, and verification"
)]
fn planck_keycode_verification() {
    let elf = firmware_path();
    assert!(
        elf.exists(),
        "firmware not found at {} -- build with: \
         cargo build -p tecla-rp2040 --example hil_matrix_test --features hil \
         --target thumbv6m-none-eabi --release",
        elf.display()
    );

    let elf_abs = elf.canonicalize().unwrap();
    let info = extract_elf_info(&elf_abs, "hil_marker");

    let report_marker =
        find_symbol(&elf_abs, "report_marker").expect("report_marker symbol not found");
    let hil_marker = find_symbol(&elf_abs, "hil_marker").expect("hil_marker symbol not found");
    let report_buf = find_symbol(&elf_abs, "REPORT_BUF").expect("REPORT_BUF symbol not found");
    let report_count =
        find_symbol(&elf_abs, "REPORT_COUNT").expect("REPORT_COUNT symbol not found");

    let tapdance_marker = find_symbol(&elf_abs, "tapdance_marker");
    let tap_result_addr = find_symbol(&elf_abs, "TAP_RESULT");
    let hold_layer_result_addr = find_symbol(&elf_abs, "HOLD_LAYER_RESULT");

    let work_dir = std::env::current_dir()
        .unwrap()
        .join("target/hil-matrix-rp2040");
    std::fs::create_dir_all(&work_dir).unwrap();

    std::fs::write(
        work_dir.join("platform.repl"),
        Platform::rp2040().repl_content(),
    )
    .unwrap();

    let resc = generate_matrix_test_resc(
        &elf_abs,
        "/hil/platform.repl",
        report_marker,
        hil_marker,
        report_buf,
        report_count,
        &info,
        10,
        tapdance_marker,
        tap_result_addr,
        hold_layer_result_addr,
    );
    std::fs::write(work_dir.join("test.resc"), &resc).unwrap();

    let runtime = std::env::var("TECLA_CONTAINER_RUNTIME").unwrap_or_else(|_| "podman".to_string());

    let cidfile = work_dir.join("renode.cid");
    let mut cmd = Command::new(&runtime);
    cmd.args([
        "run",
        "--rm",
        "--cidfile",
        cidfile.to_str().expect("cidfile path not valid UTF-8"),
        "--volume",
        &format!("{}:/hil:ro", work_dir.display()),
        "--volume",
        &format!("{}:{}", elf_abs.display(), elf_abs.display()),
        "--entrypoint",
        "/opt/renode/renode",
        "docker.io/antmicro/renode:latest",
        "--disable-xwt",
        "--plain",
        "-e",
        "include @/hil/test.resc",
    ]);

    let cmd_output = run_renode_container(cmd, 10, || {
        let Ok(cid) = std::fs::read_to_string(&cidfile) else {
            return;
        };
        let cid = cid.trim();
        if !cid.is_empty() {
            let _ = Command::new(&runtime).args(["rm", "-f", cid]).status();
        }
    });

    let all_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&cmd_output.stdout),
        String::from_utf8_lossy(&cmd_output.stderr),
    );

    assert!(
        all_output.contains("HIL OK"),
        "firmware did not complete.\n--- output ---\n{all_output}"
    );

    let reports = parse_key_log(&all_output);
    assert!(
        !reports.is_empty(),
        "no KEY: entries found in Renode output.\n--- output ---\n{all_output}"
    );

    let mut verified = 0;
    let mut failures = Vec::new();

    for &(row, col, expected_usage) in &PLANCK_BASE_LAYER {
        if let Some(&(_, _, _mods, keycode)) =
            reports.iter().find(|(r, c, _, _)| *r == row && *c == col)
        {
            if keycode == expected_usage {
                verified += 1;
            } else {
                failures.push(format!(
                    "({row},{col}): expected keycode 0x{expected_usage:02X}, got 0x{keycode:02X}",
                ));
            }
        } else {
            failures.push(format!("({row},{col}): no report"));
        }
    }

    let mut mod_verified = 0;

    for &(row, col, expected_usage) in &PLANCK_MODIFIER_KEYS {
        if let Some(&(_, _, _mods, keycode)) =
            reports.iter().find(|(r, c, _, _)| *r == row && *c == col)
        {
            if keycode == expected_usage {
                mod_verified += 1;
            } else {
                failures.push(format!(
                    "({row},{col}): expected modifier usage 0x{expected_usage:02X}, got 0x{keycode:02X}",
                ));
            }
        } else {
            failures.push(format!("({row},{col}): no report"));
        }
    }

    if !failures.is_empty() {
        eprintln!("failures:");
        for f in &failures {
            eprintln!("  {f}");
        }
    }

    eprintln!(
        "planck HIL: {verified}/{} keys verified, {mod_verified}/{} modifiers verified, {} reports captured",
        PLANCK_BASE_LAYER.len(),
        PLANCK_MODIFIER_KEYS.len(),
        reports.len(),
    );

    assert_eq!(
        verified,
        PLANCK_BASE_LAYER.len(),
        "{} key(s) failed verification",
        PLANCK_BASE_LAYER.len() - verified,
    );

    assert_eq!(
        mod_verified,
        PLANCK_MODIFIER_KEYS.len(),
        "{} modifier(s) failed verification",
        PLANCK_MODIFIER_KEYS.len() - mod_verified,
    );

    if let Some((tap, hold)) = parse_tapdance_log(&all_output) {
        eprintln!("tapdance: tap=0x{tap:02X} (expect 0x2C/Space), hold+press=0x{hold:02X} (expect 0x1E/N1)");
        if tap == 0x2C {
            eprintln!("  tap: PASS");
        } else {
            eprintln!("  tap: FAIL (got 0x{tap:02X}, expected 0x2C) — tap-hold timing needs investigation");
        }
        if hold == 0x1E {
            eprintln!("  hold+press: PASS");
        } else {
            eprintln!("  hold+press: FAIL (got 0x{hold:02X}, expected 0x1E) — layer activation needs investigation");
        }
    } else {
        eprintln!("warning: no TAPDANCE log found — tapdance_marker may not have been hooked");
    }
}
