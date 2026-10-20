# tecla

Keyboard firmware written in Rust. `no_std`, async via
[Embassy](https://github.com/embassy-rs/embassy), and independent of the MCU
through `embedded-hal`.

> [!IMPORTANT]
> Tecla is pre-release software. The core is well tested on the host, but the
> public API and board crates will change before 1.0, and only some boards have
> been run on real hardware. Check [Supported boards](#supported-boards) before
> flashing anything you rely on.

## Features

- Layers, tap-hold (mod-tap / layer-tap), combos, one-shot keys, Caps Word,
  repeat key, key overrides, dynamic macros
- Steno with an on-board dictionary (Lapwing), so the keyboard types words
  without Plover
- Mouse keys, media keys, MIDI, gamepad, rotary encoders, RGB, audio
- Split keyboards over UART, NKRO, persistent keymaps in flash, and a subset of
  the Via protocol
- Keymaps written with QMK key names and checked at compile time
- Importers for QMK (`info.json`, `keymap.c`) and ZMK configurations
- `unsafe_code = deny` across the workspace; the input pipeline is synchronous
  with injected timestamps, so it runs and is tested on the host without
  hardware

## Supported boards

Each board is a profile under [`keyboards/tecla/`](keyboards/tecla/).
The [native board guide](keyboards/tecla/README.md) lists every profile,
including the Asterisk and Voyager instruments with their controls and ranges.

| Profile | Keyboard | MCU | Flash method | Status |
|---------|----------|-----|--------------|--------|
| `asterisk_v1_4` | StenoKeyboards Asterisk v1.4 (steno) | RP2040 | UF2 | Runs on hardware |
| [`asterisk_v1_4_midi`](keyboards/tecla/asterisk_v1_4_midi/) | Asterisk v1.4 MIDI pad controller | RP2040 | UF2 | Basic playing confirmed on hardware |
| [`asterisk_v1_4_autochord`](keyboards/tecla/asterisk_v1_4_autochord/) | Asterisk four-voice harmony instrument | RP2040 | UF2 | Builds; hardware validation pending |
| `steno_uni_v4` | StenoKeyboards Uni v4 (steno) | RP2040 | UF2 | Builds |
| `planck-rp2040` | Planck-style 4×12 | RP2040 | probe-rs | Builds |
| `split-rp2040` | Split keyboard over UART | RP2040 | probe-rs | Builds |
| `voyager` | ZSA Voyager | STM32F303 | DFU | Boots and enumerates over USB; bring-up in progress |
| `planck-stm32f4` | Planck-style 4×12 | STM32F411 | probe-rs | Builds |
| `planck-nrf52840` | Planck-style 4×12 | nRF52840 | probe-rs | Builds |
| `planck-nrf52840-ble` | Planck-style 4×12, Bluetooth | nRF52840 | probe-rs | Builds; not yet tested on hardware |

"Builds" means CI compiles the firmware but it has not been run on a real
keyboard. Separately, RP2040 and nRF52840 test firmware runs in the Renode
emulator in CI. There are also diagnostic profiles (`asterisk_v1_4_keytest`,
`voyager_matrix_test`, `voyager_usbtest`, `voyager_i2c_scan`,
`voyager_trackball`) used during bring-up. ESP32-S3 remains outside the
workspace and CI; its current `esp-hal`/`esp-rtos` migration still needs an
Xtensa firmware build and hardware validation.

## Quick start: flash a board

1. Install [rustup](https://rustup.rs). The repository pins its Rust version
   and embedded targets in `rust-toolchain.toml`; rustup installs them
   automatically the first time you build.

2. Install the flashing tool for your board (see [deps.md](deps.md) for Linux
   packages):

   ```sh
   cargo install just
   cargo install elf2uf2-rs   # UF2 boards (RP2040)
   cargo install probe-rs     # probe-rs boards (needs an SWD debug probe)
   cargo install cargo-binutils  # DFU boards (Voyager), plus dfu-util from your package manager
   ```

3. Clone the repository. Steno boards also need the Lapwing dictionary
   submodule:

   ```sh
   git clone https://github.com/comploplo/tecla
   cd tecla
   git submodule update --init .vendor/plover-lapwing-aio
   ```

4. Flash:

   ```sh
   just flash --list                        # profiles, connected USB devices, mounted UF2 drives
   just flash --tecla-config asterisk_v1_4  # build and flash one profile
   ```

   The firmware is built first; then the tool waits for the board's
   bootloader. For RP2040 boards, hold the BOOT button **only while plugging
   the board in**, then release it. A drive called `RPI-RP2` appears and the
   tool copies the firmware onto it. If more than one bootloader drive is
   connected, pass `--device <name>` (from `--list`).

   On RP2040 boards the bootloader is in ROM, so a bad flash cannot brick the
   board; to go back to the vendor firmware, copy its `.uf2` file onto the
   `RPI-RP2` drive.

Steno boards type words themselves using the built-in dictionary, so turn
Plover off while you use them.

## Writing a keymap

Keymaps use the `keymap!` macro from `tecla-macros` with QMK key names. Key
count, layer names, and key names are checked at compile time.

```rust
let keymap = keymap! {
    layer!(Base,
        TAB,  Q, W, E, R, T, Y, U, I, O, P,    BSPC,
        ESC,  A, S, D, F, G, H, J, K, L, SCLN, ENT,
        LSFT, Z, X, C, V, B, N, M, COMM, DOT,  SLSH, RSFT,
        LCTL, LGUI, LALT, LSFT, LT!(Fn, SPC), SPC, SPC, RSFT, LEFT, DOWN, UP, RGHT
    ),
    layer!(Fn,
        GRV,  N1, N2, N3, N4,  N5,  N6,  N7,   N8,  N9,   N0,   DEL,
        TRNS, F1, F2, F3, F4,  F5,  F6,  MINS, EQL, LBRC, RBRC, BSLS,
        TRNS, F7, F8, F9, F10, F11, F12, TRNS, TRNS, TRNS, TRNS, TRNS,
        TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS, TRNS
    ),
};
let processor = KeyboardProcessor::new(keymap, TapHoldConfig::default());
```

- `LT!(layer, key)` is layer-tap and `MT!(modifier, key)` is mod-tap. Layers
  are referenced by name, including layers declared later.
- `TRNS` falls through to the layer below; `NOOP` blocks it.
- Steno boards use `steno!`, which maps each key to a steno key instead.

The full list of key names is in the
[`tecla-macros` docs](crates/tecla-macros/src/lib.rs). For a complete board,
start from an existing example such as
[`crates/tecla-rp2040/examples/planck.rs`](crates/tecla-rp2040/examples/planck.rs).

## Feature status

| Area | Status | Notes |
|------|--------|-------|
| Matrix scanning, debounce, layers | ✅ | |
| Tap-hold (mod-tap, layer-tap, tap-toggle) | ✅ | Quick-tap and require-prior-idle send the tap on release, so holding a quick-tapped key does not auto-repeat |
| Combos | ✅ | Fire as soon as the held keys match; no timing window |
| Caps Word, repeat key, key overrides, one-shot keys, dynamic macros | ✅ | |
| Steno with on-board dictionary | 🚧 | Single-stroke words work. Multi-stroke words whose first stroke is also a word are not reachable yet, and some Plover formatting (`{re^}`, `{-\|}`, `{>}`) is translated incorrectly |
| Autocorrect | 🚧 | Known bugs: corrections can corrupt following words, especially after backspace. Leave it disabled for now |
| Mouse, media, MIDI, gamepad, encoders | ✅ | |
| RGB, audio | ✅ | Audio driver: RP2040 PWM buzzer |
| Haptics | 🚧 | Controller and task only; no board driver yet |
| Persistent storage (wear-leveled flash) | ✅ | |
| Split keyboards (UART) | ✅ | |
| Via | 🚧 | Keymap get/set/reset, layer count, keyboard values. No macros, lighting, or encoder commands; no Vial |
| Bluetooth (nRF52840) | 🚧 | Builds in CI; not yet tested on hardware |
| QMK / ZMK config import | ✅ | Tested against pinned QMK and ZMK checkouts |

See [ROADMAP.md](ROADMAP.md) for the detailed plan.

## How it works

```
GPIO pins
   │
   ▼
matrix_task      scan + debounce          → KeyEvent
   │
   ▼
keyboard_task    KeyboardProcessor        → HidReport
   │             (layers, tap-hold, combos, steno, …)
   ▼
usb_task         USB HID reporter
```

The three stages are Embassy tasks connected by channels. `KeyboardProcessor`,
the middle stage, is plain synchronous Rust: it takes key events with a
timestamp and emits HID reports through a callback. It has no clock and no
Embassy dependency, which is why the whole input pipeline can be driven and
tested on a desktop.

Keymap actions are `Action<Ext>`. A board crate can supply its own `Ext` type
to add custom actions without forking the core.

### Repository layout

```
crates/
  tecla/            core library (no_std): input pipeline, reports, telemetry
  tecla-macros/     keymap!, layer!, steno!, MT!, LT!, #[keyboard_main]
  tecla-config/     QMK/ZMK importers, steno dictionary compiler, `flash` tool
  tecla-rp2040/     board crates: MCU setup, drivers, and example firmware
  tecla-stm32f3/
  tecla-stm32f4/
  tecla-nrf52840/
  tecla-esp32s3/    separate workspace; Xtensa build remains unverified
  tecla-hil-tests/  emulation tests (separate workspace)
keyboards/tecla/    board profiles used by `just flash`
.vendor/            QMK, ZMK, and Lapwing submodules
```

## Development

The QMK and ZMK compatibility tests need all submodules (about 1 GB):

```sh
git submodule update --init --recursive
cargo install cargo-nextest
just test
```

Tests run on the host; no embedded target or hardware is needed. Run
`just --list` for every recipe. The common ones:

| Command | What it does |
|---------|--------------|
| `just test` | Run the host test suite |
| `just mutants-instruments` | Check Chromatica and Voyager note-map tests against deliberate logic changes |
| `just lint` | Clippy on the host crates |
| `just fmt` | Format the code |
| `just build-boards` | Cross-compile every board crate and example |
| `just doc` | Build the docs, failing on broken links |
| `just cov` | Coverage report in the browser |
| `just hil` | Run firmware in the Renode emulator (needs Podman or Docker) |
| `just ci` | Everything CI runs: format, lint, board builds, docs, per-feature tests, coverage gates, emulation |

CI requires at least 95% line coverage for the `tecla` core crate and 90% for
the host workspace. [deps.md](deps.md) lists the tools each recipe needs.

For instrument mutation testing, install `cargo-mutants` with
`cargo install cargo-mutants --version 27.1.0 --locked`, then run `just mutants-instruments`.
It tests a scratch copy and writes results under `target/mutations/mutants.out`.
Use `just mutants-instruments --list` to preview the mutations without running them.

Forgejo workflows are in [`.forgejo/workflows/`](.forgejo/workflows/).
They check host tests, lint, documentation, firmware builds, BLE, and coverage
on pushes to `main` and pull requests. Instrument mutation testing is a
separate manual workflow. Enable Actions for the repository and provide a
container-capable runner labelled `ubuntu-latest`; each job installs its own
pinned Rust toolchain.

## License

Tecla is dual licensed under [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.

The QMK, ZMK, and Lapwing material in `.vendor/` and `keyboards/` belongs to
its upstream projects and keeps their licenses; see
[THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES). Firmware built with the steno
dictionary includes Lapwing data, which is MIT-licensed. Maintainers: see
[docs/publishing.md](docs/publishing.md) before cutting a release.

## Acknowledgements

- [QMK](https://github.com/qmk/qmk_firmware), which defined what keyboard
  firmware can do; Tecla's key names, keycode values, and much of its feature
  set follow QMK, and its keyboard definitions are the importer's test fixtures
- [ZMK](https://github.com/zmkfirmware/zmk), whose shield and keymap formats
  Tecla imports and whose board definitions are its test fixtures
- [Embassy](https://github.com/embassy-rs/embassy), the async embedded runtime
  Tecla is built on
- [rmk](https://github.com/HaoboGu/rmk), another Rust keyboard firmware worth
  knowing about
- [Lapwing](https://github.com/aerickt/plover-lapwing-aio), the steno theory
  and dictionary used on steno boards
