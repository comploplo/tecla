default: test

# Host-only crates — exclude embedded board crates that require cross-compilation
_host := "--workspace --exclude tecla-rp2040 --exclude tecla-nrf52840 --exclude tecla-stm32f4 --exclude tecla-stm32f3 --exclude tecla-stm32f0"

# Host feature bundle. usb/usb-midi require no_std and are checked by build-boards;
# optional host features are also tested individually by hack.
_host_features := "--features tecla/testing,tecla/embassy,tecla/storage,tecla/via,tecla/autocorrect,tecla/autocomplete"

# Run the host test suite
test:
    cargo nextest run {{_host}} {{_host_features}}

# Run documentation examples (nextest does not run doctests).
test-doc:
    cargo test --locked --doc {{_host}} {{_host_features}}

# Mutation-test the keyed instruments in a scratch copy (needs cargo-mutants).
mutants-instruments *args:
    # cargo-mutants 27.1 needs explicit Cargo package selection for its baseline too.
    cargo mutants -p tecla-instrument-tests --test-package tecla --features tecla/testing,tecla/embassy --gitignore true --cap-lints true \
        --file '**/keyboards/tecla/asterisk_v1_4_chromatica/controls.rs' \
        --file '**/keyboards/tecla/asterisk_v1_4_suspension/controls.rs' \
        --file '**/keyboards/tecla/voyager_tonnetz/instrument.rs' \
        --file '**/keyboards/tecla/voyager_tonnetz/scan_guard.rs' \
        --cargo-arg=--package=tecla \
        --cargo-arg=--test=asterisk_chromatica \
        --cargo-arg=--test=asterisk_suspension \
        --cargo-arg=--test=voyager_note_layouts \
        --cargo-arg=--test=voyager_tonnetz \
        --output target/mutations {{args}}

# Mutation-test active typing statistics and their processor/keymap integration.
mutants-typing *args:
    cargo mutants -p tecla --features testing --gitignore true --cap-lints true \
        --file 'crates/tecla/src/telemetry/typing.rs' \
        --cargo-arg=--lib \
        --cargo-arg=--test=typing_stats \
        --cargo-arg=--test=typing_stats_keymaps \
        --output target/mutations-typing {{args}}

# Mutation-test the reusable harmonic realization engine.
mutants-harmony *args:
    cargo mutants -p tecla --features testing --gitignore true --cap-lints true \
        --file 'crates/tecla/src/music/realization.rs' \
        --cargo-arg=--lib \
        --cargo-test-arg=music::realization::tests \
        --output target/mutations-harmony {{args}}

# Needs cargo-hack and cargo-nextest; does not test every feature combination.
# Test each host-compatible feature independently.
hack:
    cargo hack {{_host}} --each-feature --exclude-features usb,usb-midi nextest run

# Clippy on the host crates
lint:
    cargo clippy {{_host}} {{_host_features}} --all-targets

# Build host API docs with warnings denied, including broken intra-doc links.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps {{_host}} {{_host_features}}

# Build all publishable archives outside the checkout, including firmware examples.
package-check *args:
    python3 scripts/check-packages.py {{args}}

# Format all crates
fmt:
    cargo fmt --all

# Check formatting without writing (used by pre-commit and CI)
fmt-check:
    cargo fmt --all -- --check

# Open host coverage in a browser (needs cargo-llvm-cov); gates are below.
cov:
    cargo llvm-cov {{_host}} --features tecla/testing --open

# Coverage gate: tecla core crate, 95% of lines
cov-core:
    cargo llvm-cov -p tecla --features testing \
        --fail-under-lines 95 \
        --lcov --output-path lcov-core.info

# CLI process/USB/filesystem plumbing in src/bin/ is excluded; profile discovery,
# selector matching and command assembly in tecla_config::flash remain measured.
# Coverage gate: all host crates, 90% of lines
cov-all:
    cargo llvm-cov {{_host}} --features tecla/testing \
        --ignore-filename-regex 'src/bin/' \
        --fail-under-lines 90 \
        --lcov --output-path lcov.info

_rp2040_target := "thumbv6m-none-eabi"
_cortex_m4_target := "thumbv7em-none-eabihf"

