# tecla

An embedded Rust keyboard firmware core.

`no_std` by default. Async-first via [Embassy](https://github.com/embassy-rs/embassy). MCU-agnostic via `embedded-hal`.

**Primary targets:** RP2040 · nRF52840 · STM32F4 · ESP32-S3

______________________________________________________________________

## What it is

Tecla is a clean-room Rust keyboard firmware. Synchronous core — matrix scanning, debounce, layer resolution, tap-hold, combos, tap-dance, leader sequences, steno, autocorrect, mouse keys, dynamic macros, caps word, key overrides — wired to an Embassy async runtime. RGB, audio, haptic, split keyboard, Via/Vial, BLE, MIDI, gamepad. Compile-time keymap macros with QMK-compatible identifiers. Full QMK config import via tree-sitter C parsing.

The goal is to match and exceed QMK's keyboard input core with a type-safe, memory-safe, `unsafe_code = deny` implementation.

______________________________________________________________________

## tecla vs QMK

| | tecla | QMK |
|---|---|---|
| Language | Rust (`no_std`) | C |
| Core source | 24,633 lines | 65,957 lines |
| Test functions | 732 | 472 |
| Tests per kloc | 29.7 | 7.2 |
| `unsafe` blocks | 0 | throughout |
| Keycode resolution | Numeric pipeline via QMK header parsing | Hardcoded C preprocessor |
| QMK config parsing | 4,066/4,066 info.json, 4,341/4,341 keymap.c | N/A (native format) |
| Keycode coverage | 99.97% of 644,959 keycodes resolved to typed variants | N/A |
| Compile-time keymap validation | Yes (proc macro with span-accurate errors) | No |

tecla parses every vendored QMK keyboard configuration and resolves keycodes through the actual QMK header define chain — tree-sitter C parsing, `#include` graph walking, function-like macro expansion, and numeric classification using QMK's documented 16-bit keycode ranges. No hardcoded translation tables.

______________________________________________________________________

## Status

| Phase | What | Status |
|-------|------|--------|
| 1 | Core types — `KeyPos`, `KeyEvent`, `KeyCode`, `Action<Ext>`, `ModifierState` | ✅ |
| 2 | Matrix scanning — `MatrixState`, `MatrixScanner`, `AsyncMatrixScanner` | ✅ |
| 3 | Debounce — `SymDefer`, `AsymEagerDefer` | ✅ |
| 4 | Layer system — `LayerStack`, transparent fallthrough, press-time layer cache | ✅ |
| 5 | Tap-hold — `TapHoldRecognizer` (Default, HoldOnOtherKeyPress, PermissiveHold) | ✅ |
| 6 | Extended recognizers — combo, tap-dance, leader, steno (GeminiPR/TXBOLT) | ✅ |
| 7 | Async orchestrator — `matrix_task`, `keyboard_task`, `usb_task`, NKRO, report coalescing | ✅ |
| 8 | Proc macros — `keymap!`, `layer!`, `MT!`, `LT!`, `#[keyboard_main]` | ✅ |
| 9 | RP2040 board crate — GPIO matrix, USB HID, defmt logging | ✅ |
| 10 | QMK feature parity — repeat key, caps word, key overrides, dynamic macros, autocorrect… | ✅ |
| 11 | Persistent storage — wear-leveled flash, runtime keymap remapping | ✅ |
| 12 | Split keyboard — transport abstraction, UART, primary/secondary half | ✅ |
| 13 | Extended output — MIDI, gamepad, encoder, pointing device | ✅ |
| 14 | RGB / LED / audio / haptic | ✅ |
| 15 | Via/Vial protocol — runtime keymap editing over USB HID | ✅ |
| 16 | QMK config import — `info.json` parser, `keymap.c` transpiler, numeric keycode resolution | ✅ |
| 17 | Wireless — BLE HID on nRF52840, multi-host bonding, power management | 🔜 (compiles, hardware pending) |
| 18 | Additional MCU targets — STM32F4, ESP32-S3 | 🔜 |

See [ROADMAP.md](ROADMAP.md) for the full per-phase breakdown.

______________________________________________________________________

## Architecture

The pipeline has three stages:

```
GPIO pins
   │
   ▼
matrix_task          AsyncMatrixScanner + Debouncer
   │  KeyEvent
   ▼
keyboard_task        LayerStack + TapHoldRecognizer + ModifierState
   │  HidReport
   ▼
usb_task             AsyncReporter (USB HID)
```

Each stage runs as an independent Embassy task connected by typed channels. The keyboard task contains no Embassy dependency — it is a pure synchronous `KeyboardProcessor` that a test harness can drive directly on the host with zero hardware.

### Recognizer trait

Every input disambiguation behaviour (tap-hold, combos, tap-dance, leader, steno) implements a single trait:

```rust
pub trait Recognizer {
    type Output;
    fn observe(&mut self, event: KeyEvent, now_ms: u64) -> Recognition<Self::Output>;
    fn tick(&mut self, now_ms: u64) -> Recognition<Self::Output>;
    fn next_deadline_ms(&self) -> Option<u64>;
    fn reset(&mut self);
}
```

Timing is injected as `now_ms: u64` — no global clock, no Embassy dependency, fully deterministic and testable on the host.

### `Action<Ext>` extension point

The keymap action type is generic over an extension type:

```rust
pub enum Action<Ext = ()> {
    Key(KeyCode),
    Modifier(ModifierKey),
    ModTap { hold: ModifierKey, tap: KeyCode },
    LayerTap { layer: u8, tap: KeyCode },
    ModifiedKey { modifier: ModifierKey, key: KeyCode },
    Layer(LayerOp),
    TapToggle(u8),
    OneShotMod(ModifierKey),
    OneShotLayer(u8),
    Consumer(ConsumerCode),
    Mouse(MouseAction),
    Rgb(RgbCommand),
    Midi(MidiAction),
    CapsWord,
    RepeatKey,
    OutputSelect(OutputMode),
    Ext(Ext),          // ← board-crate extension
    Transparent,
    NoOp,
    // … and more (Audio, Haptic, BLE, Gamepad, Macro, SendString, Sleep, …)
}
```

Board crates supply their own `Ext` type for custom actions without forking the core.

______________________________________________________________________

## Keymap macros

The `tecla-macros` crate provides compile-time keymap declaration with QMK-compatible key identifiers:

```rust
use tecla_macros::{keymap, keyboard_main};

let keymap = keymap! {
    layer!(Base,
        TAB,  Q, W, E, R, T,   Y, U, I, O, P,    BSPC,
        ESC,  A, S, D, F, G,   H, J, K, L, SCLN, QUOT,
        LSFT, Z, X, C, V, B,   N, M, COMM, DOT, SLSH, RSFT,
              LGUI, LT!(Sym, SPC), MT!(LSFT, ENT),   LT!(Nav, SPC), RALT, RGUI
    ),
    layer!(Sym,
        TRNS, N1, N2, N3, N4, N5,   N6, N7, N8, N9, N0,   DEL,
        TRNS, MINS, EQL, LBRC, RBRC, BSLS,   NOOP, NOOP, NOOP, NOOP, NOOP, NOOP,
        TRNS, NOOP, NOOP, NOOP, NOOP, NOOP,   NOOP, NOOP, NOOP, NOOP, NOOP, TRNS,
              TRNS, TRNS, TRNS,   TRNS, TRNS, TRNS
    ),
    layer!(Nav,
        TRNS, NOOP, NOOP, NOOP, NOOP, NOOP,   NOOP, PGUP, UP,   PGDN, NOOP, TRNS,
        TRNS, NOOP, NOOP, NOOP, NOOP, NOOP,   HOME, LEFT, DOWN, RGHT, END,  NOOP,
        TRNS, NOOP, NOOP, NOOP, NOOP, NOOP,   NOOP, NOOP, NOOP, NOOP, NOOP, TRNS,
              TRNS, TRNS, TRNS,   TRNS, TRNS, TRNS
    ),
};

#[keyboard_main]
async fn main(spawner: embassy_executor::Spawner) {
    // spawn tasks, pass KEYMAP to KeyboardProcessor
}
```

### Layer names in `LT!`

Inside `keymap!`, `LT!` accepts layer names instead of numeric indices:

```rust
keymap! {
    layer!(Base, A, B, LT!(Fn, SPC)),  // "Fn" resolves to index 1 at compile time
    layer!(Fn,  F1, F2, TRNS),
}
```

Forward references work — `Base` can reference `Fn` before `Fn` is declared.
Unknown names, duplicate names, and mismatched key counts are all compile errors
with span-accurate diagnostics.

______________________________________________________________________

## Workspace layout

```
tecla/
├── crates/
│   ├── tecla/            # Core library (no_std, 15k lines, 730+ tests)
│   ├── tecla-macros/     # Proc macros (keymap!, layer!, MT!, LT!, #[keyboard_main])
│   ├── tecla-config/     # QMK config parser + keymap transpiler (std, tree-sitter)
│   ├── tecla-rp2040/     # RP2040 board crate (Embassy, USB HID)
│   ├── tecla-nrf52840/   # nRF52840 board crate (BLE, Embassy)
│   ├── tecla-stm32f4/    # STM32F4 board crate (Embassy)
│   └── tecla-esp32s3/    # ESP32-S3 board crate (Embassy)
├── keyboards/            # Vendored QMK keyboard tree (4,341 keymaps)
│   └── quantum/          # QMK core headers (keycode definitions)
│   ⚠ Licensed under GPL-2.0, separate from the MIT/Apache-2.0 tecla code.
├── ROADMAP.md
└── justfile
```

______________________________________________________________________

## Development

```sh
just fmt        # format
just lint       # clippy (pedantic + restriction, all feature combos)
just test       # cargo nextest run --workspace
just hack       # test every feature combination (cargo-hack)
just flash [--device <usb-device>] --qmk-keyboard <keyboard_name>   # flash a QMK-backed board profile
just flash [--device <usb-device>] --tecla-config <profile>         # flash a Tecla-native board profile
just flash --list                           # list supported profiles and UF2-compatible USB devices
just cov        # LLVM coverage report (opens in browser)
just outdated   # fail if any dependency has a newer version
just hil        # hardware-in-the-loop tests via Renode + Podman
just ci         # all of the above — same gates as CI
```

`just flash --device <usb-device> --tecla-config <profile>` builds the matching
board example and flashes it using the configured method. For UF2 and DFU
targets, the tool compiles the firmware first, prints the generated artifact
path, and then waits for a bootloader device so you can put the keyboard into
boot mode after the build succeeds.
`just flash --device <usb-device> --qmk-keyboard <keyboard_name>` resolves a
vendored QMK `keyboard_name` and flashes the matching Tecla-backed profile
when one exists.
`just flash --list` now includes the currently connected USB devices, so you
can choose a safe `--device` value before switching the board into UF2 mode.
`just flash --list` prints the native profiles, matching QMK board names,
USB devices, and currently mounted UF2 flash targets.

### HIL testing

`just hil` runs compiled firmware on emulated MCUs via [Renode](https://renode.io/) inside Podman containers:

- **Boot tests** — RP2040 and nRF52840 firmware initializes `KeyboardProcessor` on emulated hardware
- **Matrix tests** — every key in the Planck keymap is pressed, HID reports read from SRAM via Renode hooks, keycodes verified against expected values

Requires: `podman` (or `docker` via `TECLA_CONTAINER_RUNTIME=docker`), `rust-objdump`, `rust-nm`.

Pre-commit hooks run `just fmt-check`, `just lint`, and `just test` automatically.

______________________________________________________________________

## License and publishing notes

Tecla source is dual licensed under `MIT OR Apache-2.0`; see [LICENSE](LICENSE).

The `keyboards/` tree contains vendored QMK-derived compatibility data and is
licensed separately by its upstream sources. Keep that material separate from
Tecla source releases unless the release format is intended to include the
vendored QMK data and its license notices.

For crate and release preflight details, see [docs/publishing.md](docs/publishing.md).

______________________________________________________________________

## Acknowledgements

Inspired by [QMK Firmware](https://github.com/qmk/qmk_firmware) — the keyboard firmware ecosystem that defined the space.

Built on [Embassy](https://github.com/embassy-rs/embassy) — async embedded Rust done right.

Hat tip to [rmk](https://github.com/HaoboGu/rmk) — a parallel effort in the same space worth knowing about.
