# Tecla Roadmap

A QMK-inspired keyboard firmware core built in embedded Rust.
Targets `no_std` by default, async-first via Embassy, MCU-agnostic via `embedded-hal`.

**Primary targets:** RP2040, STM32, nRF52840 — modern MCUs with ample flash and RAM.
**AVR (ATmega32u4):** stretch goal, feature-flagged, tackled after all other phases complete.
Goal: eventual drop-in replacement for QMK on all supported boards.

---

## Phase 1 — Core Types and Abstractions ✅
The fundamental vocabulary the rest of the codebase speaks.

- [x] `event.rs` — `KeyPos`, `KeyEvent` (press/release with timestamp)
- [x] `action.rs` — `KeyCode` (HID-aligned enum), `ModifierKey` (bitmask),
      `ConsumerCode`, `LayerOp`, `Action<Ext>` (generic extension point)
- [x] `modifier.rs` — `ModifierState` (real / weak / oneshot stacks)
- [x] `recognizer.rs` — `Recognizer` trait + `Recognition<T>` enum —
      the **unifying primitive** for tap-hold, leader, combo, tap-dance, steno
- [x] `report.rs` — `KeyboardReport` (6KRO), `ConsumerReport`, `HidReport`
- [x] `hid.rs` — `Reporter` trait (output boundary)

---

## Phase 2 — Matrix Scanning ✅
Read the physical switch matrix and produce raw state.

- [x] `matrix.rs` — `MatrixState<ROWS, COLS>` with safe accessors,
      `MatrixScanner` trait
- [ ] Row-strobe / col-read GPIO implementation (board crate)
- [ ] Direct-pin implementation (board crate)

---

## Phase 3 — Debounce ✅
Filter electrical noise from switch contacts.

- [x] `debounce.rs` — `Debouncer<ROWS, COLS>` trait
- [x] `SymDefer` — symmetric global-timer defer
- [x] `AsymEagerDefer` — eager press, deferred release (per-key)

---

## Phase 4 — Layer System ✅
Resolve keycodes from the active layer stack.

- [x] `keymap.rs` — `Layer<Ext>`, `LayerStack<LAYERS, KEYS, Ext>`
- [x] Highest-active-layer wins, transparent fallthrough, `NoOp` blocks
- [x] Press-time layer cache (prevents stuck keys on layer change mid-hold)
- [x] `LayerOp` dispatch (momentary, toggle, set-default, on, off)

---

## Phase 5 — Tap-Hold ✅
Disambiguate tap vs. hold for `ModTap` and `LayerTap` keys.

- [x] `tapping.rs` — `TapHoldRecognizer<BUF>` implements `Recognizer`
- [x] Default mode (timeout only)
- [x] `HoldOnOtherKeyPress` mode
- [x] `PermissiveHold` mode
- [x] Configurable `tapping_term_ms` per recognizer
- [x] Waiting buffer with configurable capacity (default 32)
- [x] `drain_pending` for event replay after resolution
- [ ] One-shot modifiers and one-shot layers (next)

---

## Phase 6 — Extended Recognizers ✅
Remaining `Recognizer` implementations sharing the same trait.

- [x] `leader.rs` — `LeaderRecognizer`: Emacs-style nested trie sequences,
      per-branch timeouts, which-key metadata for display integration
- [x] `combo.rs` — `ComboRecognizer`: simultaneous key-set within a window
- [x] `tap_dance.rs` — `TapDanceRecognizer`: counted rapid taps → action per count
- [x] `steno.rs` — `StenoRecognizer`: chord until all-release →
      GeminiPR / TXBOLT serialisation (dictionary stays on host in Plover)

---

## Phase 7 — Keyboard Orchestrator ✅
Wire the full pipeline into an Embassy async entry point.

- [x] `keyboard.rs` — `KeyboardProcessor`: synchronous event → report core,
      MCU-agnostic, no Embassy dependency
- [x] `task.rs` — `tecla::task::run()`: single async Embassy task wrapping the
      full scan → debounce → process → report pipeline; tap-hold deadline raced
      via `embassy_time::with_timeout`
- [x] `AsyncMatrixScanner` trait (`embassy` feature) — async scan boundary
- [x] `AsyncReporter` trait (`embassy` feature) — async USB send boundary
- [x] NKRO report (bitmap) as alternative to 6KRO — `NkroKeyboardReport`, `RolloverMode`
- [x] Report coalescing within USB frame — `usb_task` skips identical consecutive reports
- [x] `matrix_task` / `keyboard_task` / `usb_task` — three independent Embassy tasks
      wired by `EventChannel` / `ReportChannel`; tap-hold deadline raced via
      `embassy_time::with_timeout` in `keyboard_task`

---

## Phase 8 — Extended Output (MIDI, Gamepad)
The `Action<Ext>` generic makes this architectural, not bolted-on.

- [ ] `MidiReporter` — USB MIDI class device, tight jitter via Embassy task priority
- [ ] `GamepadReporter` — USB HID gamepad descriptor
- [ ] `CompositeReporter` — keyboard + MIDI + consumer simultaneously
- [ ] Encoder events fed into the same pipeline as key events

---

## Phase 9 — Board Support Layer
Thin crates wiring the core to real hardware.

- [ ] RP2040 board crate (Raspberry Pi Pico / Pico W)
- [ ] STM32F4 board crate (WeAct / BlackPill)
- [ ] nRF52840 board crate (nice!nano — BLE via SoftDevice)
- [ ] `probe-rs` flashing and RTT logging for all ARM targets
- [ ] Split keyboard support (secondary half streams matrix events over UART)

---

## Phase 10 — QMK Config Parsing
Read existing QMK keyboards without rewriting configs.

- [ ] `tecla-config` — parse `info.json` (matrix layout, pins, USB VID/PID)
- [ ] Parse `keymap.c` via `build.rs` → emit typed Rust `const` keymap
- [ ] Validate parsed config at compile time (pin counts, layer bounds)
- [ ] `tecla-macros` — `keymap!`, `layer!`, `MT!`, `LT!`, `#[keyboard_main]`

---

## Phase 11 — Flash QMK Keyboards
Drop-in replacement for QMK on supported modern boards.

- [ ] UF2 / BIN artifacts compatible with existing QMK bootloaders
  - [ ] RP2040 UF2 (drag-and-drop)
  - [ ] STM32 DFU (`dfu-util`)
  - [ ] nRF52840 (`adafruit-nrfutil` / `probe-rs`)
- [ ] QMK-compatible bootloader entry (hold key on plug-in)
- [ ] VIA / VIAL compatibility layer (dynamic keymap remapping over USB HID)
- [ ] `rmk flash --keyboard <qmk-keyboard-id>` CLI
  - [ ] Fetches `info.json` from QMK firmware repo
  - [ ] Compiles and flashes in one step

---

## Stretch Goal — AVR Support
Drop-in replacement on legacy ATmega32u4 boards (Pro Micro, Elite-C, Teensy 2.0).
Tackled only after Phases 1–11 are complete.

- [ ] `avr` feature flag — gates Embassy, swaps in sync polling loop
- [ ] Code size audit — monomorphization budget within 32KB flash
- [ ] AVR HEX artifact via `avrdude`
- [ ] CI target: `avr-unknown-gnu-atmega32u4`
