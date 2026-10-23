# Development tools

Rust dependencies are declared in the workspace manifests and locked in
[`Cargo.lock`](Cargo.lock). [`rust-toolchain.toml`](rust-toolchain.toml) pins the
compiler, Clippy, rustfmt, LLVM tools, and Cortex-M targets.

## Setup

Install `rustup`, `git`, `just`, and a native C compiler/linker. Linux builds
also use `pkg-config`. From the repository root:

```sh
rustup toolchain install
git submodule update --init --recursive
cargo install --locked just cargo-nextest
just test
```

The submodules provide QMK/ZMK compatibility fixtures and the Lapwing steno
dictionary. Parsing and transpilation run inside this workspace; a separate QMK
installation is unnecessary.

## Optional checks

Install only the tools needed for the checks you run:

| Tool | Commands |
|---|---|
| `cargo-hack` | `just hack`: tests each host-compatible feature separately |
| `cargo-llvm-cov` | `just cov`, `just cov-core`, `just cov-all` |
| `cargo-outdated` | `just outdated` |
| `cargo-mutants` 27.1.0 | `just mutants-instruments`, `just mutants-typing`, `just mutants-harmony` |

```sh
cargo install --locked cargo-hack cargo-llvm-cov cargo-outdated
cargo install --locked cargo-mutants --version 27.1.0
```

Append `--list` to a mutation recipe to inspect its scope. These are focused
mutation suites, not a whole-repository sweep. Forgejo runs them on manual
workflow dispatch; ordinary CI runs host checks, board builds, and coverage.

Host checks exclude board crates and the `usb`/`usb-midi` features, which cannot
be combined with the host `std` build. `just build-boards` and `just lint-boards`
check the supported Cortex-M configurations separately. ESP32-S3 is excluded
from the workspace pending firmware and hardware validation.

`just package-check` also needs Python 3.9 or newer and the Lapwing submodule.
It builds and documents all eight publishable crates from extracted archives,
including their firmware examples and the BLE configuration below. Use
`just package-check --offline` when all Rust dependencies are already cached.

The nRF52840 BLE build also needs `libclang` for bindgen (for example,
`libclang-dev` on Debian/Ubuntu or Xcode's toolchain on macOS). Set
`LIBCLANG_PATH` to your installed library directory if it is not discovered.

## Flashing

`just flash --list` lists native profiles, matching QMK board names, connected
USB devices, and mounted UF2 targets. See the
[board and profile guide](keyboards/tecla/README.md) for hardware-specific steps.

```sh
just flash --tecla-config <profile>
just flash --device <usb-device> --qmk-keyboard <keyboard_name>
```

Use `--device` to select a particular device when needed. DFU profiles also
support `--dry-run` to build, convert, validate, and print the flash command
without accessing a device.

| Board family | Additional tools |
|---|---|
| RP2040 UF2 | `cargo install --locked elf2uf2-rs` |
| DFU (Voyager, NOS75) | `dfu-util` plus an objcopy implementation |
| SWD | `cargo install --locked probe-rs-tools` |
| ESP32-S3 (unverified) | `espflash` and a separate esp-rs Xtensa toolchain |

Install `dfu-util` with your package manager. `cargo install --locked cargo-binutils`
provides `rust-objcopy`; `llvm-objcopy` and `arm-none-eabi-objcopy` are also accepted.
On Linux, `elf2uf2-rs` needs libudev development files visible through
`pkg-config` (for example, `libudev-dev` on Debian/Ubuntu).

## Emulation

The `just hil-*` recipes run firmware in Renode containers. They require a
working Podman runtime, or Docker with `TECLA_CONTAINER_RUNTIME=docker`, plus
`rust-objdump` and `rust-nm` from `cargo-binutils`:

```sh
cargo install --locked cargo-binutils
```

The matrix recipes currently run their test harness with the
`x86_64-unknown-linux-gnu` target, so use an appropriate Linux environment.
`just ci` includes these emulation recipes and per-feature checks in addition
to the ordinary CI gates; it requires their extra tools. Emulation does not
replace physical hardware testing.
