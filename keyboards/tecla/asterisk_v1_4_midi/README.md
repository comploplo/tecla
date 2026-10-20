# Asterisk v1.4 USB MIDI

This profile turns the Asterisk's 26 capacitive touch pads into a USB MIDI
controller for an external synthesizer or DAW. It does not generate sound.
Basic playing on hardware has been confirmed by the user. Full pad mapping,
Stop, and USB recovery validation remain pending.

Build the firmware with:

```sh
cargo build --release -p tecla-rp2040 --example asterisk_v1_4_midi --features usb-midi --target thumbv6m-none-eabi
```

Flash it through the existing profile coordinator:

```sh
just flash --tecla-config asterisk_v1_4_midi
```

Enter the RP2040's BOOTSEL mode when the coordinator requests its UF2 volume.
Leave all pads untouched while the firmware starts and calibrates the capacitive
scanner. In a MIDI monitor or DAW, select the Asterisk MIDI input and route it to
an external instrument.

## Provisional mapping

Notes use MIDI channel 1 with a fixed attack velocity of 100. Touch sensing is
binary; this profile does not measure strike velocity or pressure. The mapping
follows scanner GPIO order, so it does not yet establish a chromatic arrangement
on the physical pads. MIDI note numbers avoid DAW-specific octave labels.

| Scanner index | GPIO | MIDI note / action |
| --- | --- | --- |
| 0 | 0 | 48 |
| 1 | 25 | 49 |
| 2 | 5 | 50 |
| 3 | 4 | 51 |
| 4 | 3 | 52 |
| 5 | 2 | 53 |
| 6 | 1 | 54 |
| 7 | 15 | 55 |
| 8 | 24 | 56 |
| 9 | 23 | 57 |
| 10 | 22 | 58 |
| 11 | 21 | 59 |
| 12 | 20 | 60 |
| 13 | 9 | 61 |
| 14 | 8 | 62 |
| 15 | 7 | 63 |
| 16 | 6 | 64 |
| 17 | 14 | 65 |
| 18 | 19 | 66 |
| 19 | 18 | 67 |
| 20 | 17 | 68 |
| 21 | 16 | 69 |
| 22 | 10 | 70 |
| 23 | 11 | 71 |
| 24 | 12 | 72 |
| 25 | 13 | Stop |

## Stop and USB recovery

Touching Stop sends an explicit Note Off for each of the 25 configured notes.
New attacks are suppressed while Stop is held and while cleanup is in progress.
After a stop or recovery, lift and touch a note pad again to sound it.

The scanner continues running while USB is disconnected. Offline attacks are
discarded. On reconnection or a failed write, the firmware clears pending output
and sends the Note Off sweep before accepting new attacks. An unplugged cable
prevents delivery until the host reconnects, so a receiving synthesizer may need
its own panic control during the interruption.

All live MIDI packets from one scan share a 10 ms deadline. Each recovery write
has its own 10 ms timeout, with one cleanup message sent per scan. A sweep takes
at least 25 scans after the connection becomes writable. This profile sends MIDI
only and does not process incoming MIDI.

## Hardware validation still required

- Confirm USB MIDI enumeration and the note mapping for every physical pad.
- Hold several pads, then release them individually; confirm matching Note Off
  messages and no stuck notes.
- Exercise the documented stop and USB recovery paths while notes are held.
- Confirm the external instrument returns to silence after each recovery test.

Record the host, receiving instrument, firmware revision, and observed results
before treating this profile as a completed physical demonstration.
