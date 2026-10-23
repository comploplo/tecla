# Tecla Roadmap

An embedded Rust keyboard firmware core — `no_std`, async-first via Embassy,
MCU-agnostic via `embedded-hal` and `embedded-hal-async`.

Also growing into an instrument firmware SDK: reusable input, musical behavior,
and timing components for building new physical MIDI instruments. Phases 21–26
cover this track; sound comes from external instruments or host software.

**Workspace targets:** RP2040, STM32, and nRF52840. The standalone ESP32-S3
crate still needs a firmware build and hardware validation before joining CI.

**Design goals:**

- Match and exceed QMK's keyboard input core with a type-safe, memory-safe,
  fully-tested Rust implementation
- One `Recognizer` trait for deadline-driven input disambiguation, so board
  crates and downstream consumers can add their own
- `Action<Ext>` carries custom action types; custom controllers must supply
  their dispatch because the default keyboard processor ignores `Ext` actions
- Embassy async pipeline eliminates busy-waiting; scan → process → report
  with zero CPU burn between events
- `unsafe_code = deny` by default, with explicit exceptions for board startup
  and interrupt glue
- Support instrument programs alongside the keyboard processor, sharing drivers
  and runtime services while keeping playing techniques and musical rules explicit

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

- [x] `Layer<Ext, CAPACITY>`, `LayerStack<LAYERS, KEYS, Ext>` with key-sized layers
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

- [ ] Leader sequences — Emacs-style nested tries, per-branch timeouts,
  which-key metadata for display integration; require a concrete runtime consumer
- [ ] Optional timed combos — eager/windowed simultaneous key-set; add only
  if a board needs semantics beyond the live chord model below
- [x] Live combo (chord) model in `KeyboardProcessor` — no timing window,
  press-to-fire: the action presses (held and repeating exactly like any
  other key, including host typematic repeat) the instant the held set
  matches a combo, releasing on the first member key-up. An intervening
  non-combo key press before any match aborts and replays the chord as
  normal keys; after a match it's ordinary rollover. Falls back to
  replaying buffered taps if the full set releases with no match
- [ ] Tap dance — counted rapid taps → action per count; require a concrete
  runtime consumer
- [x] Steno — chord until all-release, GeminiPR / TXBOLT wire format
  (dictionary stays on host in Plover), implemented by
  `devices::steno::StenoEngine`. `TapHoldRecognizer` is the `Recognizer` trait's
  production implementor.

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
- [x] USB HID via `embassy-usb` (`CompositeUsbReporter`, 6KRO)
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
- [x] Half-identification — `TileId` + `SplitConfig { is_host, col_offset }`, with `SplitConfig::remap`
- [x] USB arbitration — split.rs example shows primary-only USB enumeration
- [x] `UartTransport` — async UART impl over `embedded-io-async`, 11-byte framed KeyEvent
- [ ] Validated targets: Corne (3×6), Kyria (3×6+5t), Lily58 (4×6)

______________________________________________________________________

## Phase 13 — Extended Output

Built-in action and report variants carry these outputs through the keyboard
pipeline. Custom instrument controllers can also emit reports directly.

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
- [x] Via command decoder — protocol version, keyboard values, layer count,
  per-key get/set, keymap reset. Command IDs and byte layouts follow QMK's
  `quantum/via.h`, checked by `tests/via_protocol_ids.rs`.
- [x] Raw-HID transport — `VIA_REPORT_DESCRIPTOR` + `via_usb_task`/`service_via`
- [ ] Remaining Via commands: dynamic macros (0x0C–0x10), bulk keymap buffer
  (0x12/0x13), encoders (0x14/0x15), custom/lighting values, bootloader jump
- [ ] Vial protocol — not started

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

nRF52840 via `trouble-host` and Nordic SDC. Firmware builds in CI; radio,
pairing, and power behavior still need physical hardware validation.

- [x] BLE HID profile — `BleReporter` trait extending `AsyncReporter` with connect/disconnect/advertise
- [x] Pairing and bonding — `BondInfo`, `BleHostManager<MAX_HOSTS>`, `StorageKey::BleHost0..3`
- [x] Multi-host switching — `Action::BleHostSwitch(u8)`, `Action::BleAdvertise`, `Action::BleDisconnect`
- [x] Power management — `PowerManager` trait with `enter_deep_sleep()`, `wake_reason()`, `Action::Sleep`
- [x] Battery service — `BatteryLevel` with percent/millivolts, `is_low()`, `is_critical()`
- [x] Simultaneous USB + BLE — `DualReporter`, `OutputMode`, `Action::OutputSelect`,
  `OUT_USB` / `OUT_BLE` keymap tokens; user selects transport at runtime

