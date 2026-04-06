# Tecla Roadmap

An embedded Rust keyboard firmware core — `no_std`, async-first via Embassy,
MCU-agnostic via `embedded-hal` and `embedded-hal-async`.

**Primary targets:** RP2040, STM32, nRF52840, ESP32-S3 — modern MCUs
with ample flash and RAM, all served by mature Rust HAL ecosystems.

**Design goals:**

- Match and exceed QMK's keyboard input core with a type-safe, memory-safe,
  fully-tested Rust implementation
- One unified `Recognizer` trait for all input disambiguation — tap-hold,
  combo, leader, tap-dance, steno share a single interface
- `Action<Ext>` generic extension point: third-party features without forking
- Embassy async pipeline eliminates busy-waiting; scan → process → report
  with zero CPU burn between events
- `unsafe_code = deny` across the entire workspace — memory safety by
  construction, not convention

**AVR (ATmega32u4):** stretch goal, feature-flagged, after all primary phases.

______________________________________________________________________

## Phase 1 — Core Types and Abstractions ✅

- [x] `event.rs` — `KeyPos`, `KeyEvent` (press/release with timestamp)
- [x] `action.rs` — `KeyCode` (HID-aligned), `ModifierKey` (bitmask),
  `ConsumerCode`, `LayerOp`, `Action<Ext>` generic extension point
- [x] `modifier.rs` — `ModifierState` (real / weak / oneshot stacks)
- [x] `recognizer.rs` — `Recognizer` trait + `Recognition<T>` — the unifying
  primitive for all input disambiguation
- [x] `report.rs` — `KeyboardReport` (6KRO), `NkroKeyboardReport` (128-key
  bitmap), `ConsumerReport`, `HidReport`
- [x] `hid.rs` — `Reporter` / `AsyncReporter` output boundary traits

______________________________________________________________________

## Phase 2 — Matrix Scanning ✅

- [x] `matrix.rs` — `MatrixState<ROWS, COLS>` with safe accessors,
  `MatrixScanner` and `AsyncMatrixScanner` traits
- [x] Row-strobe / col-read GPIO implementation (`GpioMatrixScanner`)
- [x] Direct-pin implementation (`DirectPinScanner`)

______________________________________________________________________

## Phase 3 — Debounce ✅

- [x] `Debouncer<ROWS, COLS>` trait
- [x] `SymDefer` — symmetric global-timer defer
- [x] `AsymEagerDefer` — eager press, deferred release (per-key)

______________________________________________________________________

## Phase 4 — Layer System ✅

- [x] `Layer<Ext>`, `LayerStack<LAYERS, KEYS, Ext>`
- [x] Highest-active-layer wins, transparent fallthrough, `NoOp` blocks
- [x] Press-time layer cache (prevents stuck keys on layer change mid-hold)
- [x] `LayerOp` dispatch — momentary, toggle, set-default, on, off

______________________________________________________________________

## Phase 5 — Tap-Hold ✅

- [x] `TapHoldRecognizer<BUF>` implements `Recognizer`
- [x] Default (timeout), `HoldOnOtherKeyPress`, `PermissiveHold` modes
- [x] Configurable `tapping_term_ms`, waiting buffer, `drain_pending` replay
- [x] One-shot modifiers and one-shot layers in `KeyboardProcessor`

______________________________________________________________________

## Phase 6 — Extended Recognizers ✅

- [x] `LeaderRecognizer` — Emacs-style nested trie sequences, per-branch
  timeouts, which-key metadata for display integration
- [x] `ComboRecognizer` — simultaneous key-set within a configurable window
- [x] `TapDanceRecognizer` — counted rapid taps → action per count
- [x] `StenoRecognizer` — chord until all-release, GeminiPR / TXBOLT wire
  format (dictionary stays on host in Plover)

______________________________________________________________________

## Phase 7 — Keyboard Orchestrator ✅

- [x] `KeyboardProcessor` — synchronous event → report core, no Embassy dep
- [x] `tecla::task::run()` — single-task async Embassy entry point
- [x] `matrix_task` / `keyboard_task` / `usb_task` — three-task topology
  wired by `EventChannel` / `ReportChannel` (generic over `RawMutex`)
- [x] Tap-hold deadline raced via `embassy_time::with_timeout`
- [x] `RolloverMode` — switch between 6KRO and NKRO at runtime
- [x] Report coalescing — `usb_task` skips identical consecutive reports

______________________________________________________________________

## Phase 8 — Proc Macros ✅

- [x] `keymap!` — emit a `LayerStack` from a declarative key table; all layers
  must have the same key count (compile-time error otherwise)
- [x] `layer!` — emit a `Layer` from a comma-separated key identifier list
- [x] `MT!(MOD, KEY)` / `LT!(LAYER, KEY)` — mod-tap and layer-tap shorthands;
  usable standalone or inline within `layer!` / `keymap!`
- [x] Layer name resolution — `LT!(Fn, SPC)` inside `keymap!` resolves the
  name to its index at macro-expand time; forward references supported
