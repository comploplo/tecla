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

______________________________________________________________________

## Phase 20 — Device Module Refactor

Reorganize the flat `src/*.rs` layout into a device-oriented module tree.
Each input/output modality gets its own directory bundling action types,
report types, processing logic, and telemetry. Shared infrastructure
moves to `core/`. Users building novel controllers can study any device
module as a self-contained template.

- [x] Create `pipeline/` — event, recognizer, modifier, keymap, hid, report, power
- [x] Create `matrix/` — scanner traits, debounce, split
- [x] Create `devices/keyboard/` — tap-hold, combo, tap-dance, leader
- [x] Create `devices/steno/` — recognizer, engine, protocol, chord types
- [x] Create `devices/midi/` — MidiEvent helpers
- [x] Create `devices/mouse/` — pointing scanner trait
- [x] Create `devices/encoder/` — EncoderMapping, encoder_key_pos
- [x] Create `transport/` — SplitTransport, UART, BLE types
- [x] Create `storage/` — StorageKey, EepromShim, VIA
- [x] Create `keyboard/` module — KeyboardProcessor, steno_playback, tests
- [x] Extract steno playback engine with KeyOutput trait for independent testability
- [x] Update all imports across workspace (no re-exports, direct paths only)
- [x] Update board crate imports (tecla-rp2040, tecla-nrf52840, etc.)
- [x] Preserve all 660+ tests and all integration test suites
- [ ] Create `devices/gamepad/` — extract GamepadAction dispatch from
  keyboard processor, GamepadReport, gamepad telemetry

Goal: a user building a custom MIDI foot controller reads `devices/midi/`
and sees the complete MIDI path in one place. They create
`devices/pressure_pad/` following the same pattern.

______________________________________________________________________

## Research Program

**Thesis:** Tecla is a research platform for adaptive physical text entry
and accessible expert input.

The architecture is built around a unified `Recognizer` model, a
pure/testable `KeyboardProcessor`, and an extensible `Action<Ext>` action
space. Research directions are natural extensions of the existing core,
not side quests.

### Cross-cutting method: Bayesian parameter tuning

Multi-objective Bayesian optimization (MOBO) is the methodological
backbone for on-device adaptation across all three labs. Kristensson's
group has published the full progression:

- AdaptiKeyboard (ISMAR 2022) — per-user MOBO for speed/accuracy tradeoff
- Cooperative MOBO (TiiS 2024) — firmware proposes, user's typing confirms
- Group-level MOBO (IJHCI 2024) — find good defaults across populations
- CaBoP (CHI 2026) — cost-aware BO that prioritizes low-disruption tweaks
- Bayesian Methods for Interaction and Design (CUP 2022) — the textbook

On-device, this means: start from group-optimal defaults, silently tune
debounce/chord timing/actuation per user using a lightweight Gaussian
process or bandit, weighting parameter changes by disruption cost.

______________________________________________________________________

### Lab A — Adaptive Accessibility

The strongest single research pillar. High-impact, fits ASSETS-style
work, connects directly to things Tecla already has: debounce, per-key
thresholds, telemetry, haptics, layer logic, and personalized adaptation.

**ABD-MT adaptive typing** ⭐ core research

Unified typing adaptation using the ABD-MT pattern (Wobbrock MobileHCI
2024): observe → model → adapt. Not a clinical tool — the system measures
keystroke timing statistics, models consistency relative to the user's
own baseline, and adjusts firmware parameters accordingly. No diagnosis,
no ability labels, no medical claims. Benefits all users: fast typists
get tighter recognition, inconsistent typists get wider tolerance, steno
students get faster chord detection as their technique improves.

Silent by default — no setup wizard, no accessibility menu, no mode
switch (Findlater & Wobbrock CHI 2012: silent adaptation outperforms
explicit adaptation interfaces).

User-facing rules for making adaptation reversible, inspectable, and
non-hostile live in [`docs/adaptive-ux.md`](docs/adaptive-ux.md). That
document is the product contract for this research section.

