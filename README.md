# tecla

Tecla is a Rust firmware framework for keyboards, musical instruments, and
other input controllers. It provides reusable input processing, output
protocols, feedback, and hardware integrations so a device can have its own
interaction model without rebuilding the firmware around it.

The core runs under `no_std` with fixed-capacity data structures. Its processing
logic is synchronous and takes timestamps as input, so it can be tested on a
desktop. Board crates supply drivers through the `embedded-hal` ecosystem and
use [Embassy](https://embassy.dev) for asynchronous scanning and communication.

## Capabilities

- **Keyboard behavior:** layers, tap-hold, combos, one-shot keys, Caps Word,
  repeat, key overrides, dynamic macros, configurable slow/bounce/sticky keys,
  and steno with an onboard dictionary.
- **Input:** matrix and direct-pin scanning, capacitive keys, rotary encoders,
  and pointing devices. Analog scanning interfaces, calibration, filtering,
  rapid trigger, and dynamic keystroke primitives are available; a Hall-effect
  board integration remains to be built.
- **Output:** USB HID keyboard, mouse, media, and gamepad reports; USB MIDI;
  UART split communication; and an experimental nRF52840 Bluetooth path.
- **Musical systems:** reusable pitch, scale, chord, voice-leading, harmonic
  movement, and Tonnetz primitives for building different instruments.
  Musical output goes to an external synthesizer or DAW over MIDI.
- **Feedback and configuration:** RGB lighting, PWM buzzer feedback,
  flash-backed keymaps, a subset of Via, and optional telemetry including
  active typing statistics and a typed WPM readout.
- **Authoring tools:** compile-time keymap checks with QMK key names,
  QMK/ZMK configuration import, dictionary compilation, and a shared
  build-and-flash tool.

Features are composed by each firmware profile. A controller can reuse the
scanners, report types, MIDI transport, and lighting with its own state machine;
it does not have to behave like a typing keyboard.

## Architecture

```text
physical inputs → scan / debounce → device behavior → reports / transport
                                         │
                                         └→ lighting, feedback, telemetry
```

For typing firmware, `KeyboardProcessor` turns timestamped key events into HID
reports and handles layers, tap-hold, combos, and text playback. Embassy tasks
connect it to scanners and reporters. Instrument profiles use their own
behavior while sharing the hardware and protocol code.

The reusable logic lives in [`crates/tecla`](crates/tecla/). The
[`tecla-macros`](crates/tecla-macros/) crate provides keymap syntax, and
[`tecla-config`](crates/tecla-config/) handles import, dictionary compilation,
and flashing. MCU crates contain board drivers and firmware entry points;
[`keyboards/tecla`](keyboards/tecla/) contains the native profiles and wiring.

### A small keymap

```rust
use tecla::prelude::*;
use tecla_macros::{keymap, layer, LT};

let keymap: LayerStack<2, 4, ()> = keymap! {
    layer!(Base, A, B, C, LT!(1, SPC)),
    layer!(Fn,   F1, F2, F3, TRNS),
};

let processor: KeyboardProcessor<1, 4, 2, 4> =
    KeyboardProcessor::new(keymap, TapHoldConfig::default());
```

`LT!` makes a key tap Space or hold a layer; `MT!` provides modifier-tap.
`TRNS` falls through to the layer below. Layer counts, key counts, and names
are checked at compile time. See the [macro documentation](crates/tecla-macros/src/lib.rs)
and [board examples](crates/tecla-rp2040/examples/) for complete firmware.

## Hardware and status

The main workspace has board crates for RP2040, nRF52840, and STM32F0/F3/F4.
Physical keyboards, reference designs, instrument profiles, controls, and flash
instructions are documented in the [native board guide](keyboards/tecla/README.md).
That guide also distinguishes hardware-confirmed operation from build-only
support. ESP32-S3 lives in a separate workspace and still needs an Xtensa
firmware build and hardware validation.

Tecla 0.1.0 is the first public release. Host tests cover the core extensively,
while hardware validation varies by board. Bluetooth builds but awaits physical
validation; haptics has a controller/task interface without a board driver.
Via supports keymap editing, not its full protocol.

Some text features have known limits: ambiguous multi-stroke steno words and
some Plover formatting are unresolved. Autocorrect can corrupt subsequent text,
especially after backspace, and should remain disabled. See the
[roadmap](ROADMAP.md) for implemented foundations and remaining work.

## Getting started

[`rust-toolchain.toml`](rust-toolchain.toml) pins Rust and the embedded targets.
Follow [development setup](deps.md#setup) for prerequisites, then use the
[board guide](keyboards/tecla/README.md#flashing) to build and flash a
profile. From a configured checkout:

```sh
just flash --list
just flash --tecla-config <profile>
```

For development, initialize the submodules containing compatibility fixtures
and the steno dictionary, then run the host checks:

```sh
git submodule update --init --recursive
just test
just lint
just doc
```

`just build-boards` cross-compiles the configured Cortex-M firmware examples;
`just lint-boards` checks those configurations with Clippy. `just --list` also
lists coverage, per-feature tests, focused mutation tests, and Renode emulation.
Tool requirements and platform limits are in [deps.md](deps.md).

CI checks host tests, documentation, lint, board builds, and coverage. Coverage
gates are 95% for the core and 90% for the host workspace; these measure code
exercise rather than physical hardware behavior. Forgejo's focused mutation
tests run through a separate manual workflow.

## Documentation

- [Board and profile guide](keyboards/tecla/README.md) — supported hardware,
  controls, flashing, and typing statistics.
- [Development tools](deps.md) — setup, optional checks, and emulation.
- [Roadmap](ROADMAP.md) — implementation status and planned work.
- [Adaptive UX design](docs/adaptive-ux.md) — proposed behavior, research, and
  the boundary between observation and adaptation.
- [Publishing](docs/publishing.md) — release checks and crate publication.

## License and acknowledgements

Tecla is dual licensed under [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. Imported QMK, ZMK, and Lapwing
material retains its upstream licenses; see
[THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES). Steno firmware includes the
MIT-licensed Lapwing dictionary.

Tecla draws on [QMK](https://github.com/qmk/qmk_firmware) for key names and
keyboard conventions, [ZMK](https://github.com/zmkfirmware/zmk) for imported
configurations, [Embassy](https://github.com/embassy-rs/embassy) for its runtime,
and [Lapwing](https://github.com/aerickt/plover-lapwing-aio) for steno theory and
dictionary data. QMK and ZMK checkouts also serve as compatibility fixtures.