# Separate builds avoid STM32 chip feature conflicts; release fits Voyager flash.
# Excludes ESP32 and core1-autocorrect.
# Build supported Cortex-M configurations and their enabled examples.
build-boards:
    # Check storage/via serialization with no_std, separately from host tests.
    cargo check --release -p tecla --features usb,usb-midi,via,storage,embassy --target {{_rp2040_target}}
    cargo build --release -p tecla-rp2040    --lib --examples --features hil,steno-dictionary,usb-midi --target {{_rp2040_target}}
    cargo build --release -p tecla-nrf52840  --lib --examples --features hil --target {{_cortex_m4_target}}
    # BLE needs --no-default-features: nrf-mpsl supplies the critical-section
    # implementation, and cortex-m's would be a duplicate symbol. Scoped to the
    # BLE example — the other examples need cortex-m's, so they cannot be built
    # in the same invocation.
    cargo build --release -p tecla-nrf52840  --lib --example planck_ble --no-default-features --features ble --target {{_cortex_m4_target}}
    cargo build --release -p tecla-stm32f4   --lib --examples --target {{_cortex_m4_target}}
    cargo build --release -p tecla-stm32f3   --lib --examples --target {{_cortex_m4_target}}
    cargo build --release -p tecla-stm32f3   --lib --examples --features usb-midi --target {{_cortex_m4_target}}
    cargo build --release -p tecla-stm32f0   --lib --examples --target thumbv6m-none-eabi

# Clippy the same Cortex-M configurations as build-boards.
lint-boards:
    cargo clippy -p tecla-rp2040   --lib --examples --features hil,steno-dictionary,usb-midi --target {{_rp2040_target}}
    cargo clippy -p tecla-nrf52840 --lib --examples --features hil --target {{_cortex_m4_target}}
    cargo clippy -p tecla-nrf52840 --lib --example planck_ble --no-default-features --features ble --target {{_cortex_m4_target}}
    cargo clippy -p tecla-stm32f4  --lib --examples --target {{_cortex_m4_target}}
    cargo clippy -p tecla-stm32f3  --lib --examples --target {{_cortex_m4_target}}
    cargo clippy -p tecla-stm32f3  --lib --examples --features usb-midi --target {{_cortex_m4_target}}
    cargo clippy -p tecla-stm32f0  --lib --examples --target thumbv6m-none-eabi

# Run the RP2040 smoke test in a Renode container.
hil-rp2040:
    cargo build -p tecla-rp2040 --example hil_keyboard --features hil --target {{_rp2040_target}} --release
    cargo test --manifest-path crates/tecla-hil-tests/Cargo.toml --features hil --test hil_rp2040 -- --ignored

# Run the nRF52840 smoke test in a Renode container.
hil-nrf52840:
    cargo build -p tecla-nrf52840 --example hil_keyboard --features hil --target {{_cortex_m4_target}} --release
    cargo test --manifest-path crates/tecla-hil-tests/Cargo.toml --features hil --test hil_nrf52840 -- --ignored

# Verify RP2040 matrix reports in Renode (Linux x86_64 harness).
hil-matrix-rp2040:
    cargo build -p tecla-rp2040 --example hil_matrix_test --features hil --target {{_rp2040_target}} --release
    cargo test --manifest-path crates/tecla-hil-tests/Cargo.toml --features hil --test hil_matrix_rp2040 --target x86_64-unknown-linux-gnu -- --ignored

# Verify nRF52840 matrix reports in Renode (Linux x86_64 harness).
hil-matrix-nrf52840:
    cargo build -p tecla-nrf52840 --example hil_matrix_test --features hil --target {{_cortex_m4_target}} --release
    cargo test --manifest-path crates/tecla-hil-tests/Cargo.toml --features hil --test hil_matrix_nrf52840 --target x86_64-unknown-linux-gnu -- --ignored

# Run the smoke and matrix emulation suites.
hil: hil-rp2040 hil-nrf52840 hil-matrix-rp2040 hil-matrix-nrf52840

# Build and flash a profile or QMK board; use --list for profiles and devices.
flash *args:
    cargo run -p tecla-config --bin flash -- {{args}}

# List host dependencies with newer releases (needs cargo-outdated).
outdated:
    cargo outdated {{_host}} --exit-code 1

# Run CI gates plus local per-feature tests and Renode emulation.
ci:
    just fmt-check
    just lint
    just test
    just test-doc
    just build-boards
    just lint-boards
    just doc
    just package-check
    just hack
    just cov-core
    just cov-all
    just hil
