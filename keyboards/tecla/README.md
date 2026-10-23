# Tecla Native Boards

This directory holds native board profiles, including typing firmware,
MIDI instruments, and bring-up diagnostics. Each subdirectory is a profile
name accepted by `just flash --tecla-config <profile>`.

Typical contents:

- `board.toml` — board metadata, target, flash method, and profile name
- `examples/` — board wiring, MCU setup, and runtime glue
- `keymaps/` — the editable keymap / steno layout for that board
- `combos/` — optional combo definitions for that board

## Hardware status

| Board or reference design | MCU | Flash method | Hardware validation |
| --- | --- | --- | --- |
| StenoKeyboards Asterisk v1.4 | RP2040 | UF2 | Typing and basic MIDI playing confirmed; latest instrument changes need audition |
| StenoKeyboards Uni v4 | RP2040 | UF2 | Build only |
| Planck-style 4×12 | RP2040 | probe-rs | Build only |
| Split keyboard over UART | RP2040 | probe-rs | Build only |
| [NuPhy NOS75](nos75/README.md) | STM32F072 | DFU | Initial typing firmware enumerates; lighting and ROM recovery need hardware checks |
| ZSA Voyager | STM32F303 | DFU | USB and MIDI playing confirmed; latest changes need audition |
| Planck-style 4×12 | STM32F411 | probe-rs | Build only |
| Planck-style 4×12, USB or BLE | nRF52840 | probe-rs | Build only |

"Build only" means CI compiles the firmware; it does not establish that the
wiring or runtime works on a physical keyboard. Basic hardware confirmation
also does not validate every profile or recent revision. Local `just hil`
recipes run RP2040 and nRF52840 test firmware in Renode. ESP32-S3 remains
outside the main workspace and CI; its `esp-hal`/`esp-rtos` integration still
needs an Xtensa build and hardware validation.

## Profiles

- `asterisk_v1_4` — StenoKeyboards Asterisk v1.4 (steno, on-board Lapwing dictionary)
- `asterisk_v1_4_keytest` — Asterisk diagnostic: every key types a letter in pin order
- [`asterisk_v1_4_midi`](asterisk_v1_4_midi/README.md) — Asterisk USB MIDI note instrument
- [`asterisk_v1_4_autochord`](asterisk_v1_4_autochord/README.md) — four-voice bass/melody harmony with direct bars and harmonic star modifiers
- `asterisk_v1_4_chromatica` — polyphonic solo-tuned harmonica: stars select inhale, vowels slide chromatically
- `asterisk_v1_4_omnichord` — left-hand chord buttons with a soft chord bed and ten right-hand touch-harp strings
- `asterisk_v1_4_suspension` — gesture-driven suspension organ: move the foundation, retain prepared tones, and resolve them through harmonic movement
- `steno_uni_v4` — StenoKeyboards Uni v4 (steno)
- `planck-rp2040`
- `split-rp2040`
- `planck-nrf52840`
- `planck-nrf52840-ble`
- `planck-stm32f4`
- [`nos75`](nos75/README.md) — NuPhy NOS75 typing and RGB
- `voyager` — ZSA Voyager
- `voyager_usbtest` — USB/HID bring-up diagnostic for the Voyager
- `voyager_matrix_test` — raw matrix diagnostic for the Voyager
- `voyager_i2c_scan` — I2C bus scan diagnostic for the Voyager
- `voyager_trackball` — Navigator mouse diagnostic for the Voyager
- `voyager_tonnetz` — manual MIDI Tonnetz lattice, mirrored right hand
- `voyager_tonnetz_parallel` — parallel lattice with separated hand registers
- `voyager_four_finger` — chromatic rows with major thirds upward, separated hand registers
- `voyager_accordion` — chromatic columns with minor thirds across, separated hand registers
- `voyager_harmony` — polyphonic left Tonnetz with right-hand harmonic steering

MIDI profiles send notes to an external synthesizer or DAW; they have no
internal sound engine. Tonnetz primitives remain separate from AutoChord.

## Flashing