______________________________________________________________________

## Phase 18 — Additional MCU Targets

Hardware validation varies by board; see the
[native board guide](keyboards/tecla/README.md#hardware-status).

- [x] STM32F4 support — `tecla-stm32f4` board crate, Planck example, builds for thumbv7em-none-eabihf
- [x] STM32F3 support — `tecla-stm32f3` board crate and ZSA Voyager profiles;
  USB and MIDI playing confirmed. The linker uses the STM32F303CC memory map,
  with compile-time processor/stack budget checks
- [x] STM32F0 support — `tecla-stm32f0` board crate and NuPhy NOS75 profile;
  initial USB enumeration confirmed, remaining hardware checks documented
- [ ] CH32V3 support (`ch32-hal`, ultra-low-cost RISC-V)
- [ ] ESP32-S3 validation — the standalone `tecla-esp32s3` crate targets
  current `esp-hal`/`esp-rtos` APIs and resolves its dependencies, but still
  needs an Xtensa firmware build and hardware validation; excluded from CI
- [ ] Shared HAL abstraction layer

______________________________________________________________________

## Phase 19 — Flash CLI and Artifacts

- [x] Shared `just flash --tecla-config <profile>` build-and-flash coordinator
- [x] Profile, USB device, and mounted UF2 target discovery through `--list`
- [x] UF2 conversion/copy, DFU `.bin` generation and flashing, and probe-rs
  dispatch for configured native profiles
- [x] DFU image-size validation, explicit device selection, and `--dry-run`
- [x] `--qmk-keyboard <qmk-id>` resolves vendored QMK definitions to an existing
  native profile when the family match is unambiguous
- [ ] Generate complete firmware for arbitrary imported QMK boards; current
  imports and native-profile matching do not provide universal board support
- [ ] `adafruit-nrfutil` artifacts for applicable targets

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

## Phase 20 — Device Module Structure

The device-oriented module tree groups input/output modalities with their
action types, report types, processing logic, and telemetry. Shared
infrastructure lives in `pipeline/`. Device modules work alongside shared action,
report, telemetry, and runtime modules; they do not contain the entire output path.

- [x] Create `pipeline/` — event, recognizer, modifier, keymap, hid, report, power
- [x] Create `matrix/` — scanner traits, debounce, split
- [x] Create `devices/keyboard/` — tap-hold, combo
- [x] Create `devices/steno/` — engine, protocol, chord types
- [x] Create `devices/midi/` — MidiEvent helpers
- [x] Create `devices/mouse/` — pointing scanner trait
- [x] Create `devices/encoder/` — EncoderMapping, encoder_key_pos
- [x] Create `transport/` — SplitTransport, UART, BLE types
- [x] Create `storage/` — StorageKey, EepromShim, VIA
- [x] Create `keyboard/` module — KeyboardProcessor, steno_playback, tests
- [x] Extract steno playback engine with KeyOutput trait for independent testability
- [x] Direct module imports across the workspace and board crates
- [x] Unit and integration test coverage for the shared device modules
- [ ] Create `devices/gamepad/` — extract GamepadAction dispatch from
  keyboard processor, GamepadReport, gamepad telemetry

Goal: a custom controller can reuse the scanner traits, device helpers, reports,
and transports, supplying its own behavior independently of keyboard processing.

______________________________________________________________________

## Instrument Firmware SDK — Phases 21–26

Build playable instruments on existing hardware, then share code where those
instruments demonstrate the same need. These milestones describe working programs,
not a catalogue of engines, a universal sensor model, or a new execution framework.

**Scope:** external sound sources over MIDI first. Internal synthesis, samplers,
audio recording/looping, and audio DSP are out of scope for this track. Existing
Phase 16 buzzer sounds remain interface feedback.

**Already available:** Phase 13's note/CC/pressure events and USB packet encoding,
keyboard-triggered note-off, existing pad/key/encoder/pointing drivers, the
synchronous processing pattern with injected time, Embassy timers/channels, and
host-test infrastructure. Reuse these; they are not new SDK deliverables. The
Asterisk USB-MIDI pad example plays on hardware; AutoChord uses the same board
and transport, with physical validation of its musical controls pending.
Storage helpers do not yet constitute wired instrument persistence.

Start with Phase 21. Phase 22 uses that board/output path; Phase 23 adds the first
timed program. Phase 24 combines those results; Phase 25 uses the timing needed
for one event looper. Phase 26 documents and validates each working example as it
lands. Optional follow-ons below do not block these completion gates or require
finishing the keyboard/research backlog.

## Phase 21 — USB-MIDI Pad Controller

One existing pad/key board plays fixed-velocity notes on an external synth.
Use the existing MIDI types and encoding; no new message family is needed.

- [x] Add a MIDI-only board example using the existing scanner and USB-MIDI class;
  register its profile/features and include the example in board builds
- [x] Correct delivery on the path it uses: preserve repeated MIDI events, make
  buffer exhaustion/send failures visible, and avoid marking failed output as sent
- [x] Validate its configured channel/notes and define cleanup of its held notes
  on stop and reconnect, without replaying stale attacks
- [x] Test message/release ordering, invalid configuration, repeated events,
  overflow, and disconnect/reconnect recovery

Implemented in [`asterisk_v1_4_midi`](keyboards/tecla/asterisk_v1_4_midi/): the
existing capacitive scanner and `KeyboardProcessor` play notes 48–72 at velocity
100 on channel 1; GPIO13 stops the notes. Checked scan batches go directly to the
existing USB-MIDI class using the shared packet encoding. A fixed-map note-off
sweep handles stop and USB recovery; pending writes cannot cross USB sessions.
Host tests exercise the same keymap, packing, and session guard as the firmware.

**Completion gate:** pads play and release an external sound source on one board;
host monitoring and deterministic delivery tests verify the actual output path.
**Hardware:** basic pad playing confirmed by the user. Full mapping, Stop, and
disconnect/reconnect checks remain to be confirmed using the profile's checklist.

______________________________________________________________________

## Phase 22 — Four-Voice AutoChord

An instrument program owns its musical state independently of keyboard actions.
The player chooses bass and melody; Tecla supplies connected tenor and alto
voices, automatic harmonic movement, and momentary controls for the phrase and
its harmonic vocabulary.

- [x] Add an allocation-free `music` module: diatonic keys and steps, pitch-class
  sets, triads/sevenths, chord roles, and bounded four-part voicing search
- [x] Preserve played anchors, voice identities, common tones, and exact MIDI
  ownership; keep generated voice pairs strict while identifying unavoidable
  outer-voice parallels and doubled leading tones forced by the player's notes
- [x] Give physical bass/melody presses ownership of their attacks; harmony
  commands articulate generated inner voices without interrupting held outer notes
- [x] Sound the lowest held left note and highest held right note literally;
  use remaining held keys as harmonic guidance for two generated parts, with
  smooth anchor handoffs and no direct extra-note attacks
- [x] Interpret melodic flow, repeated/spaced attacks, and saved arrivals before
  voicing selection; use recent onset timing only at player events, with no
  autonomous chord changes, tempo controls, or delayed note attacks
- [x] Support named fifth omissions while retaining diminished tritones; keep
  the last successful harmonic history through an unsuccessful search
- [x] Support independent tenor/alto catches in the program API, including chromatic
  notes and sevenths; keep saved targets and resolve each on release while the
  other catch continues
- [x] Map eight left and ten right letter pads to in-key notes; use A/O/E/U for
  Prepare/Intensify/Arrive/Substitute; keep both played pitches literal for every gesture
- [x] Make each # bar directly reinterpret its hand's played note, with momentary
  held preference and independent thumb meanings; both # pads Stop
- [x] Add a bounded chromatic chord palette, revoicings, named colour tones and
  fifth omissions, common substitutions, and checked local resolution destinations;
  use independent star permissions for borrowing and suspended/extended colours,
  including tritone substitutions, sus2/sus4, ninths, and thirteenths
- [x] Let an established dominant root become implied over a continuous prepared
  bass when its actual inner guide pair and destination remain intact; retain
  all other structural tones and clear the permission when its proof is lost
- [x] Follow ordinary keyboard motion through predominant, dominant, and tonic
  harmony; retain phrase direction, applied destinations, and each inner voice's
  leading-tone or seventh resolution through rolled changes and catches
- [x] Distinguish contextual six-four inversions and prepared melody ornaments;
  give cadential six-fours voice-specific 4–3/6–5 targets and bass doubling;
  connect seventh-chord and diminished-tritone guides without doubling generated
  tendency tones, including raised leading tones in applied major triads
- [x] Let prepared melody decorations keep a steady triad beneath them and
  survive reharm commands; admit prepared step-resolving melody tensions over
  a new harmony while preserving literal pitches and inner-voice obligations
- [x] Prepare held tonic/dominant pedals, follow short directional bass figures,
  and give inner voices event-driven passing/neighbour steps with saved endpoints
- [x] Separate functional pull from chord colour; make held A/O/E steering
  and U substitution momentary while retaining musical debts; star releases end
  their permissions without cutting off or silently reharmonizing the heard chord
- [x] Reuse Phase 21's scanner, USB-MIDI encoding, bounded delivery, and recovery
  in a shared board loop; register `asterisk_v1_4_autochord` without
  `KeyboardProcessor`, a generic program trait, or new dependencies
- [x] Test musical policy, connected note slides, partial catch release, repeated
  commands, inversion, MIDI ownership, overlapping held shapes, raw touch filtering,
  Stop, and offline recovery; preserve keyboard and MIDI profile coverage

When no supported four-voice result exists, selected anchors and any caught
inner notes continue; other inner voices are silent. Additional held keys guide
chord choices without sounding extra voices or supplying missing chord tones.
The initial physical note layout is C major, with chromatic harmony available
around those literal notes.
Tension chords retain destinations even while the current played notes prevent
arrival. Ordinary note changes can unlock those arrivals without an Arrive press;
thumbs steer subsequent changes, # bars reinterpret the played notes, and stars
permit borrowing and richer chord colours. Voice catches are available through
the program API independently of the physical star mapping.
Phrase tests cover classical cadences, jazz ii–V–I guide tones, inversions,
pedal harmony, ascending/descending bass figures, inner passing lines,
prepared ornaments, and applied dominants. Idle scans and modifier releases do
not advance a line; releasing a thumb ends its steering without changing the
sounding harmony. This is a declared, bounded performance
vocabulary rather than a complete Baroque or jazz grammar.

The [`AutoChord profile`](keyboards/tecla/asterisk_v1_4_autochord/) documents the
current playing map. **Software gate passed:** workspace host tests, host and
RP2040 Clippy, the `no_std` target check, release profile build, and UF2 generation.
**Physical completion gate pending:** play
connected phrases, try each thumb and direct bar, compare independent/combined
star permissions, and verify both-# Stop and USB recovery with
an external sound source. Add timed execution with its first consumer in Phase 23.

______________________________________________________________________

## Phase 23 — Single-Pattern MIDI Sequencer

One monophonic lane, one RAM-resident pattern, internal tempo, and physical
start/stop. Initial editing can happen while stopped.

- [ ] Store a bounded pattern of notes and gate lengths; implement step position,
  next-step deadline, and the currently active note/gate state
- [ ] Adapt the existing injected-time/deadline-wakeup pattern for this program;
  service due work even under input load, with explicit late-step behavior
- [ ] On stop, release sounding notes; preserve each gate's actual note identity
  until its note-off rather than looking it up again from an edited pattern
- [ ] Test event order, gate endings, pattern wrap, tempo changes, and stop with
  fake time; record basic step/gate timing on the board

**Completion gate:** one pattern drives an external synth without physical input,
with correct gate timing and stop behavior. Program-local deadlines are sufficient;
a general scheduler, independent lanes, or launch-queue system is not a prerequisite.

______________________________________________________________________

## Phase 24 — Chord Selection and Strumming

Extend the Phase 22 program: one control group selects root, chord quality, and
inversion; another triggers either a block chord or a low-to-high strum.

- [ ] Add bounded chord/voicing calculations for the demonstrated controls, using
  ordinary MIDI note numbers and checking note-range limits
- [ ] Keep the selected harmony separate from the notes captured at attack; define
  how release or a new attack ends an in-progress strum
- [ ] Reuse Phase 23 timing where it fits; extract a shared deadline helper only
  when it removes real duplication between the two programs
- [ ] Test chord changes while held, strum order, retrigger, and release; demonstrate
  both articulations through external MIDI on the existing board

**Completion gate:** the same selected chord can be played as a block or strum,
with correct releases. This does not require a tuning framework, MPE allocator,
all playing techniques, or a general harmony/counterpoint solver.

______________________________________________________________________

## Phase 25 — MIDI Event Looper

Record and repeat the note events produced by the reference instrument. Start
with one loop in RAM and record/replace/play/stop/clear controls.

- [ ] Store its emitted MIDI events with relative timestamps in a bounded buffer;
  make loop length and full-buffer behavior explicit
- [ ] Replay using the timing already demonstrated in Phase 23; define recording
  end and loop-boundary behavior so partial notes do not remain sounding
- [ ] Release active notes on stop/clear/replacement and test repeated playback,
  capacity exhaustion, and interruption with injected time

**Completion gate:** a played phrase records and loops on an external synth with
documented capacity and correct note endings. Overdub, undo, gesture interpolation,
generic automation, tracker editing, and flash persistence are separate follow-ons.

______________________________________________________________________

## Phase 26 — Reproducible Instrument Examples

Document and validate the examples as they land. Shared SDK code is the result
of demonstrated reuse, not a completion target of its own.

- [ ] Document each example's controls, board wiring, build/flash commands, external
  MIDI setup, tests, capacities, and known limits; include calibration or feedback
  only where that instrument actually uses them
- [ ] Reuse the current board-profile discovery, host tests, and build tooling;
  ensure feature-gated MIDI examples are actually selected in CI/board builds
- [ ] Distinguish host-tested, cross-built, and physically validated support;
  record firmware size and relevant timing measurements on the tested board
- [ ] Extract helpers where working programs duplicate behavior, keeping state
  specific to the programs that use it; port to a second compatible board when
  there is a real adapter/use case rather than requiring new hardware for this gate

**Completion gate:** a working reference instrument has reproducible instructions,
host tests, and a recorded hardware demonstration. A mandatory two-family hardware
campaign, all-inclusive template, new feature-flag matrix, and persistence layer
are not prerequisites.

### Instrument Follow-ons — Choose When a Reference Instrument Needs Them

These preserve the exploration directions without making every one a prerequisite
for a playable controller, sequencer, chord instrument, or looper:

- **External synchronization:** MIDI input and clock/start/stop handling when the
  sequencer must follow a host/device; then define clock loss and resumption
- **Expressive pitch:** pitch bend, fine tuning, release velocity, and MPE when a
  concrete control technique needs them; test against a compatible receiving synth
- **More composition tools:** arpeggiation, generative rhythm, independent lanes,
  and queued pattern changes when the chosen playing/editing controls need them
- **Tracker:** extend Phase 23 playback with a cell editor, reusable phrases, and
  phrase ordering; begin with edits while stopped and an external MIDI sound source
- **Gesture recording / richer looping:** use a real moving control to justify
  capturing original sensor/control values; add overdub, undo, interpolation,
  parameter locks, or automation only for the selected interaction
- **Baroque harmony/counterpoint:** a separate four-voice experiment with named
  supported rules, voice history, prepared suspensions, and infeasible-input tests;
  keep its policy independent of output and timing
- **Save/restore:** one versioned, program-owned record using existing storage
  dependencies when material must survive power cycles; wire a real board flash
  region before claiming persistence
- **Additional hardware/transports:** specific sensor drivers, DIN/BLE-MIDI, or
  CV/gate for a chosen instrument/board; no arbitrary-sensor support promise

______________________________________________________________________

## Research Program

**Thesis:** Tecla is a research platform for adaptive physical text entry
and accessible expert input.

Existing foundations include a pure/testable `KeyboardProcessor`, telemetry,
configurable filters, and reusable scanner/report boundaries. The `Recognizer`
trait currently has one production implementor, tap-hold; live combos and steno
use their own state machines. Unless checked complete below, research items
are proposals, not shipped adaptive behavior.

### Cross-cutting method: Bayesian parameter tuning

Multi-objective Bayesian optimization (MOBO) is one candidate method for
evaluating parameter recommendations. It is not implemented in Tecla.
Relevant work from Kristensson's group includes:

- AdaptiKeyboard (ISMAR 2022) — per-user MOBO for speed/accuracy tradeoff
- Cooperative MOBO (TiiS 2024) — firmware proposes, user's typing confirms
- Group-level MOBO (IJHCI 2024) — find good defaults across populations
- CaBoP (CHI 2026) — cost-aware BO that prioritizes low-disruption tweaks
- Bayesian Methods for Interaction and Design (CUP 2022) — the textbook

First compare recommendations against stable user-selected settings. A
lightweight Gaussian process or bandit is a possible later experiment if simpler
methods prove insufficient. Any output-changing adaptation must follow the
opt-in, bounded, reversible policy below; evaluation must include disruption.

______________________________________________________________________

### Lab A — Adaptive Accessibility

The strongest single research pillar. High-impact, fits ASSETS-style
work, connects directly to things Tecla already has: debounce, per-key
thresholds, telemetry, haptic interfaces, and layer logic. Personalized
adaptation remains planned.

**ABD-MT adaptive typing** ⭐ core research

Unified typing adaptation using the ABD-MT pattern (Wobbrock MobileHCI
2024): observe → model → adapt. Not a clinical tool — the system measures
keystroke timing statistics, models consistency relative to the user's
own baseline, and proposes firmware parameter changes. No diagnosis,
no ability labels, no medical claims. The research goal is to reduce unwanted
input without suppressing deliberate fast taps, repeated presses, or rolled
steno chords; benefits must be measured for each mechanism and user.

Observation is quiet by default. Output-changing adaptation starts in shadow
mode and requires an explicit profile setting, with bounded changes, manual
overrides, and rollback. A result from one adaptive interface does not establish
that silent adaptation is preferable for every recognizer or user.

User-facing rules for making adaptation reversible, inspectable, and
non-hostile live in [`docs/adaptive-ux.md`](docs/adaptive-ux.md). That
document is the product contract for this research section.

Proposed design principle: each telemetry struct collects one domain's raw
events. `ProcessorTelemetry` would synthesize across domains to produce
adaptation recommendations. No struct should compute
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
- [x] Windowed accessibility observations — `reset_deltas` zeroes the
  `delta_slow_accepts/rejects`, `delta_bounce_rejects`, and
  `delta_sticky_uses` fields. Snapshot rates use the delta window; bounce
  rejection rates include presses accepted by the bounce filter even when
  a later slow-key filter rejects them
- [x] Accessibility documentation describes observed input behavior without
  making medication or health claims

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
- [ ] Steno chord accumulation — add optional `settle_ms: Option<u64>` for
  rolled chord detection (currently pure structural all-release)
- [ ] `AutocorrectEngine` — extract hard-coded 30ms transposition
  threshold to configurable field with setter

*Step 4 — Adaptation methods on `ProcessorTelemetry` (the Adapter):*

All cross-cutting synthesis lives here. Individual telemetry structs
expose raw data; `ProcessorTelemetry` interprets it. Shared primitive
observation (pair table), divergent interpretation per recognizer:
combo asks "was this simultaneous or just fast sequential typing?"
steno asks "is the user done adding keys or is another finger coming?"

- [ ] Evaluate whether a board needs optional timed combo recognition before
  adding window recommendations. Current live combos have no timing window;
  preserve that behavior unless a concrete profile opts into different semantics
- [ ] `recommended_tapping_term(&self) -> u64` — from core hold-time
  consistency distribution
- [ ] `recommended_debounce(&self, key: usize) -> u16` — core per-key
  hold-time variance + accessibility bounce rejection delta rate.
  These signals identify a candidate for review, not proof that debounce
  is too short; compare rejected intentional repeats and added latency
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

*Step 5 — Chord timing experiments via pair table:*

- [ ] Compare optional timed combo semantics with the existing live chord model
  on a concrete board before introducing a recognizer or changing dispatch
- [ ] `StenoEngine`: evaluate pair spread as evidence for an optional settle
  policy alongside the existing all-release rule. Steno rolls are intentional
  technique, not imprecision

*Step 6 — Processor integration:*

- [ ] `KeyboardProcessor::adapt_from_telemetry(&mut self)` — reads
  `self.telemetry` and records `recommended_*` results in shadow mode.
  Apply only in an explicitly enabled adaptive profile, at safe recognizer
  boundaries; the 250ms telemetry tick is an observation cadence, not a
  requirement to change behaviour. No-op when telemetry is `None`
- [ ] Compare bounded recommendations against the user's stable baseline,
  including false activations, missed intentional input and response delay;
  freeze manual overrides and support immediate rollback

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
- [ ] Any optional timed combo policy preserves the current live model by default
- [ ] Debounce cross-cutting: core variance + accessibility rejections
  both influence per-key recommendation
- [ ] Session trend: all timing recommendations drift with session trend
- [ ] Cold boot: persisted baseline restores correctly
- [ ] Zero-cost: telemetry disabled → no adaptation overhead
- [ ] Intentional rapid repeats, fast taps and rolls remain available;
  default shadow mode, profile opt-in, manual freezes and rollback are tested

Treat debounce, tap-hold timing, combo windows and bounce filtering as separate,
reversible recommendations, not a progressive assistance ladder. Wider windows
can add delay, merge separate actions or reject deliberate repeats. Evaluate
each change against those costs and the user's intended actions; enabling one
mechanism must not silently enable another. Keep the accepted stable settings
available for comparison and restore them when a change makes input worse.

Enables later (not in scope): per-key debounce in board crate hardware
debouncers, host TUI showing live adaptation state, analog input
integration (rapid trigger thresholds from same profile).

- Literature: Wobbrock CREATE center, ABD-MT (MobileHCI 2024),
  Findlater (CHI 2012), Aalto CHI 2018 (136M keystrokes, rollover
  ratio as #1 speed differentiator), EdgeWrite (UIST 2003)

**Analog input** — core primitives implemented; hardware integration pending

- [x] `AnalogMatrixScanner` and `AsyncAnalogMatrixScanner` traits returning
  per-key `u8` travel, normalized by the board driver to 0–255
- [x] Integer EMA smoothing and expand-only calibration with a noise guard
- [x] `RapidTrigger` state tracking with asymmetric press/release sensitivity
  and press, bottom-out, release-from-bottom, and release transitions
- [x] `Action::Dks` and `KeyboardProcessor::process_dks_phase` for travel phases
- [ ] Connect an actual analog scanner to a board runtime and validate travel,
  calibration, and trigger behavior on hardware
- [ ] Per-key actuation settings; `RapidTrigger` currently uses one shared config
- [ ] Adaptive actuation recommendations after a stable hardware baseline
- [ ] Velocity-sensitive typing — key travel speed as input dimension
- Reference implementations: libhmk (C), Macrolev (ESP32), Minipad (RP2040)

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

**Steno** — implemented, with translation limitations

- [x] `StenoEngine` — chord accumulation until all-release
- [x] `StenoEngine` — on-device DAFSA dictionary translation (Lapwing)
- [x] GeminiPR / TXBOLT wire protocols for host-side Plover
- [x] `steno!` macro for compile-time steno keymap declaration
- [x] `CapTouchScanner` — capacitive touch sensing for RP2040
- [x] Boards: Steno Uni V4 (matrix), Asterisk v1.4 (cap touch)
- Hardware use demonstrates all-release chording, on-device translation, and
  capacitive scanning. Prefix-ambiguous multi-stroke words and some Plover
  formatting remain limited; see the [native board guide](keyboards/tecla/README.md)

**DAFSA steno dictionary compression** — implemented

- [x] Full Lapwing dictionary (159,805 entries, 5.5 MB JSON) compressed
  to 1.6 MB firmware binary — fits on a stock 2 MB RP2040 with no
  external flash
- [x] Techniques: DAFSA suffix sharing, u24 stroke/offset encoding
  (3 bytes instead of 4), compact variable-length text encoding,
  binary search over sorted children
- This removes an external-flash requirement for Tecla's dictionary storage.
  Dictionary fit does not imply full Plover translation compatibility or
  establish novelty relative to other firmware
- Potential venue: ASSETS or CHI — "Efficient On-Device Steno Dictionary
  Compression for Resource-Constrained Keyboards"

**Braille input/output** — product feature

- [ ] Braille chording — 6/8-key simultaneous input mapped to braille cells
- [ ] HID braille display output — USB braille display profile for
  screen-reader integration
- Strengthens the accessibility credibility story

______________________________________________________________________

### Lab B — Expert Expressive Input

Where expert users push the input model to its limits. Tecla's steno engine
and live combo handling provide existing chord-based input to evaluate.

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

**Instrument firmware** — platform track

- See [Instrument Firmware SDK — Phases 21–26](#instrument-firmware-sdk--phases-2126)
  for delivery milestones, reference instruments, and hardware validation
- Grid pads, faders, encoders, and velocity-sensitive keys are example controls;
  musical behavior belongs to instrument programs rather than keyboard actions
- SysEx patch selection/routing remains an extension when a concrete instrument
  requires it; internal sound generation is out of scope for this track

**Gesture input**

- [ ] Swipe-to-layer, pinch-to-zoom via trackpad/touchpad integration
- [ ] Build on existing `pointing_task` infrastructure

______________________________________________________________________

### Lab C — Predictive Assistive Typing

Where on-device inference meets text entry. Telemetry is the research
substrate — without it, adaptive ideas stay vibes.

**Live telemetry** ⭐ platform enabler (prerequisite for most research)

Existing architecture: `TelemetryExt` provides a common snapshot/reset
interface for collectors. Recording methods remain specific to each collector;
implementing the trait does not automatically wire observations or extend the
transport format.

```
KeyboardProcessor
├── optional ProcessorTelemetry<KEYS>
│   ├── CoreTelemetry<KEYS>
│   └── built-in modality collectors
└── optional TypingTelemetry
          │ snapshot / collect
          ▼
TelemetryFrame           (fixed set of optional payloads)
```

The trait defines `snapshot_state`, `clear_deltas`, and `collect`, with an
associated snapshot type and a no-op implementation for `()`. A custom modality
needs recording calls and explicit frame/host-decoder integration.

Core telemetry:

- [x] Per-key press count, hold time EMA + variance, correction-key error proxy
- [x] Press-to-press (PP) interval EMA in a bounded 64-entry bigram table
- [ ] Press-to-release, release-to-release, and release-to-press intervals;
  per-bigram variance and frequency-ranked retention
- [x] Rollover ratio (% of presses where next key down before prev up)
- [x] Raw press-rate WPM from a 50-entry timestamp ring buffer
- [x] Since-boot active typing average, excluding idle gaps and generated text;
  `TYPE_WPM` readout on Voyager Enter/Nav + W and NOS75 Fn + W, plus an optional
  `typing` telemetry payload. This is separate from the raw press-rate metric.
- [x] Collection points in processor press/release handling and backspace detection

Implemented modality collectors (runtime wiring depends on the profile):

- [x] **Steno** — dictionary hit/retry and multi-stroke completion rates,
  strokes per minute, undo counts
- [x] **MIDI** — note velocity mean/min/max, peak polyphony, CC message counts
- [x] **Gamepad** — observed per-axis range, button counts, peak held-button count
- [x] **Pointing** — mean movement counts per sample, button-down edge counts,
  click-with-movement coincidence, scroll event counts. The
  `mean_counts_per_sample` field has no timestamps and is not physical speed
  or an error rate
- [x] **Accessibility** — slow/bounce rejection rates, sticky activation counts,
  hold-time trends; these observations do not establish fatigue or health state
- [x] **Encoder** — clockwise/counter-clockwise counts and direction reversals

Additional observations remain planned:

- [ ] **Steno** — dictionary coverage and chord hold-time distributions
- [ ] **MIDI** — note durations, CC value ranges, intended-chord timing jitter
- [ ] **Gamepad** — dead-zone noise and button hold-time distributions
- [ ] **Pointing** — timestamped movement, hover-before-click observations
- [ ] **Encoder** — timestamped rotation speed
- [ ] **Split** — per-half press balance, cross-hand vs same-hand bigram
  timing, transport round-trip latency measurement
- [ ] **Cap touch** — per-pad discharge count distribution, threshold
  margin, ambient baseline drift over time

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
- [x] `CdcTelemetryWriter` (RP2040 and nRF52840) — CDC ACM writer implementations;
  firmware profiles must explicitly attach a telemetry endpoint and task
- [x] `TelemetryChannel` type for frame routing between tasks
- [x] Composite USB HID reporter — descriptors and wire encoding for NKRO,
  consumer, mouse, and gamepad, plus USB-MIDI behind the `usb-midi` feature.
  Shared in core (`transport::usb`) rather than per board.
- [ ] Haptic driver implementation for a board (DRV2605L); the trait, the
  controller, and the task plumbing exist but nothing drives real hardware
- [x] nRF52840 BLE firmware builds and links in the CI board matrix
- [ ] BLE hardware validation — building and linking do not validate the radio
- [ ] Publish `tecla-nrf52840` after hardware validation
- [ ] CDC ACM telemetry integration in remaining profiles/STM32 board crates;
  validate the standalone ESP32 writer with its firmware build
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

RAM cost depends on key count, enabled collectors, and channel capacities.
Measure the complete processor and task allocation for each target; telemetry
does not by itself establish whether an input was intended or successful.

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
