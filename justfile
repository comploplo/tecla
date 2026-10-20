# bindgen (via nrf-mpsl-sys/nrf-sdc-sys, used by the `ble` feature) needs
# libclang. macOS keeps it inside the Xcode toolchain, which bindgen does not
# search on its own; elsewhere this resolves to empty and bindgen's normal
# lookup applies. Only fills in when LIBCLANG_PATH is not already set.
_libclang := `xcode-select -p 2>/dev/null | sed 's|$|/Toolchains/XcodeDefault.xctoolchain/usr/lib|' || true`
export LIBCLANG_PATH := env_var_or_default("LIBCLANG_PATH", _libclang)

default: test

# Host-only crates — exclude embedded board crates that require cross-compilation
_host := "--workspace --exclude tecla-rp2040 --exclude tecla-nrf52840 --exclude tecla-stm32f4 --exclude tecla-stm32f3"

# Every tecla feature that can build on the host. NOT --all-features: `usb`
# pulls embassy-usb -> ssmarshal, which cannot compile alongside `std`. The
# `usb`/`usb-midi` features are covered by `just build-boards` instead, which
# cross-compiles the board crates with them on.
_host_features := "--features tecla/testing,tecla/embassy,tecla/storage,tecla/via,tecla/autocorrect,tecla/autocomplete"

# Run the host test suite
test:
    cargo nextest run {{_host}} {{_host_features}}

# Mutation-test the keyed instruments in a scratch copy (needs cargo-mutants).
mutants-instruments *args:
    cargo mutants -p tecla --features mutation-testing --gitignore true --cap-lints true \
        --file '**/keyboards/tecla/asterisk_v1_4_chromatica/controls.rs' \
        --file '**/keyboards/tecla/voyager_tonnetz/instrument.rs' \
        --file '**/keyboards/tecla/voyager_tonnetz/scan_guard.rs' \
        --cargo-arg=--test=asterisk_chromatica \
        --cargo-arg=--test=voyager_note_layouts \
        --cargo-arg=--test=voyager_tonnetz \
        --output target/mutations {{args}}

# Check each feature compiles and passes tests on its own. Not the full
# powerset: the features are additive, so the failure this catches — a feature
# missing its own deps or cfg gates — shows up with that feature alone.
# Test each feature on its own (needs cargo-hack, cargo-nextest)
hack:
    cargo hack {{_host}} --each-feature --exclude-features usb,usb-midi nextest run

# Clippy on the host crates
lint:
    cargo clippy {{_host}} {{_host_features}} --all-targets

# Build the docs.rs documentation, failing on any broken intra-doc link.
# This is the gate that catches doc rot — module tours and links that describe
# a layout the code has moved on from.
# Build the docs, failing on broken intra-doc links
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps {{_host}} {{_host_features}}

# Format all crates
fmt:
    cargo fmt --all

# Check formatting without writing (used by pre-commit and CI)
fmt-check:
    cargo fmt --all -- --check

# Coverage report in a browser. Gates are cov-core / cov-all below.
# Coverage report in a browser (needs cargo-llvm-cov)
cov:
    cargo llvm-cov {{_host}} --features tecla/testing --open

# Coverage gate for the core crate: 95% of lines.
#
# keyboard/processor.rs reads lower than its source coverage because
# llvm-cov counts every monomorphization of the generic
# KeyboardProcessor<ROWS,COLS,LAYERS,KEYS> separately. That is a real effect,
# but it is not a licence to stop at whatever number falls out: the gate is
# enforced on the workspace total, which is currently above it.
# Coverage gate: tecla core crate, 95% of lines
cov-core:
    cargo llvm-cov -p tecla --features testing \
        --fail-under-lines 95 \
        --lcov --output-path lcov-core.info

# Coverage gate for all host crates: 90% of lines.
#
# `src/bin/` is excluded. What that leaves out is now only CLI driver code:
# process spawning, sysfs walking, waiting for a bootloader volume, and the
# `-> !` exit paths. The decision-making logic that used to live in the binary
# — profile discovery, selector matching, flash-command suggestion, cargo
# argument assembly — was moved to `tecla_config::flash`, which sits at ~98%
# and is included in this number.
#
# Be honest about what the exclusion buys: 92.3% with it, 88.9% without. If
# `src/bin/` ever grows real logic again, move that logic to the library
# rather than widening this exclusion.
# Coverage gate: all host crates, 90% of lines
cov-all:
    cargo llvm-cov {{_host}} --features tecla/testing \
        --ignore-filename-regex 'src/bin/' \
        --fail-under-lines 90 \
        --lcov --output-path lcov.info

_rp2040_target := "thumbv6m-none-eabi"
_cortex_m4_target := "thumbv7em-none-eabihf"

# Cross-compile every board crate and its examples. `lint`/`test`/`hack` all
# run on {{_host}}, which excludes these crates entirely — without this recipe
# nothing ever compiles them and board code silently rots against dependency
# updates.
#
# --release because that is what gets flashed, and because a debug build does
# not fit: the STM32F303CC has 256KB of flash and the unoptimised Voyager
# firmware overflows it. Release is ~78KB, so the headroom is real.
# Cross-compile every board crate and its examples
build-boards:
    # storage/via pull serde+postcard into a no_std build — a combination that
    # silently never compiled until it was checked here.
    cargo check --release -p tecla --features usb,usb-midi,via,storage,embassy --target {{_rp2040_target}}
    cargo build --release -p tecla-rp2040    --lib --examples --features hil,steno-dictionary,usb-midi --target {{_rp2040_target}}
    cargo build --release -p tecla-nrf52840  --lib --examples --features hil --target {{_cortex_m4_target}}
    # BLE needs --no-default-features: nrf-mpsl supplies the critical-section
    # implementation, and cortex-m's would be a duplicate symbol. Scoped to the
    # BLE example — the other examples need cortex-m's, so they cannot be built
    # in the same invocation.
    cargo build --release -p tecla-nrf52840  --lib --example planck_ble --no-default-features --features ble --target {{_cortex_m4_target}}
    cargo build --release -p tecla-stm32f4   --lib --examples --target {{_cortex_m4_target}}
    cargo build --release -p tecla-stm32f3   --lib --examples --features usb-midi --target {{_cortex_m4_target}}

# Clippy the board crates — same rot problem as build-boards.
# Clippy the cross-compiled board crates
lint-boards:
    cargo clippy -p tecla-rp2040   --lib --examples --features hil,steno-dictionary,usb-midi --target {{_rp2040_target}}
    cargo clippy -p tecla-nrf52840 --lib --examples --features hil --target {{_cortex_m4_target}}
    cargo clippy -p tecla-nrf52840 --lib --example planck_ble --no-default-features --features ble --target {{_cortex_m4_target}}
    cargo clippy -p tecla-stm32f4  --lib --examples --target {{_cortex_m4_target}}
    cargo clippy -p tecla-stm32f3  --lib --examples --features usb-midi --target {{_cortex_m4_target}}

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
# Build and flash a board: just flash --tecla-config <profile> (see --list)
flash *args:
    cargo run -p tecla-config --bin flash -- {{args}}

# Check for outdated dependencies
# List dependencies with newer releases (needs cargo-outdated)
outdated:
    cargo outdated {{_host}} --exit-code 1

# Run everything — same gates as CI
ci:
    just fmt-check
    just lint
    just build-boards
    just lint-boards
    just doc
    just hack
    just cov-core
    just cov-all
    just hil
