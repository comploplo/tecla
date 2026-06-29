# Publishing Checklist

This repository contains publishable Tecla source plus vendored compatibility
fixtures. Check these items before making a public release or publishing crates.

## License

- Tecla source is dual licensed as `MIT OR Apache-2.0`; the combined license
  text is in [`LICENSE`](../LICENSE).
- Vendored QMK-derived keyboard data under `keyboards/` is not Tecla source.
  Keep it clearly separated and preserve the upstream license notices for any
  files copied from QMK.
- The Lapwing steno dictionary data is third-party content. Verify its upstream
  license before bundling it into release archives or binary firmware images.

## Crates

- Publish the first crates in dependency order:
  1. `tecla-config`
  2. `tecla`
  3. `tecla-macros`
- Board support crates depend on specific embedded targets and examples; publish
  them only after their hardware bring-up status is reflected in the README.
- HIL tests live in the private `crates/tecla-hil-tests` crate. It is kept out
  of the workspace and marked `publish = false` because it depends on the local
  `../../../hilt` checkout. The `just hil-*` targets run it by manifest path.

## Preflight

```sh
just fmt-check
just test
cargo package -p tecla --list
cargo package -p tecla-config --list
cargo package -p tecla-macros --list
```

Run `just ci` before tagging when HIL hardware/emulation dependencies are
available.