Design principle: each telemetry struct collects one domain's raw
events. `ProcessorTelemetry` is the only place that synthesizes across
domains to produce adaptation recommendations. No struct computes
rates or trends from another struct's data. No struct duplicates
another's observations.

*Step 1 — Telemetry structural cleanup:*

- [ ] Move session trend (first-half vs second-half EMA drift) into
  `CoreTelemetry` — this is a core typing signal, not accessibility-
  specific. Every user's timing drifts over a session
- [ ] Slim `AccessibilityTelemetry` to filter-rejection deltas only:
  `delta_slow_accepts`, `delta_slow_rejects`, `delta_bounce_rejects`,
  `delta_sticky_uses` (plus cumulative totals). Drop `hold_time_samples`
  deque — that signal already lives in `CoreTelemetry::hold_time_ema/var`
- [ ] Fix `AccessibilityTelemetry::reset_deltas` (currently a no-op) to
  zero the delta fields so the adaptation tick sees recent behavior,
  not lifetime totals
- [x] Remove doc reference to "medication cycle detection" from
  `accessibility.rs` module doc — not our claim to make

*Step 2 — Telemetry observation extensions:*

- [ ] `CoreTelemetry`: co-press pair table — sparse `PairStat` array
  (64 or 128 slots, same pattern as existing `BigramStat`). Each entry:
  `from: u8, to: u8` (ordered), `spread_ema: u16`, `spread_var: u16`,
  `count: u16`. Records the time gap between press events for any two
  keys held simultaneously. This is the shared primitive observation
  that both combo and steno adaptation read from
- [ ] `CoreTelemetry`: rollover envelope (`u16`, max overlap duration),
  transposition rate (`u16`, consecutive presses < 30ms)
- [ ] `StenoTelemetry`: inter-chord gap EMA (time from all-release to
  next first-press — the steno-specific rhythm signal)

*Step 3 — Recognizer runtime setters:*

- [ ] `KeyboardProcessor::set_tapping_term(&mut self, ms: u64)`
- [ ] `ComboRecognizer::set_combo_term(&mut self, ms: u64)`
- [ ] `TapDanceRecognizer::set_inter_tap(&mut self, ms: u64)`
- [ ] `LeaderRecognizer::set_timeout(&mut self, ms: u64)`
- [ ] `StenoRecognizer` — add optional `settle_ms: Option<u64>` for
  rolled chord detection (currently pure structural all-release)
- [ ] `AutocorrectEngine` — extract hard-coded 30ms transposition
  threshold to configurable field with setter

*Step 4 — Adaptation methods on `ProcessorTelemetry` (the Adapter):*

All cross-cutting synthesis lives here. Individual telemetry structs
expose raw data; `ProcessorTelemetry` interprets it. Shared primitive
observation (pair table), divergent interpretation per recognizer:
combo asks "was this simultaneous or just fast sequential typing?"
steno asks "is the user done adding keys or is another finger coming?"

- [ ] `recommended_combo_term(&self, keys: &[u8]) -> u64` — worst-case
  pair spread for the given key set + margin. Rollover envelope as
  ceiling. Floored at authored combo_term_ms
- [ ] `recommended_tapping_term(&self) -> u64` — from core hold-time
  consistency distribution
- [ ] `recommended_debounce(&self, key: usize) -> u16` — core per-key
  hold-time variance + accessibility bounce rejection delta rate.
  High variance + high bounce rejections = debounce too tight for
  this key. Session trend shifts all keys together
- [ ] `recommended_settle_time(&self) -> u64` — core pair table read
  through steno lens + steno inter-chord gap rhythm to detect
  "done adding keys"
- [ ] `recommended_autocorrect_window(&self) -> u64` — wider for
  high-rollover typists, tighter for hunt-and-peck