- [x] `#[keyboard_main]` — wraps `async fn main` with `#[embassy_executor::main]`
- [x] Full HID key identifier table (~80 QMK-compatible aliases: `A`…`Z`,
  `F1`…`F12`, `SPC`, `ENT`, `LSFT`, `TRNS`, `NoOp`, numpad, nav, …)
- [x] Compile-time validation — layer count, key count, layer name bounds,
  duplicate name detection with span-accurate error diagnostics

______________________________________________________________________

## Phase 9 — RP2040 Hardware Validation ✅

First real hardware. Validates every architectural decision end-to-end.

- [x] `tecla-rp2040` crate — depends on `embassy-rp`
- [x] Row-strobe / col-read GPIO matrix scanner (`GpioMatrixScanner`)
- [x] Direct-pin matrix scanner (`DirectPinScanner`)
- [x] USB HID via `embassy-usb` (`UsbHidReporter`, 6KRO)
- [x] RTT logging via `defmt` + `probe-rs` (defmt-rtt, probe-rs runner)
- [x] UF2 artifact for drag-and-drop flashing
- [x] Example: 4×12 ortholinear layout (Planck-style)
- [x] Example: split-ready wiring (foundation for Phase 12)

______________________________________________________________________

## Phase 10 — QMK Feature Parity: Core Input

Close the gap with QMK's keyboard input core. Each item is a self-contained
`Recognizer` or `KeyboardProcessor` feature.

- [x] **Repeat key** — `Action::RepeatKey`, replays last non-repeat keycode
- [x] **Caps Word** — `Action::CapsWord`, auto-shifts A–Z, deactivates on
  non-continue keys (space, enter, escape, tab, symbols)
- [x] **Key overrides** — `KeyOverride` struct, intercept key+modifier combos
  and remap to replacement actions; configured via `with_overrides()`
- [x] **Layer lock** — `Action::LayerLock(n)`, toggle-locks a layer persistently
- [x] **Tri-layer** — `TriLayerDef`, auto-activates target layer when two
  source layers are both active; configured via `with_tri_layers()`
- [x] **Dynamic macros** — `MacroRecord(slot)` / `MacroStop` / `MacroPlay(slot)`,
  4 slots × 64 keycodes, stored in RAM
- [x] **Send String** — `Action::SendString(&'static [KeyCode])`, emits a
  static keycode sequence as if typed
- [x] **Autocorrect** — implemented in Phase 14 (trie-based, edit-distance matching)
- [x] **Mouse keys** — `Action::Mouse(MouseAction)`, HID mouse report for
  movement, click, and scroll; `MouseReport` type added
- [x] **Bootmagic** — `Action::Bootmagic`, sets a flag checked at startup

______________________________________________________________________

## Phase 11 — Persistent Storage

Save dynamic keymaps, per-key config, and runtime state across power cycles.

- [x] `storage.rs` — async storage via `sequential-storage`, `StorageKey` enum,
  `PersistedState`, `load_state()` / `save_state()` behind `storage` feature
- [x] Wear-leveling — delegated to `sequential-storage` (log-structured flash)
- [x] Persist: default layer, locked layers, rollover mode, dynamic macro slots
- [x] Runtime keymap remapping — `Layer::set()`, `LayerStack::set_action()`,
  `KeyboardProcessor::restore_state()`
- [x] EEPROM emulation shim — `EepromShim` with `read_byte`/`write_byte`, wired to Via protocol

______________________________________________________________________

## Phase 12 — Split Keyboard

Half of the custom keyboard market. Needs its own phase.

- [x] `SplitTransport` trait — async send/recv for `KeyEvent`s
- [x] `remote_event_task` — primary half receives from transport into EventChannel
- [x] `secondary_task` — secondary half scans + debounces + sends over transport
- [x] Half-identification — `SplitRole` enum + `SplitConfig` with col_offset remapping
- [x] USB arbitration — split.rs example shows primary-only USB enumeration
- [x] `UartTransport` — async UART impl over `embedded-io-async`, 11-byte framed KeyEvent
- [ ] Validated targets: Corne (3×6), Kyria (3×6+5t), Lily58 (4×6)

______________________________________________________________________

## Phase 13 — Extended Output

The `Action<Ext>` hook makes these architectural additions, not bolt-ons.

- [x] MIDI support — `MidiAction`, `MidiEvent`, `HidReport::Midi` + auto note-off on release
- [x] Gamepad support — `GamepadAction`, `GamepadReport`, `HidReport::Gamepad` + button/axis tracking
- [x] Composite output — `HidReport` enum IS the composite; board crates route by variant
- [x] Encoder support — `encoder_task` routes rotations as virtual key positions
  through the full `KeyboardProcessor` pipeline, layer-resolved per encoder
- [x] Pointing device — `PointingEvent`, `AsyncPointingScanner`, `pointing_task`,
  `pointing_event_to_report`; sends `HidReport::Mouse` to report channel

______________________________________________________________________

## Phase 14 — Autocorrect and Autocomplete

Deferred from Phase 10. Pure core logic, no hardware deps.

- [x] `AutocorrectEngine` — trie-based dictionary, circular keystroke buffer,
  word-boundary-triggered correction with edit-distance matching,
  `Action::AutocorrectToggle`
