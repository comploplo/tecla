_toolchain_root := `rustup toolchain list -v | awk '/stable/{print $NF}'`
_llvm_bin := _toolchain_root + "/lib/rustlib/x86_64-unknown-linux-gnu/bin"
export LLVM_COV      := _llvm_bin + "/llvm-cov"
export LLVM_PROFDATA := _llvm_bin + "/llvm-profdata"

default: test

# Host-only crates — exclude embedded board crates that require cross-compilation
_host := "--workspace --exclude tecla-rp2040 --exclude tecla-nrf52840 --exclude tecla-stm32f4"

# Run tests across all host-compilable workspace crates
test:
    cargo nextest run {{_host}}

# Check every feature combination compiles and passes tests
# Requires: cargo install cargo-hack cargo-nextest
hack:
    cargo hack {{_host}} --feature-powerset nextest run

# Clippy across all crates and feature combinations
# Run this before implementing any module to verify clean baseline
lint:
    cargo hack {{_host}} --feature-powerset clippy --all-targets

# Format all crates
fmt:
    cargo fmt --all

# Check formatting without writing (used by pre-commit and CI)
fmt-check:
    cargo fmt --all -- --check

# Coverage — tecla core must be 100%, all other crates 90%+
# Requires: cargo install cargo-llvm-cov
cov:
    cargo llvm-cov {{_host}} --features tecla/testing --open

# Coverage enforcement — CI gate, fails build if thresholds not met
# Note: keyboard.rs hosts the generic KeyboardProcessor<ROWS,COLS,LAYERS,KEYS>.
# LLVM counts each un-instantiated monomorphization as a missed "line", so the
# effective ceiling for that file is ~93% even with 100% source-line coverage.
# 95% is the highest achievable threshold given this tooling limitation.
cov-core:
    cargo llvm-cov -p tecla --features testing \
        --fail-under-lines 95 \
        --lcov --output-path lcov-core.info

cov-all:
    cargo llvm-cov {{_host}} --features tecla/testing \
        --fail-under-lines 90 \
        --lcov --output-path lcov.info

_rp2040_target := "thumbv6m-none-eabi"
_cortex_m4_target := "thumbv7em-none-eabihf"

# Run RP2040 HIL smoke test in Renode via Podman
hil-rp2040:
    cargo build -p tecla-rp2040 --example hil_keyboard --features hil --target {{_rp2040_target}} --release
    cargo test --manifest-path crates/tecla-hil-tests/Cargo.toml --features hil --test hil_rp2040 -- --ignored

# Run nRF52840 HIL smoke test in Renode via Podman
hil-nrf52840:
    cargo build -p tecla-nrf52840 --example hil_keyboard --features hil --target {{_cortex_m4_target}} --release
    cargo test --manifest-path crates/tecla-hil-tests/Cargo.toml --features hil --test hil_nrf52840 -- --ignored

# Run RP2040 matrix integration test — presses every key via simulated GPIO
hil-matrix-rp2040:
    cargo build -p tecla-rp2040 --example hil_matrix_test --features hil --target {{_rp2040_target}} --release
    cargo test --manifest-path crates/tecla-hil-tests/Cargo.toml --features hil --test hil_matrix_rp2040 --target x86_64-unknown-linux-gnu -- --ignored

# Run nRF52840 matrix integration test — presses every key via simulated GPIO
hil-matrix-nrf52840:
    cargo build -p tecla-nrf52840 --example hil_matrix_test --features hil --target {{_cortex_m4_target}} --release
    cargo test --manifest-path crates/tecla-hil-tests/Cargo.toml --features hil --test hil_matrix_nrf52840 --target x86_64-unknown-linux-gnu -- --ignored

# Run all HIL tests
hil: hil-rp2040 hil-nrf52840 hil-matrix-rp2040 hil-matrix-nrf52840

# Flash a board using the central Rust flash coordinator.
# Usage:
#   just flash --device <usb-target> --qmk-keyboard <keyboard_name>
#   just flash --device <usb-target> --tecla-config <profile>
flash *args:
    cargo run -p tecla-config --bin flash -- {{args}}

# Check for outdated dependencies
# Requires: cargo install cargo-outdated
outdated:
    cargo outdated {{_host}} --exit-code 1

# Run everything — same gates as CI
ci:
    just fmt-check
    just lint
    just hack
    just outdated
    just cov-core
    just cov-all
    just hil