- [ ] `typing_consistency(&self) -> u8` — 0-255 composite from core
  hold variance + accessibility rejection rates, for external
  consumers (host TUI, telemetry frames)
- [ ] `AutocorrectEngine`: dynamic transposition vs proximity weighting —
  high rollover → bias toward transposition corrections, high hold
  variance → bias toward proximity corrections

*Step 5 — Combo adaptation via pair table:*

- [ ] `ComboRecognizer`: no new fields. `adapted_combo_term()` takes a
  `&[PairStat]` slice and queries worst-case spread among its key
  pairs, floored at authored `combo_term_ms`, ceilinged at global
  rollover envelope. `tick()` uses `adapted_combo_term()` instead of
  fixed value
- [ ] `StenoRecognizer`: reads pair table through different lens —
  press-spread of chord's key pairs tells it how long to wait for
  the user's roll to complete. Steno rolls are intentional technique,
  not imprecision

*Step 6 — Processor integration:*

- [ ] `KeyboardProcessor::adapt_from_telemetry(&mut self)` — reads
  `self.telemetry`, calls `recommended_*` methods, writes values into
  recognizer configs and engine thresholds. No-op when telemetry is
  `None`. Called from existing 250ms telemetry tick
- [ ] Bidirectional: tightens parameters when consistency improves,
  loosens when it degrades. Adaptation is relative to the user's own
  converged baseline, not population norms

*Step 7 — Autocorrect adaptation:*

- [ ] `AutocorrectEngine::min_correction_confidence` — users with high
  consistency get stricter matching (fewer false corrections), users
  with low consistency get looser matching (catch more real errors)
- [ ] Dynamic proximity weighting from typing profile

*Step 8 — Persistence:*

- [ ] `StorageKey::TypingBaseline` — converged EMA statistics so cold
  boot doesn't start from zero
- [ ] `StorageKey::AdaptiveConfig` — enabled flag, user overrides
  (e.g., max combo window cap)
- [ ] Save periodically (every 5 min or on significant drift), restore
  in `restore_state()`

*Step 9 — Tests:*

- [ ] Convergence: system stabilizes after N presses
- [ ] Bidirectionality: parameters tighten when consistency improves
- [ ] Boundary: recommendations stay in sane ranges
- [ ] Per-combo: adapted window reflects pair table data
- [ ] Debounce cross-cutting: core variance + accessibility rejections
  both influence per-key recommendation
- [ ] Session trend: all timing recommendations drift with session trend
- [ ] Cold boot: persisted baseline restores correctly
- [ ] Zero-cost: telemetry disabled → no adaptation overhead