- [x] Rollover transposition detection via `check_transposed()` (opt-in)
- [x] `AutocompleteEngine` — trie-based prefix walking, frequency-ranked
  suggestions, `Action::AutocompleteAccept` / `AutocompleteToggle`
- [x] Default dictionary — `tecla_config::dictionary::fetch_and_compile()` downloads SCOWL and compiles to trie
- [x] Keyboard-proximity weighted matching — `proximity::generate_proximity_table()` + `AutocorrectEngine::with_proximity()`

Future exploration (not in scope now, may revisit):

- Human debouncing — with proximity metrics + full dictionary trie, detect
  accidental adjacent-key presses, letters out of order, spacebar before last
  char or after first char. Deterministic rules, slim code. Experiment once
  proximity and dictionary are in place.
- WPM-adaptive correction aggressiveness (CHI 2018: 136M keystrokes study)
- Frecency-weighted per-word correction ranking (Mozilla-style exponential decay)
- Character trigram model for typo vs intentional-word discrimination (~4KB)
- DAFSA compression for ~3x smaller dictionaries than plain trie
- Aho-Corasick multi-pattern matching via double-array trie (daachorse crate)
- TinyML character-level prediction on MCU (MCUNet, speculative)

______________________________________________________________________

## Phase 15 — QMK Config Import and Via/Vial

Make tecla a drop-in replacement for QMK from the user's perspective.
Parser and protocol work is pure Rust — no hardware deps.

- [x] `tecla-config` crate — parse QMK `info.json` (matrix, pins, USB VID/PID)
- [x] `keymap.c` transpiler — parse QMK keycodes, emit `keymap!`/`layer!` Rust source
- [x] Compile-time validation — dimension checks, unknown keycodes, layer consistency
- [x] Via / Vial protocol — `ViaProtocol` in core, dynamic keymap get/set over USB HID

______________________________________________________________________

## Phase 16 — RGB / LED / Audio / Haptic

Display and feedback subsystems. Animation engine and reactive event system
are pure core logic; hardware drivers are board-specific.

- [x] LED/RGB driver trait — `LedDriver` async trait abstracting WS2812/IS31FL3733
- [x] RGB task — `rgb_task` async Embassy task ticking animation engine + LED driver
- [x] Reactive animations — respond to keypress events from the pipeline
- [x] Audio — `audio.rs` with `Tone`, `Melody`, `AudioCommand`, `BuzzerDriver` trait,
  `AudioController`, `audio_task`, built-in melodies (startup, layer-change, error)
- [x] Haptic feedback — `haptic.rs` with `HapticEffect`, `HapticCommand`, `HapticDriver`
  trait, `HapticController`, `haptic_task`

______________________________________________________________________

## Phase 17 — Wireless (BLE)

nRF52840 via SoftDevice or nrf-softdevice crate. This is ZMK territory —
tecla's goal is to match ZMK's BLE quality with a superior core.
Blocked on nRF52840 hardware.

- [x] BLE HID profile — `BleReporter` trait extending `AsyncReporter` with connect/disconnect/advertise
- [x] Pairing and bonding — `BondInfo`, `BleHostManager<MAX_HOSTS>`, `StorageKey::BleHost0..3`
- [x] Multi-host switching — `Action::BleHostSwitch(u8)`, `Action::BleAdvertise`, `Action::BleDisconnect`
- [x] Power management — `PowerManager` trait with `enter_deep_sleep()`, `wake_reason()`, `Action::Sleep`
- [x] Battery service — `BatteryLevel` with percent/millivolts, `is_low()`, `is_critical()`
- [x] Simultaneous USB + BLE — `DualReporter`, `OutputMode`, `Action::OutputSelect`,
  `OUT_USB` / `OUT_BLE` keymap tokens; user selects transport at runtime

______________________________________________________________________

## Phase 18 — Additional MCU Targets

Blocked on hardware.

- [x] STM32F4 support — `tecla-stm32f4` board crate, Planck example, builds for thumbv7em-none-eabihf
- [ ] CH32V3 support (`ch32-hal`, ultra-low-cost RISC-V)
- [x] ESP32-S3 support — `tecla-esp32s3` board crate written, excluded pending upstream esp-hal bug
- [ ] Shared HAL abstraction layer

______________________________________________________________________

## Phase 19 — Flash CLI and Artifacts

Hardware-dependent tooling from original Phase 16.

- [ ] `tecla flash --keyboard <qmk-id>` CLI
  - [ ] Fetches `info.json` from QMK firmware repo
  - [ ] Compiles and flashes in one step via `probe-rs` or UF2
- [ ] UF2 / DFU / `adafruit-nrfutil` artifacts per target

______________________________________________________________________

## Stretch — AVR and Zephyr

**AVR (ATmega32u4):** Pro Micro, Elite-C, Teensy 2.0.

- [ ] `avr` feature flag — gates Embassy, swaps in sync polling loop
- [ ] Code-size audit — monomorphization budget within 32 KB flash
- [ ] AVR HEX artifact via `avrdude`

**Zephyr RTOS target:**

- [ ] `zephyr-hal` backend as an alternative to Embassy for shops already
  on Zephyr
