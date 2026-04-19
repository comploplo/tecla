# Tecla Dependencies

This repository is Rust-first. Most day-to-day work only needs the Rust toolchain plus a small set of helper binaries for flashing, HIL, and coverage.

## Required

- `rustup` with the current stable toolchain
- `cargo`
- `just`
- `git`
- `pkg-config` on Linux

## Common developer tools

These are used by the `just` targets and CI-style workflows:

- `cargo-nextest`
- `cargo-hack`
- `cargo-llvm-cov`
- `cargo-outdated`

Install with:

```sh
cargo install cargo-nextest cargo-hack cargo-llvm-cov cargo-outdated
```

## Flashing tools

### RP2040 UF2 boards

- `elf2uf2-rs`

Install with:

```sh
cargo install elf2uf2-rs
```

Linux note:
- `elf2uf2-rs` needs system `libudev` development files available through `pkg-config`.
- On Debian/Ubuntu that usually means `libudev-dev`.
- On Fedora/RHEL-like systems that usually means `systemd-libs` and `systemd-devel`.

### SWD boards

- `probe-rs`

Install with:

```sh
cargo install probe-rs
```

### ESP32-S3 boards

- `espflash`
- the esp-rs toolchain for the target board

Install with:

```sh
cargo install espflash
```

## HIL / emulation

The HIL tests run Renode in a container:

- `podman`, or `docker` if you set `TECLA_CONTAINER_RUNTIME=docker`
- `rust-objdump`
- `rust-nm`

On a standard Rust install, `rust-objdump` and `rust-nm` usually come from:

```sh
rustup component add llvm-tools
```

## Notes

- QMK parsing and transpilation are handled inside the workspace and do not require a separate QMK checkout beyond the vendored `keyboards/` tree.
- Flashing support is split by board family:
  - `just flash --device <usb-device> --qmk-keyboard <keyboard_name>` for QMK-derived boards
  - `just flash --device <usb-device> --tecla-config <profile>` for Tecla-native examples
  - `just flash --list` shows the current native profiles, matching QMK board names,
    USB devices, and mounted UF2 flash targets so you can choose a safe `--device`
