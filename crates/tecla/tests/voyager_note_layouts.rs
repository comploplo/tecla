#![allow(clippy::tests_outside_test_module, reason = "integration tests")]
#![allow(missing_docs, reason = "integration tests")]
#![expect(clippy::unwrap_used, reason = "test assertions")]

#[path = "../../../keyboards/tecla/voyager_tonnetz/instrument.rs"]
mod instrument;

use instrument::colours::{self, NoteState};
use instrument::{Instrument, Orientation};
use std::collections::BTreeSet;
use tecla::devices::midi::output;
use tecla::matrix::MatrixState;
use tecla::pipeline::report::MidiEvent;

const MAPS: [Orientation; 2] = [Orientation::FourFinger, Orientation::Accordion];
const DOWN: usize = 24;
const UP: usize = 25;
const DAMPER: usize = 50;
const ACCENT: usize = 51;

// Independently transcribed physical keys, including bottom-right N at (10,2).
const POSITIONS: [(usize, usize); 52] = [
    (0, 1),
    (0, 2),
    (0, 3),
    (0, 4),
    (0, 5),
    (0, 6),
    (1, 1),
    (1, 2),
    (1, 3),
    (1, 4),
    (1, 5),
    (1, 6),
    (2, 1),
    (2, 2),
    (2, 3),
    (2, 4),
    (2, 5),
    (2, 6),
    (3, 1),
    (3, 2),
    (3, 3),
    (3, 4),
    (3, 5),
    (4, 4),
    (5, 0),
    (5, 1),
    (6, 0),
    (6, 1),
    (6, 2),
    (6, 3),
    (6, 4),
    (6, 5),
    (7, 0),
    (7, 1),
    (7, 2),
    (7, 3),
    (7, 4),
    (7, 5),
    (8, 0),
    (8, 1),
    (8, 2),
    (8, 3),
    (8, 4),
    (8, 5),
    (10, 2),
    (9, 1),
    (9, 2),
    (9, 3),
    (9, 4),
    (9, 5),
    (11, 5),
    (11, 6),
];

const FOUR_LEFT: [u8; 24] = [
    59, 60, 61, 62, 63, 64, 55, 56, 57, 58, 59, 60, 51, 52, 53, 54, 55, 56, 47, 48, 49, 50, 51, 52,
];
const FOUR_RIGHT: [u8; 24] = [
    77, 78, 79, 80, 81, 82, 73, 74, 75, 76, 77, 78, 69, 70, 71, 72, 73, 74, 65, 66, 67, 68, 69, 70,
];
const ACCORDION_LEFT: [u8; 24] = [
    48, 51, 54, 57, 60, 63, 47, 50, 53, 56, 59, 62, 46, 49, 52, 55, 58, 61, 45, 48, 51, 54, 57, 60,
];
const ACCORDION_RIGHT: [u8; 24] = [
    67, 70, 73, 76, 79, 82, 66, 69, 72, 75, 78, 81, 65, 68, 71, 74, 77, 80, 64, 67, 70, 73, 76, 79,
];

fn note(map: Orientation, led: usize) -> Option<u8> {
    let (left, right) = match map {
        Orientation::FourFinger => (&FOUR_LEFT, &FOUR_RIGHT),
        Orientation::Accordion => (&ACCORDION_LEFT, &ACCORDION_RIGHT),
        _ => unreachable!("only the two new audition maps"),
    };
    match led {
        1..=23 => Some(left[led]),
        26..=49 => Some(right[led - 26]),
        _ => None,
    }
}

fn update(instrument: &mut Instrument, pads: &[usize], ready: bool) -> Vec<MidiEvent> {
    let mut matrix = MatrixState::new();
    for &pad in pads {
        let (row, col) = POSITIONS[pad];
        assert!(matrix.set(row, col, true));
    }
    let mut events: heapless::Vec<MidiEvent, { instrument::MAX_EVENTS }> = heapless::Vec::new();
    instrument.update(&matrix, ready, &mut |event| events.push(event).unwrap());
    events.into_iter().collect()
}

