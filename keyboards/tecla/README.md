# Tecla Native Boards

This directory holds Tecla-native board definitions.

Each subdirectory is a stable, human-readable profile name that maps to a
Tecla-native board definition.

Typical contents:

- `board.toml` — board metadata, target, flash method, and profile name
- `examples/` — board wiring, MCU setup, and runtime glue
- `keymaps/` — the editable keymap / steno layout for that board
- `combos/` — optional combo definitions for that board

Current profiles:

- `asterisk_v1_4` — StenoKeyboards Asterisk v1.4 (steno, on-board Lapwing dictionary)
- `asterisk_v1_4_keytest` — Asterisk diagnostic: every key types a letter in pin order
- [`asterisk_v1_4_midi`](asterisk_v1_4_midi/README.md) — Asterisk USB MIDI note instrument (basic playing confirmed)
- [`asterisk_v1_4_autochord`](asterisk_v1_4_autochord/README.md) — four-voice bass/melody harmony with direct bars and harmonic star modifiers (builds; physical validation pending)
- `asterisk_v1_4_chromatica` — polyphonic solo-tuned harmonica: stars select inhale, vowels slide chromatically
- `asterisk_v1_4_omnichord` — left-hand chord buttons with a soft chord bed and ten right-hand touch-harp strings
- `steno_uni_v4` — StenoKeyboards Uni v4 (steno)
- `planck-rp2040`
- `split-rp2040`
- `planck-nrf52840`
- `planck-nrf52840-ble`
- `planck-stm32f4`
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

Flash any of them with `just flash --tecla-config <profile>`; `just flash --list`
shows them all.

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

These files are intentionally separate from the vendored QMK tree under
`keyboards/`.