Install the tools for your board's flash method from
[deps.md](../../deps.md#flashing), then run from the repository root:

```sh
just flash --list                      # profiles, USB devices, and mounted UF2 targets
just flash --tecla-config <profile>    # build and flash a native profile
```

For UF2 and DFU profiles, the coordinator builds the firmware first, then waits
for the board's bootloader. If multiple matching devices are connected, pass
`--device <selector>` using the device identifiers from `--list`. SWD profiles
require a compatible debug probe and use the configured probe-rs runner.

For RP2040 UF2 profiles, hold the **BOOT/BOOTSEL** button while plugging USB in,
then release it. The `RPI-RP2` drive appears and the coordinator copies the
firmware onto it. On the Asterisk this is the small **B** push button, not the
steno **-B** touch pad. Keep capacitive pads untouched during startup calibration.
A bad application flash does not remove the RP2040's ROM bootloader: re-enter
BOOTSEL and copy the board's matching vendor `.uf2` to return to vendor firmware.

Voyager and NOS75 use DFU rather than a mounted drive. Follow the board's
bootloader-entry procedure; [NOS75's guide](nos75/README.md) also explains its
stock-settings and recovery constraints. DFU profiles accept `--dry-run` to
build and validate without accessing a device.

On-board steno profiles also need the dictionary submodule:

```sh
git submodule update --init .vendor/plover-lapwing-aio
```

They type words directly, so turn Plover off while using them.

## Typing statistics

The Voyager typing profile binds **held left Enter/Nav thumb + W** to a WPM
readout; NOS75 uses **Fn + W**. The shortcut types a result such as `72 WPM`
into the focused application, without Enter or a reset. Reading or typing the
result does not change the measurement. If a character key is held, the
readout waits for its release to avoid retyping it.

The counter starts fresh at boot and averages active typing since then:

- A word is five manually entered characters, including spaces and
  punctuation. Enter and Tab each count as one whitespace character.
- The first character of each burst establishes a timing baseline. Subsequent
  characters contribute to speed over intervals of at most two seconds;
  longer gaps start another burst without adding idle time. Isolated
  characters alone do not establish a rate.
- Backspace and Delete add active time within a burst, but no characters.
  Correction effort lowers the result without judging spelling accuracy.
- Modifiers, navigation, shortcuts, macros, and generated text add no
  characters. Host-side auto-repeat, pasted text, IME composition, and
  application edits are not visible to the firmware.

Typed readouts use the same US host-layout convention as text playback. The
optional `typing` telemetry payload exposes the active average, active
milliseconds, and total accepted characters. That total includes the first
character of each burst, even though it establishes a baseline for the rate.

### Keymaps and custom runtimes

Use `TYPE_WPM` in a keymap and `.with_typing_stats()` on its
`KeyboardProcessor`. The existing `.with_telemetry()` builder enables the
same collector alongside detailed diagnostics; NOS75 uses the smaller
standalone collector to fit its RAM budget.

Custom runtimes must send the ordinary reports from `process_event` or `tick`
before calling `drain_wpm_readout` for a separate text batch. The shared Tecla
runtimes already handle this. See the
[processor implementation](../../crates/tecla/src/keyboard/processor.rs) and
[typing collector](../../crates/tecla/src/telemetry/typing.rs) for the API and
timing rules.

## Asterisk Chromatica

All eighteen letter pads can sound together. Each group of four holes exhales
**C E G C** and inhales **D F A B**; this is solo tuning for a chromatic
harmonica. Upper-row pads repeat their lower partners one octave higher.
The left lower **S K W R** starts at C3; right lower **R B G S Z** starts at C4
and adds the next octave's first hole on Z. Duplicate pitches share ownership.

- Hold either **star** for inhale; releasing both returns to exhale.
- Hold **O** or **E** to raise all held notes one semitone.
- Hold **A** or **U** to lower them one semitone. Opposing slides cancel.
- Hold **left #** for an octave down or **right #** for an octave up.
- Hold **both # bars** to stop and require fresh touches.

Breath, slide and octave changes immediately change held notes. Attacks use
velocity 100 on MIDI channel 1. Use a polyphonic synth patch to hear chords;
the firmware has no sound engine or breath-pressure model.

## Asterisk Suspension

`asterisk_v1_4_suspension` plays a shared harmonic body of up to six voices.
Letter pads express musical actions rather than fixed pitches. The planner
remembers the heard harmony and its destination; new contacts advance the
phrase, while holding still does not sequence or retrigger it.

| Physical pads | Action |
| --- | --- |
| Left upper **S / T** | Prepare a destination / lean toward it with dominant tension |
| Left lower **S / K** | Arrive at the destination / take a deceptive resolution |
| Left upper **P**, lower **W** | Move the harmonic reference up / down one scale step |
| Left upper **H** | Continue a descending-fifths sequence |
| Left lower **R** | Seek a substitute interpretation |
| Right upper **F / D** | Raise / lower the generated upper voices |
| Right upper **P / T** | Spread / gather the generated upper voices |
| Right upper **L** | Reinterpret through common tones |
| Right lower **R B G S Z** | Catch currently sounding upper voices, then release them to resolve |
| **A O E U** | Momentarily steer prepare / lean / arrive / substitute |
| Left / right **#** | Request third / fifth in the bass |
| Left / right **star** | Allow borrowed harmony / extended chord colours |
| Both **#** | Stop and require fresh letter touches |

The catch pads cover overlapping bands from lower to higher upper voices;
**G** catches all upper voices. They capture the voices sounding when touched,
not fixed pitches. The bass remains free to move beneath a catch. Releasing a
catch resolves that band's pending movement without restarting the phrase.

Start on left lower **S**, transfer to left upper **S**, then **T**, then left
lower **S** again: prepare, tension, arrival. To hear a delayed resolution,
catch right lower **G** before tapping **T** and the arrival; release **G**
afterward. Release a pad before tapping it again. Keep at least one letter
contact held between gestures; lifting all letters for 24 ms silences the body.
Bars, stars and vowels alone do not start sound. MIDI uses channel 1 and
velocity 88, with the same capacitive scanner and recovery path as the other
Asterisk instruments. Use an external polyphonic synth; there is no internal
sound engine.

## Voyager manual note maps

These four profiles play the notes you press, with shared controls and colours.
Pitch names below use MIDI 60 = C4; ranges are before the global octave shift.

| Profile | Across / upward | Left range | Right range |
| --- | --- | --- | --- |
| `voyager_tonnetz` | +3 / +4 semitones; right hand mirrors horizontally | B1–D4 | B2–D5 |
| `voyager_tonnetz_parallel` | +3 / +4 semitones | B1–D4 | E♭4–F♯6 |
| `voyager_four_finger` | +1 / +4 semitones | B2–E4 | F4–B♭5 |
| `voyager_accordion` | +3 / +1 semitones | A2–E♭4 | E4–B♭5 |

Four-finger and accordion cover every semitone in each hand's range. Both put
C3 on the left-hand **Z** position. Try C major with **Z+A+F** on four-finger
and **Z+S+D** on accordion, using the Voyager's default typing positions.

| Physical control | Action |
| --- | --- |
| Left outer/front thumb | Hold for one octave down on new notes |
| Left inner/back thumb | Hold for one octave up on new notes |
| Right inner/back thumb | Tap damper on/off |
| Right outer/front thumb | Hold for accented new attacks, velocity 120 instead of 88 |
| Top-left Esc | Stop, clear damper, and require fresh presses |

Both octave thumbs cancel. Held notes keep their attack-time pitches. The
parallel, four-finger and accordion registers are adjacent and separated at
each global octave setting; notes held across different settings can overlap.
Damper retains released notes; switching it off preserves physically held notes.

Natural notes have a dim white background, accidentals are dark, and C
references are brighter. Activity uses fixed pitch-class colours: held notes,
exact duplicates, other octaves and retained notes have distinct brightness.
Softer harmonic hints show triad completions and Tonnetz destinations without
generating notes. Stop remains red; thumb LEDs show their accepted state.

Native profiles are separate from the vendored QMK definitions elsewhere under
`keyboards/`.