fn on(note: u8) -> MidiEvent {
    MidiEvent::NoteOn {
        channel: 0,
        note,
        velocity: 88,
    }
}

fn off(note: u8) -> MidiEvent {
    MidiEvent::NoteOff { channel: 0, note }
}

fn pressed_pitches(map: Orientation, pads: &[usize], offset: i16) -> BTreeSet<u8> {
    pads.iter()
        .filter_map(|&pad| note(map, pad))
        .map(|pitch| u8::try_from(i16::from(pitch) + offset).unwrap())
        .collect()
}

#[test]
fn all_fifty_two_physical_positions_and_every_octave_match_the_two_note_tables() {
    assert_eq!(instrument::POSITIONS, POSITIONS);
    assert_eq!(POSITIONS.into_iter().collect::<BTreeSet<_>>().len(), 52);
    for map in MAPS {
        for (thumbs, offset) in [(&[DOWN][..], -12i16), (&[][..], 0), (&[UP][..], 12)] {
            for pad in 0..52 {
                assert_eq!(instrument::base_note(map, pad), note(map, pad));
                let mut instrument = Instrument::new(map);
                assert!(update(&mut instrument, thumbs, true).is_empty());
                let mut pressed = thumbs.to_vec();
                pressed.push(pad);
                let events = update(&mut instrument, &pressed, true);
                if let Some(pitch) = note(map, pad) {
                    let shifted = u8::try_from(i16::from(pitch) + offset).unwrap();
                    assert_eq!(events, [on(shifted)], "{map:?}, pad {pad}, shift {offset}");
                    assert_eq!(
                        instrument.frame()[pad],
                        colours::note(shifted, NoteState::Held)
                    );
                    assert_eq!(update(&mut instrument, thumbs, true), [off(shifted)]);
                } else {
                    assert!(events.is_empty(), "a control must not play a note");
                }
            }
        }
        assert_eq!(instrument::base_note(map, 52), None);
        assert_eq!(instrument::base_note(map, usize::MAX), None);
    }
}

#[test]
fn each_half_is_chromatically_complete_and_registers_are_adjacent_at_every_octave() {
    for map in MAPS {
        let (left_low, left_high, right_low, right_high) = match map {
            Orientation::FourFinger => (47, 64, 65, 82),
            Orientation::Accordion => (45, 63, 64, 82),
            _ => unreachable!(),
        };
        for offset in [-12i16, 0, 12] {
            let left = pressed_pitches(map, &(1..24).collect::<Vec<_>>(), offset);
            let right = pressed_pitches(map, &(26..50).collect::<Vec<_>>(), offset);
            let expected = |low, high| {
                (low..=high)
                    .map(|pitch| u8::try_from(pitch + offset).unwrap())
                    .collect::<BTreeSet<_>>()
            };
            assert_eq!(left, expected(left_low, left_high));
            assert_eq!(right, expected(right_low, right_high));
            assert_eq!(*left.last().unwrap() + 1, *right.first().unwrap());
            assert!(left.is_disjoint(&right));
        }
    }
}

#[test]
fn chromatic_and_major_scale_phrases_are_literal_and_legato() {
    for (map, chromatic, major) in [
        (
            Orientation::FourFinger,
            [19, 20, 21, 22, 23, 14, 15, 16, 17, 8, 9, 10, 1],
            [19, 21, 13, 14, 16, 8, 10, 1],
        ),
        (
            Orientation::Accordion,
            [19, 13, 7, 1, 14, 8, 2, 15, 9, 3, 16, 10, 4],
            [19, 7, 14, 8, 15, 3, 10, 4],
        ),
    ] {
        for (pads, pitches) in [
            (&chromatic[..], (48..=60).collect::<Vec<_>>()),
            (&major[..], vec![48, 50, 52, 53, 55, 57, 59, 60]),
        ] {
            let mut instrument = Instrument::new(map);
            let mut previous = None;
            for (&pad, pitch) in pads.iter().zip(pitches) {
                let mut expected = previous.map(off).into_iter().collect::<Vec<_>>();
                expected.push(on(pitch));
                assert_eq!(update(&mut instrument, &[pad], true), expected);
                previous = Some(pitch);
            }
            assert_eq!(update(&mut instrument, &[], true), [off(60)]);
        }
    }
}