Graceful degradation ladder (progressive, each step only if previous
isn't enough): widen per-key debounce → lengthen tap-hold timing →
widen combo windows → auto-enable bounce keys. Never reduces capability
— only loosens timing constraints when consistency drops, tightens
them when it improves.

Enables later (not in scope): per-key debounce in board crate hardware
debouncers, host TUI showing live adaptation state, analog input
integration (rapid trigger thresholds from same profile).

- Literature: Wobbrock CREATE center, ABD-MT (MobileHCI 2024),
  Findlater (CHI 2012), Aalto CHI 2018 (136M keystrokes, rollover
  ratio as #1 speed differentiator), EdgeWrite (UIST 2003)

**Per-key analog input** ⭐ core research

- [ ] `AnalogMatrixScanner` returning `u8` (0-255 travel) instead of `bool`
- [ ] EMA low-pass filter with bit-shift alpha (integer-only, ~5 lines)
- [ ] Auto-calibration: EMA-smooth idle at startup, continuously recalibrate
  rest/bottom-out values with epsilon guard (libhmk pattern)
- [ ] Three-state rapid trigger machine (INACTIVE/DOWN/UP with extremum
  tracking, asymmetric up/down sensitivity) — ~40 lines of Rust
- [ ] Dynamic Keystroke (DKS): 4 keycodes per key triggered at press,
  bottom-out, release-from-bottom, release (as `Action<Ext>` variant)
- [ ] Adjustable actuation points per key, driven by adaptive typing profile
- [ ] Velocity-sensitive typing — key travel speed as input dimension
- Reference implementations: libhmk (C, most complete), Macrolev (ESP32),
  Minipad (RP2040). No production Rust implementation exists — tecla first

**Haptic language** ⭐ core research

- [ ] Structured vibration vocabulary using Tactons parameter space:
  rhythm (most reliable), roughness, amplitude, duration
  (Brewster & Brown, Glasgow — ~50-100 distinguishable patterns)
- [ ] Accessibility feedback: silent mode signaling, BLV users described
  haptic-only text entry as "liberating" (TypeHap, UIST 2025)
- [ ] Passive haptic learning of chords: vibrate correct finger pattern
  during idle time, users learn without active attention
  (Seim & Starner, Georgia Tech — validated for Braille/piano/typing)
- [ ] Expert-mode signaling: haptic confirmation of complex inputs
- [ ] Haptic keyclick: 23% speed improvement on flat keyboards, audio adds
  nothing on top (Ma et al., IEEE World Haptics 2015)
- Tecla has: `haptic_task`, `HapticDriver` trait, `HapticCommand` action

**Accessibility modes** ✅ shipped

- [x] Sticky keys, slow keys, bounce keys
- [x] Runtime toggle actions + `AccessibilityConfig` builder
- [ ] Visual/audio feedback on accessibility state changes

**Steno** ✅ research success

- [x] `StenoRecognizer` — chord accumulation until all-release
- [x] `StenoEngine` — on-device DAFSA dictionary translation (Lapwing)
- [x] GeminiPR / TXBOLT wire protocols for host-side Plover
- [x] `steno!` macro for compile-time steno keymap declaration
- [x] `CapTouchScanner` — capacitive touch sensing for RP2040
- [x] Boards: Steno Uni V4 (matrix), Asterisk v1.4 (cap touch)
- Validated: Recognizer trait handles expert chording input, on-device
  dictionary translation works within MCU constraints, cap touch is a
  viable scanner modality alongside mechanical switches
- First proof that Tecla's architecture supports non-keyboard input
  paradigms as first-class citizens

**DAFSA steno dictionary compression** ✅ research success — potentially publishable

- [x] Full Lapwing dictionary (159,805 entries, 5.5 MB JSON) compressed
  to 1.6 MB firmware binary — fits on a stock 2 MB RP2040 with no
  external flash
- [x] Techniques: DAFSA suffix sharing, u24 stroke/offset encoding
  (3 bytes instead of 4), compact variable-length text encoding,
  binary search over sorted children
- [x] First known keyboard firmware to fit a full production steno
  dictionary on a standard 2 MB RP2040 Pico
- Prior art: Javelin uses a compact hash map requiring 16 MB flash;
  Kolibri uses a prefix trie requiring external flash. Neither fits
  on stock hardware
- Significance: removes the cost barrier to embedded steno — a $4 Pico
  replaces $25+ boards with 16 MB flash, directly expanding access to
  stenography as an assistive input method
- Potential venue: ASSETS or CHI — "Efficient On-Device Steno Dictionary
  Compression for Resource-Constrained Keyboards"

**Braille input/output** — product feature

- [ ] Braille chording — 6/8-key simultaneous input mapped to braille cells
- [ ] HID braille display output — USB braille display profile for
  screen-reader integration
- Strengthens the accessibility credibility story

______________________________________________________________________

### Lab B — Expert Expressive Input

Where expert users push the input model to its limits. Tecla's steno
engine and `Recognizer` trait make chord-like input first-class.

**Chording efficiency theory** ⭐ core research (most publication-friendly)

- [ ] Optimal chord-to-word assignment as Quadratic Assignment Problem
  (QAP) — tractable for 12-24 key chording keyboards with GA or IP
- [ ] Fitness function combining: word frequency (Zipf), chord ergonomic
  difficulty (Fitts' Law inter-key distance), collision avoidance,
  mnemonic similarity to word
- [ ] Zipf's law insight: ~100 chords cover ~50% of all English text.
  Progressive introduction starting from frequency rank 1 is optimal
- [ ] Learning-curve modeling: chord keyboards have 77.3% learning
  percentage — steep start, fast improvement (Anderson et al. 2009)
- [ ] Firmware-driven spaced repetition scheduling (Leitner box or
  half-life regression) for chord introduction — track per-chord
  recall accuracy, introduce new chords only when existing ones
  are consolidated. Combined with passive haptic learning
- [ ] Phoneme-level chord mapping (MCS, Beddoes & Hu 1994) more
  learnable than letter-level — worth exploring as alternative theory
- [ ] Offline optimizer tool: takes user corpus + ergonomic model,
  produces personalized chord dictionary
- [ ] RoboType-style simulator (Kristensson 2023) to test adaptive
  chord algorithms offline with synthetic users
- Literature: MCS (IEEE 1994), ZipChord (Zipf), Anderson (2009),
  KALQ (Oulasvirta CHI 2013), 25-Hour Keyboard (Somani 2025)

**Chorded text entry (non-steno)**

- [ ] CharaChorder-style simultaneous key-to-word mapping for
  non-stenographers, leveraging the existing `Recognizer` trait
- [ ] ZipChord-style hybrid: normal QWERTY + chords for top 100 words
- Natural deployment target for chording efficiency research

**Emacs-style layers** — product feature

- [ ] Prefix-key driven modal layer activation (C-x map, M-x map style)
- [ ] Composable layer stacks with auto-deactivation
- [ ] Which-key integration — display available bindings after prefix timeout
- Excellent design, strong user differentiation, more product than science

**Conditional layer predicates** — product feature

- [ ] "If layer A active AND modifier X held, activate layer B"
- [ ] Generalized tri-layer beyond QMK's simple two-source model
- Should probably just ship this

**On-device macro scripting**

- [ ] Conditionals in macros — if-held, if-layer, wait-for-key
- [ ] Tiny bytecode interpreter for firmware-side macro logic

**MIDI controllers** — demonstration vertical

- [ ] Dedicated control surface platform — grid pads, faders (via ADC),
  encoders-to-CC, velocity-sensitive keys
- [ ] MIDI sysex for firmware-side patch selection and routing
- Great showcase for `Action<Ext>` extensibility, not a core research thrust

**Gesture input**

- [ ] Swipe-to-layer, pinch-to-zoom via trackpad/touchpad integration
- [ ] Build on existing `pointing_task` infrastructure

______________________________________________________________________

### Lab C — Predictive Assistive Typing

Where on-device inference meets text entry. Telemetry is the research
substrate — without it, adaptive ideas stay vibes.

**Live telemetry** ⭐ platform enabler (prerequisite for most research)

Architecture: `TelemetryExt` trait — generic extension point for
modality-specific telemetry, same pattern as `Action<Ext>`. Users
building novel controllers implement the trait and get the full
collection/snapshot/transport pipeline for free.

```
TelemetryState<KEYS, Ext: TelemetryExt = ()>
├── CoreTelemetry<KEYS>     (always present)
└── Ext                     (modality-specific, user-extensible)
```

`TelemetryExt` trait: `observe(&mut self, event)`, `snapshot(&self)`,
`reset_deltas(&mut self)`. `()` impl is zero-cost no-op.

Core telemetry (canonical feature set from CMU/Aalto/BiDAlab):

- [ ] Per-key press count, hold time EMA + variance, error rate
- [ ] Four inter-key latencies: PP, PR, RR, RP per keystroke pair
- [ ] Per-bigram IKI for top 64 bigrams (mean+variance)
- [ ] Rollover ratio (% of presses where next key down before prev up)
- [ ] Sliding-window WPM from 50-entry timestamp ring buffer
- [ ] Collection points: `handle_press`, `handle_release`, backspace detection

Modality-specific telemetry (feature-gated, each ~50-200 bytes):

- [ ] **Steno** — per-chord dictionary hit/miss/retry rate, multi-stroke
  completion rate, strokes-per-minute, dictionary coverage ratio,
  undo frequency, chord hold time distribution (rolled vs clean)
- [ ] **MIDI** — per-note velocity distribution (mean/min/max), note
  duration histogram, CC value range utilization, polyphony depth,
  timing jitter between notes in intended chords
- [ ] **Gamepad** — per-axis utilization range, dead zone noise floor,
  button hold time distribution, simultaneous button combos
- [ ] **Pointing** — movement velocity distribution, click-while-moving
  rate, scroll frequency, hover duration before click
- [ ] **Accessibility** — slow key rejection rate, bounce key rejection
  rate, sticky key usage count, hold time trend over session (fatigue),
  error rate trend over session (comfort and consistency drift)
- [ ] **Split** — per-half press balance, cross-hand vs same-hand bigram
  timing, transport round-trip latency measurement
- [ ] **Cap touch** — per-pad discharge count distribution, threshold
  margin, ambient baseline drift over time
- [ ] **Encoder** — rotation velocity, direction reversal frequency,
  per-encoder usage imbalance

Transport:

- [x] Snapshot frame: tagged union with modality flags header, host
  dispatches to correct decoder based on flags present
- [x] `TelemetryFrame` serialize/deserialize with round-trip tests
- [x] `collect_frame` gathers all modality snapshots and resets deltas
- [x] Pointing telemetry wired in processor (key-triggered mouse) and
  `pointing_task_with_telemetry` (async device scanner)
- [x] Encoder telemetry wired in `encoder_task_with_telemetry` (async)
- [x] `keyboard_task_telemetry` — 250ms periodic frame collection in keyboard loop
- [x] `telemetry_task` — receives frames, serializes, writes to transport
- [x] `AsyncTelemetryWriter` trait for board-crate transport implementations
- [x] `CdcTelemetryWriter` (RP2040) — CDC ACM virtual serial for telemetry output
- [x] `TelemetryChannel` type for frame routing between tasks
- [ ] CDC ACM USB endpoint in remaining board crates (nRF52840, STM32, ESP32)
- [ ] Host companion: Ratatui TUI (phase 1), web dashboard (phase 2)
- [ ] Expose ability profile to host for software-side adaptation

Implementation order:

1. ~~`TelemetryExt` trait + `CoreTelemetry` struct + collection points~~ ✅
2. ~~Tests — counts, EMA convergence, WPM, rollover detection~~ ✅
3. ~~`StenoTelemetry` impl (first modality extension)~~ ✅
4. ~~All modality impls (MIDI, gamepad, accessibility, pointing, encoder)~~ ✅
5. ~~Snapshot frame binary format + `collect_frame`~~ ✅
6. ~~`telemetry_task` + CDC ACM USB endpoint (RP2040)~~ ✅
7. Host TUI + remaining board crate CDC endpoints

Total firmware RAM cost: ~2KB core + 50-200 bytes per modality.
Without this, most of Lab A and Lab C are unmeasurable.

**Rich runtime keymap editing** — platform enabler

- [ ] USB protocol exposing full `Action<Ext>` type system
- [ ] Companion app beyond Via/Vial's keycode subset
- [ ] Enables running interventions fast instead of reflashing firmware
- Tecla has: wear-leveled persistent state, runtime `set_action()`

**Modern autocomplete theory**

- [ ] Character trigram trie (~50KB) for next-key prediction — fits in
  RP2040 RAM alongside everything else, uses existing trie infrastructure
- [ ] Context-aware completion (preceding word influences suggestions)
- [ ] Frecency-weighted ranking (Mozilla-style exponential decay)
- [ ] Sentence-level decoding: resolve borderline chords using surrounding
  context, not just per-chord dictionary (VelociTap, Kristensson CHI 2015)
- [ ] Gboard-style spatial personalization: learn per-key Gaussian
  distributions of actuation timing, update online (Sivek MobileHCI 2022)
- [ ] Host-side hooks for LLM expansion (SpeakFaster pattern: abbreviation
  → LLM expansion for progressive motor conditions)
- Neural approaches (LSTM, Transformer) are infeasible on M0+ (264KB RAM).
  Character n-gram models are the practical path. MLLM-2 compact model
  is 74KB — fits with room to spare
  (text-entry studies 1990–2024)

**Predictive key pre-activation** — moonshot 🌙

- [ ] Reduce debounce on predicted next key using language model + finger
  trajectory data, measurably faster typing for fluent typists
- [ ] Requires: solid telemetry, analog input, decent per-user error model
- [ ] Danger: false positives are catastrophic — needs high-confidence
  prediction before lowering thresholds
- Literature: Zhai, Kristensson, ShapeWriter lineage (CHI/UIST)

______________________________________________________________________

### Parked

Interesting directions kept on file but not prioritized.

**Typing biometrics** — lab branch, not flagship

- Keystroke dynamics authentication using inter-key latency + hold duration
- Literature: Monrose & Rubin 1997 → modern deep learning approaches
- Concerns: behavior drift is a known unsolved problem, privacy is
  immediately thorny, continuous auth changes the trust model
- Framing: opt-in, lab branch, not on by default, not the first story

**Split latency prediction**

- Send predicted matrix state over UART/BLE, correct on mismatch
- Technically clever, niche audience, save for a systems-heavy side paper

**RSI/ergonomics monitoring**

- Typing load tracking, rest reminders, force curve analysis
- Frame as load/fatigue estimation, not "medical monitoring"
- Needs extremely strong validation to avoid overclaiming

______________________________________________________________________

### Academic sources to watch

- **ASSETS** — accessibility, motor impairment, adaptive interfaces
- **CHI** — human-computer interaction, predictive text, input techniques
- **UIST** — novel input techniques, gesture recognition
- **Open Steno Project** — Plover Discord, pushing firmware limits
- **Applied Ergonomics / Human Factors** — RSI prevention, key force curves

### Key labs and collaborators

- **Wobbrock / CREATE Center (UW)** — ability-based design, ABD-MT toolkit,
  motor impairment text entry. Natural academic partner
- **Kristensson (Cambridge)** — Bayesian optimization for interactive devices,
  text entry for non-speaking individuals, function structure models
- **Zhai (Google Research)** — gesture keyboards, haptics, optimal control
  modeling of typing movements
- **Brewster (Glasgow)** — Tactons, vibrotactile communication parameter space
- **BiDAlab (Madrid)** — TypeNet/TypeFormer keystroke dynamics datasets (136M+
  keystrokes), useful for adaptive behavior research
- **Seim / Starner (Georgia Tech)** — passive haptic learning of chords
- **Oulasvirta (Aalto)** — computational keyboard optimization (KALQ),
  136M keystrokes study, CaBoP cost-aware BO

### Key reference implementations

- **libhmk** (C) — most complete open-source Hall effect keyboard library
- **Macrolev** (ESP32) — analog pipeline with EMA + rapid trigger
- **ZipChord** — hybrid chord input using Zipf's law
- **Plover** — dominant open-source stenography engine
- **self-adapting-keyboard** (Android) — on-device adaptive key behavior
- **ABD-MT** (UW) — Observer → Modeler → Adapter toolkit for ability-based design

### Priority order for next 5 research pushes

1. ABD-MT adaptive typing (unified observe → model → adapt for all recognizers)
2. Per-key analog input
3. Live telemetry host TUI (platform visibility for everything else)
4. Chording efficiency theory
5. Haptic language
