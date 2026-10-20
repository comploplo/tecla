# Publishing Checklist

This repository contains publishable Tecla source plus vendored compatibility
fixtures. Check these items before making a public release or publishing crates.

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

- Publish the first crates in dependency order:
  1. `tecla-config`
  2. `tecla`
  3. `tecla-macros`
- Board support crates depend on specific embedded targets and examples; publish
  them only after their hardware bring-up status is reflected in the README.
- HIL tests live in `crates/tecla-hil-tests`. It has its own workspace and is
  marked `publish = false` because it is a repo-local test harness, not a
  library anyone should depend on. The `just hil-*` targets run it by manifest
  path.
- The nRF BLE stack uses published `trouble-host` 0.8 and `nrf-sdc`/`nrf-mpsl`
  0.4 releases with matching `bt-hci` 0.10 types.
- `tecla-esp32s3` is `exclude`d from the workspace while its current
  `esp-hal`/`esp-rtos` migration awaits an Xtensa firmware build and hardware
  validation. Its standalone dependency graph resolves, but nothing in CI
  compiles it. Do not publish it or treat its code as verified.
- `tecla-config` sets `exclude = ["tests/"]`: its integration tests assert
  against `../../keyboards`, a symlink tree into the git submodules, which does
  not exist inside a `.crate` archive.
- `repository` points at `https://github.com/comploplo/tecla`, which does not
  exist yet — push the repo before publishing, or crates.io and docs.rs will
  both link to a 404.
- Allow crates.io time to index each dependency before verifying or publishing
  the next crate. `tecla` has a dev-dependency on `tecla-config`, and
  `tecla-macros` has a dev-dependency on `tecla`.

## Preflight

```sh
just fmt-check
just lint
just test
just doc            # rustdoc -D warnings; docs.rs builds fail otherwise
just build-boards   # nothing else in CI compiles the board crates
just cov-core
just cov-all
cargo package -p tecla-config
# Publish tecla-config, then wait for it to appear in the crates.io index.
cargo package -p tecla
# Publish tecla, then wait for it to appear in the crates.io index.
cargo package -p tecla-macros
```

`cargo package --list` only inspects archive contents; it does not perform the
package build verification used by the commands above.

Run `just ci` before tagging when HIL hardware/emulation dependencies are
available.