#[test]
fn changing_major_minor_and_seventh_shapes_keeps_common_tones() {
    for (map, major, minor, seventh) in [
        (Orientation::FourFinger, [19, 13, 16], [19, 12, 16], 9),
        (Orientation::Accordion, [19, 14, 15], [19, 20, 15], 16),
    ] {
        let mut instrument = Instrument::new(map);
        assert_eq!(
            update(&mut instrument, &major, true),
            [on(48), on(52), on(55)]
        );
        assert_eq!(update(&mut instrument, &minor, true), [off(52), on(51)]);
        let mut dominant = major.to_vec();
        dominant.push(seventh);
        assert_eq!(
            update(&mut instrument, &dominant, true),
            [off(51), on(52), on(58)]
        );
        assert!(update(&mut instrument, &dominant, true).is_empty());
        assert_eq!(
            update(&mut instrument, &[], true),
            [off(48), off(52), off(55), off(58)]
        );
    }
}

#[test]
fn duplicate_keys_rearticulate_share_ownership_and_keep_attack_time_pitch_and_velocity() {
    for (map, first, duplicate) in [
        (Orientation::FourFinger, 1, 11),
        (Orientation::Accordion, 4, 23),
    ] {
        let mut instrument = Instrument::new(map);
        let pitch = note(map, first).unwrap();
        assert_eq!(update(&mut instrument, &[first], true), [on(pitch)]);
        assert_eq!(
            update(&mut instrument, &[first, duplicate, ACCENT], true),
            [
                off(pitch),
                MidiEvent::NoteOn {
                    channel: 0,
                    note: pitch,
                    velocity: 120
                },
            ]
        );
        assert!(update(&mut instrument, &[duplicate, UP], true).is_empty());
        // New opposite-hand attack is shifted, while the duplicate stays pinned.
        let right = note(map, 44).unwrap();
        assert_eq!(
            update(&mut instrument, &[duplicate, UP, 44], true),
            [on(right + 12)]
        );
        assert!(update(&mut instrument, &[duplicate, UP, DOWN, 44], true).is_empty());
        assert_eq!(
            update(&mut instrument, &[duplicate, UP, DOWN, 44, 45], true),
            [on(note(map, 45).unwrap())]
        );
        let released = update(&mut instrument, &[], true);
        assert_eq!(
            released,
            [pitch, right + 12, note(map, 45).unwrap()]
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(off)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn damper_off_preserves_held_notes_and_stop_and_reconnect_require_fresh_presses() {
    for map in MAPS {
        let mut instrument = Instrument::new(map);
        let left = note(map, 19).unwrap();
        let right = note(map, 44).unwrap();
        assert_eq!(
            update(&mut instrument, &[DAMPER, 19, 44], true),
            [on(left), on(right)]
        );
        assert_eq!(instrument.frame()[DAMPER], colours::control(85, true));
        assert!(update(&mut instrument, &[44], true).is_empty());
        assert_eq!(
            instrument.frame()[19],
            colours::note(left, NoteState::Retained)
        );
        assert_eq!(update(&mut instrument, &[44, DAMPER], true), [off(left)]);
        assert_eq!(
            instrument.frame()[44],
            colours::note(right, NoteState::Held)
        );
        assert_eq!(
            update(&mut instrument, &[0, 44, UP, ACCENT, DAMPER], true),
            [off(right)]
        );
        let frame = instrument.frame();
        assert_eq!(frame[0], colours::stop(true));
        for (pad, hue) in [(DOWN, 128), (UP, 192), (DAMPER, 85), (ACCENT, 16)] {
            assert_eq!(frame[pad], colours::control(hue, false));
        }
        assert!(update(&mut instrument, &[44, UP, ACCENT, DAMPER], true).is_empty());
        update(&mut instrument, &[], true);
        assert_eq!(update(&mut instrument, &[19], true), [on(left)]);
        assert_eq!(
            update(&mut instrument, &[19, 44, UP, ACCENT, DAMPER], false),
            [off(left)]
        );
        assert!(update(&mut instrument, &[19, 44, UP, ACCENT, DAMPER], true).is_empty());
        update(&mut instrument, &[], true);
        assert_eq!(update(&mut instrument, &[19], true), [on(left)]);
        assert_eq!(update(&mut instrument, &[], true), [off(left)]);
    }
}

#[test]
fn every_held_note_lights_duplicates_and_octaves_without_sounding_them() {
    for map in MAPS {
        for pad in (1..24).chain(26..50) {
            let mut instrument = Instrument::new(map);
            let pitch = note(map, pad).unwrap();
            assert_eq!(update(&mut instrument, &[pad], true), [on(pitch)]);
            let frame = instrument.frame();
            for other in (1..24).chain(26..50) {
                let candidate = note(map, other).unwrap();
                let state = if other == pad {
                    NoteState::Held
                } else if candidate == pitch {
                    NoteState::ExactHeld
                } else if candidate % 12 == pitch % 12 {
                    NoteState::OctaveHeld
                } else {
                    NoteState::Idle
                };
                assert_eq!(frame[other], colours::note(candidate, state));
            }
            assert!(update(&mut instrument, &[pad], true).is_empty());
            assert_eq!(instrument.frame(), frame);
        }
        let mut instrument = Instrument::new(map);
        let triad = if map == Orientation::FourFinger {
            [19, 13, 16]
        } else {
            [19, 14, 15]
        };
        assert_eq!(
            update(&mut instrument, &triad, true),
            [on(48), on(52), on(55)]
        );
        let frame = instrument.frame();
        for pad in (1..24).chain(26..50) {
            let pitch = note(map, pad).unwrap();
            if [3, 9, 11].contains(&(pitch % 12)) {
                assert_eq!(frame[pad], colours::note(pitch, NoteState::Hint));
            }
        }
        assert!(update(&mut instrument, &triad, true).is_empty());
    }
}

#[test]
fn full_polyphonic_sustain_sweeps_preserve_wire_ownership_across_midi_packet_boundaries() {
    for map in MAPS {
        let mut instrument = Instrument::new(map);
        let pads: Vec<_> = (1..24).chain(26..50).collect();
        let mut active = BTreeSet::new();
        let mut desired = BTreeSet::new();
        update(&mut instrument, &[DAMPER], true);
        update(&mut instrument, &[], true);
        for (thumb, offset) in [(Some(DOWN), -12i16), (None, 0), (Some(UP), 12)] {
            let mut pressed = pads.clone();
            pressed.extend(thumb);
            let events = update(&mut instrument, &pressed, true);
            assert!(events.len() > output::PACKET_EVENTS);
            let mut attacked = false;
            for chunk in events.chunks(output::PACKET_EVENTS) {
                let mut packet = [0; output::PACKET_BYTES];
                let len = output::pack_events(chunk, &mut packet).unwrap();
                for bytes in packet[..len].as_chunks::<4>().0 {
                    match *bytes {
                        [0x08, 0x80, pitch, 0] => {
                            assert!(!attacked, "all releases precede attacks across USB packets");
                            assert!(active.remove(&pitch));
                        }
                        [0x09, 0x90, pitch, 88] => {
                            attacked = true;
                            assert!(active.insert(pitch));
                        }
                        _ => panic!("unexpected USB MIDI event: {bytes:?}"),
                    }
                }
            }
            desired.extend(pressed_pitches(map, &pads, offset));
            assert_eq!(active, desired);
            assert!(update(&mut instrument, &[], true).is_empty());
        }
        assert_eq!(
            update(&mut instrument, &[0], true),
            active.into_iter().map(off).collect::<Vec<_>>()
        );
        assert!(update(&mut instrument, &[], true).is_empty());
    }
}
