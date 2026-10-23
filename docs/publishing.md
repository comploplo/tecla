# Publishing Tecla 0.1.0

This repository contains eight publishable crates plus repository-only test
harnesses and compatibility fixtures. Run the archive checks below before tagging
a source release or publishing crates. All publishable Tecla crates and their
internal versioned dependencies use `0.1.0` for this first release.

## License

- Tecla source is dual licensed as `MIT OR Apache-2.0`. The texts live in
  [`LICENSE-MIT`](../LICENSE-MIT) and [`LICENSE-APACHE`](../LICENSE-APACHE),
  symlinked into each publishable crate directory so they land inside the
  `.crate` archive. Third-party notices for vendored material are in
  [`THIRD-PARTY-NOTICES`](../THIRD-PARTY-NOTICES).
- Vendored QMK-derived keyboard data under `keyboards/` is not Tecla source.
  Keep it clearly separated and preserve the upstream license notices for any
  files copied from QMK.
- The Lapwing steno dictionary data (and `lapwing.steno`, compiled from it) is
  MIT-licensed third-party content. Ship its notice from `THIRD-PARTY-NOTICES`
  with any release archive or firmware image that bundles it.

## Crates

- Package and verify `tecla-config`, `tecla`, and `tecla-macros` before the board
  crates. Their path-only development dependencies are stripped by Cargo during
  packaging.
- Before publishing board crates, verify their versioned dependencies, target
  metadata, examples, and [hardware status](../keyboards/tecla/README.md). A
  successful cross-compilation does not establish hardware validation.
- HIL and mutation-discovery harnesses are repository-only and marked
  `publish = false`. They must not become targets of a published library: their
  test inputs deliberately refer to the checkout's board profiles and fixtures.
- `tecla-esp32s3` is `exclude`d from the workspace while its
  `esp-hal`/`esp-rtos` integration awaits an Xtensa firmware build and hardware
  validation. Its standalone dependency graph resolves, but nothing in CI
  compiles it. Do not publish it or treat its code as verified.
- Checkout-dependent integration tests and benchmarks are excluded from the
  library archives. The board crates package the native profile sources their
  firmware examples use, rather than reaching outside the archive to
  `keyboards/`. The RP2040 crate also packages its steno dictionary inputs and
  third-party notice.
- Verify that the workspace's `repository` URL points to the intended public
  repository before publishing. It may differ from a checkout's Forgejo origin;
  crates.io and docs.rs use the package metadata, not the local remote.

## Preflight

```sh
just fmt-check
just lint
just test
just test-doc       # nextest does not execute documentation examples
just doc            # rustdoc with warnings denied
just build-boards   # supported Cortex-M configurations
just lint-boards
just hack           # each host-compatible feature independently
just cov-core
just cov-all
just package-check
```

`just package-check` runs [`scripts/check-packages.py`](../scripts/check-packages.py).
It creates all eight `.crate` archives, extracts them to a temporary directory
outside the checkout, and checks:

- Packaged licenses, versioned dependencies, and target paths.
- Core and tooling libraries/binaries, including the optional dictionary client
  and the core's `no_std` USB/MIDI/storage features.
- Core and tooling documentation examples, compiled and run from the archives.
- All configured Cortex-M examples, including the optional RP2040 core1 demo and
  the nRF52840 BLE configuration.
- Documentation using each crate's docs.rs target and feature settings, with
  warnings denied.

Cargo packages the workspace crates together with `--locked`, leaving the
checkout's lockfile unchanged. Before the crates exist on crates.io, extracted
builds use Cargo patches pointing only to the other extracted release archives.
`cargo package --no-verify` **creates** archives; it is not build verification.
The subsequent checks compile the extracted contents. No source files or Cargo
configuration are borrowed from the checkout, and the script never publishes.
Its build cache remains in `target/package-check` for subsequent runs.

Python 3 and the normal board build prerequisites, including BLE's libclang,
are required. Initialize the dictionary submodule before running the gate;
see [setup](../deps.md). With dependencies already cached, run
`python3 scripts/check-packages.py --offline` to disable registry access.

Hosted Git source ZIP/tar downloads omit submodule contents. Repository builds
need a recursive Git checkout; the verified `.crate` archives include the inputs
their packaged examples require.

`cargo package --list` only lists archive contents. It does not compile them.
After publishing dependencies, run ordinary `cargo package` without
`--no-verify` before publishing each dependent crate. The publication order is
`tecla-config`, `tecla`, `tecla-macros`, then the five Cortex-M board crates;
allow each dependency time to appear in the registry index.

The ordinary board matrix does not cover every feature combination; the package
gate additionally builds the RP2040 `core1-autocorrect` example. ESP32-S3 remains
outside both matrices. Mutation suites are manual and focused; list their scopes
before choosing additional release checks.

Run `just ci` before tagging in an environment with the
[emulation dependencies](../deps.md#emulation). It includes the checks above
and Renode tests. Perform physical hardware checks separately for the boards
included in the release.
