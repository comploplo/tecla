#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test assertions")]

// Exercise the actual firmware instrument with independently transcribed
// physical positions and MIDI notes, rather than deriving the test oracle
// from its mapping helpers.
#[path = "../../../keyboards/tecla/voyager_tonnetz/instrument.rs"]
mod instrument;
#[path = "../../../keyboards/tecla/voyager_tonnetz/scan_guard.rs"]
mod scan_guard;

use std::collections::BTreeSet;
use tecla::matrix::MatrixState;
use tecla::pipeline::report::MidiEvent;
use tecla::rgb::{hsv_to_rgb, Hsv, Rgb};

use instrument::{Instrument, Orientation};

const ORIENTATIONS: [Orientation; 2] = [Orientation::Mirrored, Orientation::Parallel];
const STOP: usize = 0;
const DOWN: usize = 24;
const UP: usize = 25;
const DAMPER: usize = 50;
const ACCENT: usize = 51;

// This is physical LED order, including the right bottom N key at (10,2).
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

const LEFT: [u8; 24] = [
    47, 50, 53, 56, 59, 62, 43, 46, 49, 52, 55, 58, 39, 42, 45, 48, 51, 54, 35, 38, 41, 44, 47, 50,
];
const MIRRORED_RIGHT: [u8; 24] = [
    74, 71, 68, 65, 62, 59, 70, 67, 64, 61, 58, 55, 66, 63, 60, 57, 54, 51, 62, 59, 56, 53, 50, 47,
];
const PARALLEL_RIGHT: [u8; 24] = [
    75, 78, 81, 84, 87, 90, 71, 74, 77, 80, 83, 86, 67, 70, 73, 76, 79, 82, 63, 66, 69, 72, 75, 78,
];

fn expected_note(orientation: Orientation, led: usize) -> Option<u8> {
    match led {
        1..=23 => Some(LEFT[led]),
        26..=49 => Some(match orientation {
            Orientation::Mirrored => MIRRORED_RIGHT[led - 26],
            Orientation::Parallel => PARALLEL_RIGHT[led - 26],
            Orientation::FourFinger | Orientation::Accordion => {
                unreachable!("these maps have independent voyager_note_layouts tests")
            }
        }),
        _ => None,
    }
}

fn matrix(leds: &[usize]) -> MatrixState<12, 7> {
    let mut matrix = MatrixState::new();
    for &led in leds {
        let (row, col) = POSITIONS[led];
        assert!(matrix.set(row, col, true));
    }
    matrix
}

fn update(instrument: &mut Instrument, leds: &[usize], ready: bool) -> Vec<MidiEvent> {
    let mut events = Vec::new();
    instrument.update(&matrix(leds), ready, &mut |event| events.push(event));
    events
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

fn apply(active: &mut BTreeSet<u8>, events: &[MidiEvent]) {
    let mut attacked = false;
    for &event in events {
        match event {
            MidiEvent::NoteOn {
                channel,
                note,
                velocity,
            } => {
                assert_eq!(channel, 0);
                assert!([88, 120].contains(&velocity));
                assert!(active.insert(note), "attack without release: {event:?}");
                attacked = true;
            }
            MidiEvent::NoteOff { channel, note } => {
                assert_eq!(channel, 0);
                assert!(!attacked, "all releases must precede attacks");
                assert!(active.remove(&note), "release without ownership: {event:?}");
            }
            _ => panic!("manual instrument emitted a controller event: {event:?}"),
        }
    }
}

fn piano_idle(pc: u8) -> Rgb {
    match pc % 12 {
        0 => Rgb::new(40, 40, 40),
        2 | 4 | 5 | 7 | 9 | 11 => Rgb::new(24, 24, 24),
        _ => Rgb::BLACK,
    }
}

fn colour(pc: u8, value: u8, held: bool) -> Rgb {
    if !held && matches!(value, 16 | 24) {
        return piano_idle(pc);
    }
    // Written as the published circle-of-fifths indices rather than calling
    // the firmware's pitch-colour helper.
    const FIFTHS: [u16; 12] = [0, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];
    let hue = u8::try_from((32 + 256 * FIFTHS[usize::from(pc)] / 12) % 256).unwrap();
    let saturation = if held {
        120
    } else if value == 112 {
        160
    } else {
        210
    };
    hsv_to_rgb(Hsv::new(hue, saturation, value))
}

fn pad_for_pc(pc: u8) -> usize {
    (1..24).find(|&led| LEFT[led] % 12 == pc).unwrap()
}

fn assert_hints(orientation: Orientation, held: &[usize], hints: &BTreeSet<u8>) {
    let mut instrument = Instrument::new(orientation);
    let events = update(&mut instrument, held, true);
    let expected: BTreeSet<_> = held
        .iter()
        .map(|&led| expected_note(orientation, led).unwrap())
        .collect();
    let mut active = BTreeSet::new();
    apply(&mut active, &events);
    assert_eq!(active, expected, "hints may never sound additional notes");
    let frame = instrument.frame();
    for (led, rgb) in frame.into_iter().enumerate() {
        if let Some(note) = expected_note(orientation, led) {
            let pc = note % 12;
            let pressed = held.contains(&led);
            let value = if pressed {
                144
            } else if expected.contains(&note) {
                112
            } else if expected.iter().any(|held| held % 12 == pc) {
                96
            } else if hints.contains(&pc) {
                48
            } else if pc == 0 {
                24
            } else {
                16
            };
            assert_eq!(
                rgb,
                colour(pc, value, pressed),
                "LED {led}, held {held:?}, hint PCs {hints:?}"
            );
        }
    }
    assert!(update(&mut instrument, held, true).is_empty());
    assert_eq!(instrument.frame(), frame, "unchanged state cannot animate");
    let released = update(&mut instrument, &[], true);
    apply(&mut active, &released);
    assert!(active.is_empty());
}

#[test]
fn physical_positions_match_all_fifty_two_key_leds() {
    assert_eq!(instrument::POSITIONS, POSITIONS);
    assert_eq!(instrument::LEDS, 52);
    assert_eq!(instrument::CHANNEL, 0);
    assert_eq!(instrument::STOP, STOP);
    assert_eq!(instrument::OCTAVE_DOWN, DOWN);
    assert_eq!(instrument::OCTAVE_UP, UP);
    assert_eq!(instrument::DAMPER, DAMPER);
    assert_eq!(instrument::ACCENT, ACCENT);
    let unique: BTreeSet<_> = POSITIONS.into_iter().collect();
    assert_eq!(unique.len(), 52);
}

#[test]
fn every_physical_note_uses_the_published_register_in_both_orientations() {
    for orientation in ORIENTATIONS {
        for led in 0..52 {
            assert_eq!(
                instrument::base_note(orientation, led),
                expected_note(orientation, led)
            );
            if let Some(note) = expected_note(orientation, led) {
                for (thumb, offset) in [(None, 0i16), (Some(DOWN), -12), (Some(UP), 12)] {
                    let mut instrument = Instrument::new(orientation);
                    let mut pressed: Vec<_> = thumb.into_iter().collect();
                    pressed.push(led);
                    let shifted = u8::try_from(i16::from(note) + offset).unwrap();
                    assert_eq!(
                        update(&mut instrument, &pressed, true),
                        [on(shifted)],
                        "LED {led}, offset {offset}"
                    );
                    assert!(update(&mut instrument, &pressed, true).is_empty());
                    assert_eq!(update(&mut instrument, &[], true), [off(shifted)]);
                }
            }
        }
    }
}

#[test]
fn parallel_registers_are_immediately_adjacent_at_all_global_octaves() {
    for (thumb, offset) in [(None, 0i16), (Some(DOWN), -12), (Some(UP), 12)] {
        let mut left = BTreeSet::new();
        let mut right = BTreeSet::new();
        for led in (1..24).chain(26..50) {
            let mut instrument = Instrument::new(Orientation::Parallel);
            let mut pressed: Vec<_> = thumb.into_iter().collect();
            pressed.push(led);
            let events = update(&mut instrument, &pressed, true);
            let [MidiEvent::NoteOn { note, .. }] = events[..] else {
                panic!("expected one attack")
            };
            if led < 24 {
                left.insert(note);
            } else {
                right.insert(note);
            }
        }
        assert_eq!(i16::from(*left.first().unwrap()), 35 + offset);
        assert_eq!(i16::from(*left.last().unwrap()), 62 + offset);
        assert_eq!(i16::from(*right.first().unwrap()), 63 + offset);
        assert_eq!(i16::from(*right.last().unwrap()), 90 + offset);
        assert_eq!(left.last().unwrap() + 1, *right.first().unwrap());
    }
}

#[test]
fn matrix_spacers_are_silent_and_do_not_change_colours() {
    for orientation in ORIENTATIONS {
        let mut instrument = Instrument::new(orientation);
        let initial = instrument.frame();
        let mut state = MatrixState::new();
        for row in 0..12 {
            for col in 0..7 {
                if !POSITIONS.contains(&(row, col)) {
                    assert!(state.set(row, col, true));
                }
            }
        }
        let mut events = Vec::new();
        instrument.update(&state, true, &mut |event| events.push(event));
        assert!(events.is_empty());
        assert_eq!(instrument.frame(), initial);
    }
}

#[test]
fn held_notes_keep_their_attack_octave_while_new_notes_use_global_thumbs() {
    for orientation in ORIENTATIONS {
        let mut instrument = Instrument::new(orientation);
        let right = expected_note(orientation, 27).unwrap();
        assert_eq!(update(&mut instrument, &[1], true), [on(50)]);
        assert!(update(&mut instrument, &[1, DOWN], true).is_empty());
        assert_eq!(
            update(&mut instrument, &[1, DOWN, 27], true),
            [on(right - 12)]
        );
        assert!(update(&mut instrument, &[1, UP, 27], true).is_empty());
        assert_eq!(update(&mut instrument, &[UP, 27], true), [off(50)]);
        assert_eq!(update(&mut instrument, &[1, UP, 27], true), [on(62)]);
        let mut active = BTreeSet::from([62, right - 12]);
        apply(&mut active, &update(&mut instrument, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn both_octave_thumbs_cancel_and_accent_applies_only_to_new_attacks() {
    let mut instrument = Instrument::new(Orientation::Parallel);
    assert_eq!(update(&mut instrument, &[1, DOWN, UP], true), [on(50)]);
    assert!(update(&mut instrument, &[1, DOWN, UP, ACCENT], true).is_empty());
    assert_eq!(
        update(&mut instrument, &[1, 26, DOWN, UP, ACCENT], true),
        [MidiEvent::NoteOn {
            channel: 0,
            note: 75,
            velocity: 120
        }]
    );
    assert!(update(&mut instrument, &[1, 26], true).is_empty());
    assert_eq!(update(&mut instrument, &[1], true), [off(75)]);
    assert_eq!(update(&mut instrument, &[1, 26], true), [on(75)]);
}

#[test]
fn duplicate_keys_share_release_ownership_and_a_fresh_owner_rearticulates() {
    // Top-row D3 and bottom-row D3 on the left are distinct physical pads.
    for released_first in [1, 23] {
        let mut instrument = Instrument::new(Orientation::Mirrored);
        assert_eq!(update(&mut instrument, &[1], true), [on(50)]);
        assert_eq!(update(&mut instrument, &[1, 23], true), [off(50), on(50)]);
        let remaining = if released_first == 1 { 23 } else { 1 };
        assert!(update(&mut instrument, &[remaining], true).is_empty());
        assert_eq!(update(&mut instrument, &[], true), [off(50)]);
    }
    let mut instrument = Instrument::new(Orientation::Mirrored);
    assert_eq!(update(&mut instrument, &[1, 23], true), [on(50)]);
    assert_eq!(update(&mut instrument, &[], true), [off(50)]);
    assert_eq!(update(&mut instrument, &[1], true), [on(50)]);
    assert_eq!(update(&mut instrument, &[23], true), [off(50), on(50)]);
}

#[test]
fn damper_is_edge_toggled_and_turning_it_off_preserves_held_notes() {
    let mut instrument = Instrument::new(Orientation::Mirrored);
    assert_eq!(update(&mut instrument, &[1, DAMPER], true), [on(50)]);
    let latched = instrument.frame()[DAMPER];
    assert!(update(&mut instrument, &[DAMPER], true).is_empty());
    assert_eq!(instrument.frame()[DAMPER], latched);
    assert!(update(&mut instrument, &[], true).is_empty());
    assert_eq!(update(&mut instrument, &[2], true), [on(53)]);
    assert_eq!(update(&mut instrument, &[2, DAMPER], true), [off(50)]);
    assert_ne!(instrument.frame()[DAMPER], latched);
    assert_eq!(update(&mut instrument, &[], true), [off(53)]);
}

#[test]
fn a_retained_duplicate_is_rearticulated_and_survives_another_owners_release() {
    let mut instrument = Instrument::new(Orientation::Mirrored);
    assert_eq!(update(&mut instrument, &[1, DAMPER], true), [on(50)]);
    assert!(update(&mut instrument, &[], true).is_empty());
    assert_eq!(update(&mut instrument, &[23], true), [off(50), on(50)]);
    assert_eq!(update(&mut instrument, &[1, 23], true), [off(50), on(50)]);
    assert!(update(&mut instrument, &[23], true).is_empty());
    assert!(update(&mut instrument, &[23, DAMPER], true).is_empty());
    assert_eq!(update(&mut instrument, &[], true), [off(50)]);
}

#[test]
fn sustained_pitches_from_different_attack_octaves_do_not_replace_each_other() {
    let mut instrument = Instrument::new(Orientation::Mirrored);
    assert_eq!(update(&mut instrument, &[1, DAMPER], true), [on(50)]);
    assert!(update(&mut instrument, &[UP], true).is_empty());
    assert_eq!(update(&mut instrument, &[1, UP], true), [on(62)]);
    assert!(update(&mut instrument, &[], true).is_empty());
    let events = update(&mut instrument, &[DAMPER], true);
    let mut active = BTreeSet::from([50, 62]);
    apply(&mut active, &events);
    assert!(active.is_empty());
}

#[test]
fn sustained_chromatic_sweeps_across_both_hands_never_degrade_or_lose_ownership() {
    for orientation in ORIENTATIONS {
        let mut instrument = Instrument::new(orientation);
        let mut expected = BTreeSet::new();
        let mut active = BTreeSet::new();
        assert!(update(&mut instrument, &[DAMPER], true).is_empty());
        assert!(update(&mut instrument, &[], true).is_empty());
        let pads: Vec<_> = (1..24).chain(26..50).collect();
        for (thumb, offset) in [(Some(DOWN), -12i16), (None, 0), (Some(UP), 12)] {
            let mut pressed = pads.clone();
            pressed.extend(thumb);
            apply(&mut active, &update(&mut instrument, &pressed, true));
            for &led in &pads {
                expected.insert(
                    u8::try_from(i16::from(expected_note(orientation, led).unwrap()) + offset)
                        .unwrap(),
                );
            }
            assert_eq!(active, expected);
            apply(&mut active, &update(&mut instrument, &[], true));
            assert_eq!(active, expected);
        }
        apply(&mut active, &update(&mut instrument, &[DAMPER], true));
        assert!(active.is_empty());
        assert!(update(&mut instrument, &[], true).is_empty());
    }
}

#[test]
fn stop_clears_damper_and_requires_fresh_note_and_modifier_presses() {
    let mut instrument = Instrument::new(Orientation::Parallel);
    assert_eq!(
        update(&mut instrument, &[1, UP, DAMPER, ACCENT], true),
        [MidiEvent::NoteOn {
            channel: 0,
            note: 62,
            velocity: 120
        }]
    );
    assert!(update(&mut instrument, &[UP, ACCENT], true).is_empty());
    assert_eq!(
        update(&mut instrument, &[STOP, 2, UP, DAMPER, ACCENT], true),
        [off(62)]
    );
    assert!(update(&mut instrument, &[STOP, 1, 2, UP, DAMPER, ACCENT], true).is_empty());
    assert!(update(&mut instrument, &[1, 2, UP, DAMPER, ACCENT], true).is_empty());
    assert!(update(&mut instrument, &[2, UP, DAMPER, ACCENT], true).is_empty());
    // Blocked thumbs cannot silently restore their pre-Stop state.
    assert_eq!(
        update(&mut instrument, &[1, 2, UP, DAMPER, ACCENT], true),
        [on(50)]
    );
    assert_eq!(update(&mut instrument, &[], true), [off(50)]);
    assert_eq!(update(&mut instrument, &[1], true), [on(50)]);
}

#[test]
fn disconnect_and_recovery_require_fresh_presses_for_every_control() {
    for orientation in ORIENTATIONS {
        let mut instrument = Instrument::new(orientation);
        assert_eq!(
            update(&mut instrument, &[1, DOWN, DAMPER, ACCENT], true),
            [MidiEvent::NoteOn {
                channel: 0,
                note: 38,
                velocity: 120
            }]
        );
        assert_eq!(
            update(&mut instrument, &[1, DOWN, DAMPER, ACCENT], false),
            [off(38)]
        );
        assert!(update(&mut instrument, &[1, 2, DOWN, DAMPER, ACCENT], false).is_empty());
        assert!(update(&mut instrument, &[1, 2, DOWN, DAMPER, ACCENT], true).is_empty());
        assert!(update(&mut instrument, &[2, DOWN, DAMPER, ACCENT], true).is_empty());
        assert_eq!(
            update(&mut instrument, &[1, 2, DOWN, DAMPER, ACCENT], true),
            [on(50)]
        );
        assert_eq!(update(&mut instrument, &[], true), [off(50)]);
        assert_eq!(
            update(&mut instrument, &[1, DOWN, ACCENT], true),
            [MidiEvent::NoteOn {
                channel: 0,
                note: 38,
                velocity: 120
            }]
        );
    }
}

#[test]
fn idle_colours_are_fixed_by_pitch_class_in_both_profiles_and_all_octaves() {
    for orientation in ORIENTATIONS {
        for thumbs in [vec![], vec![DOWN], vec![UP], vec![DOWN, UP]] {
            let mut instrument = Instrument::new(orientation);
            assert!(update(&mut instrument, &thumbs, true).is_empty());
            let frame = instrument.frame();
            for (led, &rgb) in frame.iter().enumerate() {
                if let Some(note) = expected_note(orientation, led) {
                    let pc = note % 12;
                    assert_eq!(rgb, colour(pc, if pc == 0 { 24 } else { 16 }, false));
                }
            }
            let stop = frame[STOP];
            assert!(
                stop.r > stop.g && stop.r > stop.b,
                "Stop must stay visibly red"
            );
        }
    }
}

#[test]
fn every_pair_of_distinct_pitch_classes_shows_only_major_minor_completions() {
    for first in 0..12 {
        for second in first + 1..12 {
            let mut hints = BTreeSet::new();
            for root in 0..12 {
                for third in [3, 4] {
                    let triad = [root, (root + third) % 12, (root + 7) % 12];
                    if triad.contains(&first) && triad.contains(&second) {
                        hints.extend(triad.into_iter().filter(|pc| *pc != first && *pc != second));
                    }
                }
            }
            for orientation in ORIENTATIONS {
                assert_hints(
                    orientation,
                    &[pad_for_pc(first), pad_for_pc(second)],
                    &hints,
                );
            }
        }
    }
}

#[test]
fn all_twenty_four_triads_show_exactly_the_new_parallel_relative_and_leading_tone_pitches() {
    for root in 0..12 {
        for third in [3, 4] {
            let held = [root, (root + third) % 12, (root + 7) % 12].map(pad_for_pc);
            let new_intervals = if third == 4 { [3, 9, 11] } else { [4, 8, 10] };
            let hints = new_intervals
                .map(|interval| (root + interval) % 12)
                .into_iter()
                .collect();
            for orientation in ORIENTATIONS {
                assert_hints(orientation, &held, &hints);
            }
        }
    }
}

#[test]
fn other_shapes_have_no_harmonic_interpretation_and_duplicate_pitches_count_once() {
    for orientation in ORIENTATIONS {
        for pcs in [
            vec![],
            vec![0],
            vec![0, 4, 8],
            vec![0, 3, 6],
            vec![0, 4, 7, 11],
        ] {
            let pads: Vec<_> = pcs.into_iter().map(pad_for_pc).collect();
            assert_hints(orientation, &pads, &BTreeSet::new());
        }
        // The two D3 pads contribute one class, leaving a D/F pair. Its
        // completions are B-flat major and D minor (B-flat and A).
        assert_hints(orientation, &[1, 23, 2], &BTreeSet::from([9, 10]));
    }
}

#[test]
fn held_and_retained_notes_have_distinct_brightness_and_only_held_notes_guide_hints() {
    let mut instrument = Instrument::new(Orientation::Mirrored);
    let c = pad_for_pc(0);
    let e = pad_for_pc(4);
    let b = pad_for_pc(11);
    let b_note = expected_note(Orientation::Mirrored, b).unwrap();
    assert_eq!(update(&mut instrument, &[b, DAMPER], true), [on(b_note)]);
    assert_eq!(instrument.frame()[b], colour(11, 144, true));
    assert!(update(&mut instrument, &[], true).is_empty());
    assert_eq!(instrument.frame()[b], colour(11, 80, false));
    let events = update(&mut instrument, &[c, e], true);
    assert_eq!(events.len(), 2);
    let frame = instrument.frame();
    assert_eq!(frame[c], colour(0, 144, true));
    assert_eq!(frame[e], colour(4, 144, true));
    // C/E has G and A completions; the retained B must not turn it into
    // an interpreted Cmaj7 or cancel those completion hints.
    for pc in [7, 9] {
        let pad = pad_for_pc(pc);
        assert_eq!(frame[pad], colour(pc, 48, false));
    }
}

#[test]
fn thumb_leds_track_the_accepted_controls_and_stop_stays_red() {
    let mut instrument = Instrument::new(Orientation::Mirrored);
    let idle = instrument.frame();
    assert!(update(&mut instrument, &[DOWN, UP, DAMPER, ACCENT], true).is_empty());
    let active = instrument.frame();
    for thumb in [DOWN, UP, DAMPER, ACCENT] {
        assert_ne!(active[thumb], idle[thumb]);
    }
    assert!(update(&mut instrument, &[], true).is_empty());
    let latched = instrument.frame();
    for thumb in [DOWN, UP, ACCENT] {
        assert_eq!(latched[thumb], idle[thumb]);
    }
    assert_eq!(latched[DAMPER], active[DAMPER]);
    assert!(update(&mut instrument, &[STOP, DOWN, UP, DAMPER, ACCENT], true).is_empty());
    let stopped = instrument.frame();
    for thumb in [DOWN, UP, DAMPER, ACCENT] {
        assert_eq!(stopped[thumb], idle[thumb]);
    }
    assert!(stopped[STOP].r > stopped[STOP].g && stopped[STOP].r > stopped[STOP].b);
}

#[test]
fn scan_guard_requires_a_success_and_expires_at_exactly_one_hundred_milliseconds() {
    let mut guard = scan_guard::ScanGuard::new();
    assert!(!guard.is_fresh(0));
    assert!(!guard.is_fresh(100));
    guard.succeeded(10);
    assert!(guard.is_fresh(10));
    assert!(guard.is_fresh(109));
    assert!(!guard.is_fresh(110));
    guard.succeeded(109);
    assert!(guard.is_fresh(110));
    assert!(guard.is_fresh(208));
    assert!(!guard.is_fresh(209));
}

#[test]
fn delayed_scan_samples_keep_their_completion_time_instead_of_renewing_at_delivery() {
    let mut guard = scan_guard::ScanGuard::new();
    guard.succeeded(100);
    assert!(guard.is_fresh(199));
    assert!(!guard.is_fresh(200));
    // The queue may deliver this successful scan after a slow USB write.
    // Processing it at 400 ms cannot treat it as a scan completed at 400.
    guard.succeeded(150);
    assert!(!guard.is_fresh(400));
    guard.succeeded(350);
    assert!(guard.is_fresh(400));
    assert!(!guard.is_fresh(450));
}

#[test]
fn scan_timeout_flushes_the_instrument_and_its_first_recovery_snapshot_is_silent() {
    let mut guard = scan_guard::ScanGuard::new();
    let mut instrument = Instrument::new(Orientation::Parallel);
    let initial_fresh = guard.is_fresh(0);
    guard.succeeded(0);
    let first_recovered = guard.is_fresh(0) && !initial_fresh;
    assert!(first_recovered);
    assert!(update(&mut instrument, &[1], !first_recovered).is_empty());
    assert!(update(&mut instrument, &[1], guard.is_fresh(1)).is_empty());
    assert!(update(&mut instrument, &[], guard.is_fresh(2)).is_empty());
    assert_eq!(update(&mut instrument, &[1], guard.is_fresh(3)), [on(50)]);
    assert!(update(&mut instrument, &[1], guard.is_fresh(99)).is_empty());
    assert_eq!(
        update(&mut instrument, &[1], guard.is_fresh(100)),
        [off(50)]
    );
    let was_fresh = guard.is_fresh(150);
    guard.succeeded(150);
    let recovered = guard.is_fresh(150) && !was_fresh;
    assert!(recovered);
    assert!(update(&mut instrument, &[1, UP, DAMPER, ACCENT], !recovered).is_empty());
    assert!(update(
        &mut instrument,
        &[1, UP, DAMPER, ACCENT],
        guard.is_fresh(151)
    )
    .is_empty());
    assert!(update(&mut instrument, &[UP, DAMPER, ACCENT], guard.is_fresh(152)).is_empty());
    // Held-through-recovery modifiers stay unarmed until their own fresh press.
    assert_eq!(
        update(
            &mut instrument,
            &[1, UP, DAMPER, ACCENT],
            guard.is_fresh(153)
        ),
        [on(50)]
    );
    assert_eq!(update(&mut instrument, &[], guard.is_fresh(154)), [off(50)]);
}

#[test]
fn both_instruments_keep_exact_ownership_through_bounded_midi_batches_and_wire_packets() {
    use tecla::devices::midi::output;

    let send = |instrument: &mut Instrument, pressed: &[usize], active: &mut BTreeSet<u8>| {
        let mut batch: heapless::Vec<MidiEvent, { instrument::MAX_EVENTS }> = heapless::Vec::new();
        instrument.update(&matrix(pressed), true, &mut |event| {
            batch.push(event).unwrap();
        });
        let mut attacked = false;
        let mut released = BTreeSet::new();
        let mut packets = 0;
        for chunk in batch.chunks(output::PACKET_EVENTS) {
            let mut packet = [0; output::PACKET_BYTES];
            let len = output::pack_events(chunk, &mut packet).unwrap();
            assert!(len > 0 && len <= 60);
            assert_eq!(len % 4, 0);
            packets += 1;
            // Decode actual USB bytes independently of MidiEvent::to_usb_packet.
            // The order assertion spans packet boundaries as well as each chunk.
            for bytes in packet[..len].as_chunks::<4>().0 {
                let [cin, status, note, velocity] = bytes;
                assert!(*note <= 127);
                match (*cin, *status) {
                    (0x09, 0x90) => {
                        assert!([88, 120].contains(velocity));
                        assert!(
                            active.insert(*note),
                            "wire attack without release: {bytes:?}"
                        );
                        attacked = true;
                    }
                    (0x08, 0x80) => {
                        assert_eq!(*velocity, 0);
                        assert!(
                            !attacked,
                            "wire release followed an attack in the same scan"
                        );
                        assert!(
                            active.remove(note),
                            "wire release without ownership: {bytes:?}"
                        );
                        released.insert(*note);
                    }
                    _ => panic!("unexpected cable, CIN, channel or status: {bytes:?}"),
                }
            }
        }
        (batch.len(), packets, released)
    };

    for orientation in ORIENTATIONS {
        let mut instrument = Instrument::new(orientation);
        let mut active = BTreeSet::new();
        let mut retained = BTreeSet::new();
        let pads: Vec<_> = (1..24).chain(26..50).collect();
        assert_eq!(pads.len(), 47);
        send(&mut instrument, &[DAMPER], &mut active);
        send(&mut instrument, &[], &mut active);
        for (thumb, offset) in [(Some(DOWN), -12i16), (None, 0), (Some(UP), 12)] {
            let mut pressed = pads.clone();
            pressed.extend(thumb);
            let (_, packets, _) = send(&mut instrument, &pressed, &mut active);
            assert!(packets >= 2, "full chords must exercise packet boundaries");
            for &led in &pads {
                retained.insert(
                    u8::try_from(i16::from(expected_note(orientation, led).unwrap()) + offset)
                        .unwrap(),
                );
            }
            assert_eq!(active, retained);
            // Keep two literal notes from the final high-octave attacks held.
            let remaining: &[usize] = if offset == 12 { &[5, 26] } else { &[] };
            send(&mut instrument, remaining, &mut active);
            assert_eq!(active, retained);
        }

        let anchors = [5, 26].map(|led| expected_note(orientation, led).unwrap() + 12);
        let mut expected: BTreeSet<_> = anchors.into_iter().collect();
        let mut pressed = vec![5, 26, DOWN, DAMPER, ACCENT];
        for &led in &pads {
            let low = expected_note(orientation, led).unwrap() - 12;
            // Exclude new owners of either anchor pitch so a valid shared-pitch
            // rearticulation cannot obscure the held-note preservation check.
            if ![5, 26].contains(&led) && !anchors.contains(&low) {
                pressed.push(led);
                expected.insert(low);
            }
        }
        let (events, packets, released) = send(&mut instrument, &pressed, &mut active);
        assert!(
            events > 30 && packets >= 3,
            "damper-off mixed burst must span several packets"
        );
        assert!(
            anchors.iter().all(|note| !released.contains(note)),
            "damper-off released a held literal anchor"
        );
        assert_eq!(
            active, expected,
            "wire notes must match held captured pitches exactly"
        );
        send(&mut instrument, &[5, 26], &mut active);
        assert_eq!(active, anchors.into_iter().collect());
        send(&mut instrument, &[STOP, 5, 26], &mut active);
        assert!(active.is_empty());
        send(&mut instrument, &[5, 26], &mut active);
        assert!(
            active.is_empty(),
            "Stop must not resurrect held notes on the wire"
        );
    }
}

#[test]
fn every_held_note_lights_exact_duplicates_and_octave_equivalents_in_both_maps() {
    for orientation in ORIENTATIONS {
        for (thumb, offset) in [(None, 0i16), (Some(DOWN), -12), (Some(UP), 12)] {
            for origin in (1..24).chain(26..50) {
                let mut instrument = Instrument::new(orientation);
                let mut pressed: Vec<_> = thumb.into_iter().collect();
                pressed.push(origin);
                let captured =
                    u8::try_from(i16::from(expected_note(orientation, origin).unwrap()) + offset)
                        .unwrap();
                assert_eq!(update(&mut instrument, &pressed, true), [on(captured)]);
                let frame = instrument.frame();
                for (led, rgb) in frame.into_iter().enumerate() {
                    if let Some(base) = expected_note(orientation, led) {
                        let prospective = u8::try_from(i16::from(base) + offset).unwrap();
                        let value = if led == origin {
                            144
                        } else if prospective == captured {
                            112
                        } else if prospective % 12 == captured % 12 {
                            96
                        } else if prospective % 12 == 0 {
                            24
                        } else {
                            16
                        };
                        assert_eq!(
                            rgb,
                            colour(prospective % 12, value, led == origin),
                            "{orientation:?}, origin {origin}, LED {led}, offset {offset}"
                        );
                    }
                }
                assert!(update(&mut instrument, &pressed, true).is_empty());
                assert_eq!(instrument.frame(), frame);
            }
        }
    }
}

#[test]
fn equivalent_note_lights_use_captured_pitch_after_octave_changes() {
    for orientation in ORIENTATIONS {
        for (attack_thumb, attack_offset) in [(None, 0i16), (Some(DOWN), -12), (Some(UP), 12)] {
            for (current_thumb, current_offset) in [(None, 0i16), (Some(DOWN), -12), (Some(UP), 12)]
            {
                let mut instrument = Instrument::new(orientation);
                let mut attacked: Vec<_> = attack_thumb.into_iter().collect();
                attacked.push(1);
                let captured = u8::try_from(50 + attack_offset).unwrap();
                assert_eq!(update(&mut instrument, &attacked, true), [on(captured)]);
                let mut current: Vec<_> = current_thumb.into_iter().collect();
                current.push(1);
                assert!(update(&mut instrument, &current, true).is_empty());
                let frame = instrument.frame();
                for (led, rgb) in frame.into_iter().enumerate() {
                    if let Some(base) = expected_note(orientation, led) {
                        let prospective = u8::try_from(i16::from(base) + current_offset).unwrap();
                        let value = if led == 1 {
                            144
                        } else if prospective == captured {
                            112
                        } else if prospective % 12 == captured % 12 {
                            96
                        } else if prospective % 12 == 0 {
                            24
                        } else {
                            16
                        };
                        assert_eq!(
                            rgb,
                            colour(prospective % 12, value, led == 1),
                            "{orientation:?}, LED {led}, captured {captured}, prospective {prospective}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn damper_pitch_classes_light_the_whole_lattice_below_held_equivalents() {
    for orientation in ORIENTATIONS {
        for (attack_thumb, attack_offset) in [(None, 0i16), (Some(DOWN), -12), (Some(UP), 12)] {
            for (current_thumb, current_offset) in [(None, 0i16), (Some(DOWN), -12), (Some(UP), 12)]
            {
                let mut instrument = Instrument::new(orientation);
                let mut attacked: Vec<_> = attack_thumb.into_iter().collect();
                attacked.extend([1, DAMPER]);
                let captured = u8::try_from(50 + attack_offset).unwrap();
                assert_eq!(update(&mut instrument, &attacked, true), [on(captured)]);
                let current: Vec<_> = current_thumb.into_iter().collect();
                assert!(update(&mut instrument, &current, true).is_empty());
                let frame = instrument.frame();
                for (led, rgb) in frame.into_iter().enumerate() {
                    if let Some(base) = expected_note(orientation, led) {
                        let prospective = u8::try_from(i16::from(base) + current_offset).unwrap();
                        let value = if led == 1 {
                            80
                        } else if prospective % 12 == captured % 12 {
                            64
                        } else if prospective % 12 == 0 {
                            24
                        } else {
                            16
                        };
                        assert_eq!(
                            rgb,
                            colour(prospective % 12, value, false),
                            "{orientation:?}, LED {led}"
                        );
                    }
                }
                let mut damper_off = current;
                damper_off.push(DAMPER);
                assert_eq!(update(&mut instrument, &damper_off, true), [off(captured)]);
                for (led, rgb) in instrument.frame().into_iter().enumerate() {
                    if let Some(note) = expected_note(orientation, led) {
                        assert_eq!(
                            rgb,
                            colour(note % 12, if note % 12 == 0 { 24 } else { 16 }, false)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn retained_origins_keep_their_priority_over_matching_held_notes() {
    let mut instrument = Instrument::new(Orientation::Mirrored);
    update(&mut instrument, &[1, DAMPER], true);
    update(&mut instrument, &[], true);
    update(&mut instrument, &[23], true);
    let frame = instrument.frame();
    assert_eq!(frame[1], colour(2, 80, false));
    assert_eq!(frame[23], colour(2, 144, true));
    assert_eq!(frame[30], colour(2, 96, false));
}

#[test]
fn stop_and_recovery_clear_equivalent_and_sustained_note_highlights() {
    for orientation in ORIENTATIONS {
        for stop in [true, false] {
            let mut instrument = Instrument::new(orientation);
            let idle = instrument.frame();
            update(&mut instrument, &[1, DAMPER], true);
            update(&mut instrument, &[], true);
            update(&mut instrument, &[2], true);
            let pressed = if stop { vec![STOP, 2] } else { vec![2] };
            update(&mut instrument, &pressed, stop);
            let cleared = instrument.frame();
            for led in (1..24).chain(26..50) {
                assert_eq!(cleared[led], idle[led]);
            }
            update(&mut instrument, &[2], true);
            for led in (1..24).chain(26..50) {
                assert_eq!(instrument.frame()[led], idle[led]);
            }
            update(&mut instrument, &[], true);
            assert_eq!(update(&mut instrument, &[2], true), [on(53)]);
            assert_eq!(instrument.frame()[2], colour(5, 144, true));
        }
    }
}

#[test]
fn shared_palette_keeps_pitch_hue_and_published_state_strength_across_all_octaves() {
    use instrument::colours::{self, NoteState};
    const FIFTHS: [u16; 12] = [0, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];
    for pitch in 0..=127 {
        let pc = usize::from(pitch % 12);
        let hue = u8::try_from((32 + 256 * FIFTHS[pc] / 12) % 256).unwrap();
        for (state, saturation, value) in [
            (NoteState::Held, 120, 144),
            (NoteState::Retained, 210, 80),
            (NoteState::ExactHeld, 160, 112),
            (NoteState::OctaveHeld, 210, 96),
            (NoteState::GeneratedExact, 180, 88),
            (NoteState::GeneratedOctave, 210, 64),
            (NoteState::Sustained, 210, 64),
            (NoteState::Hint, 210, 48),
        ] {
            assert_eq!(
                colours::note(pitch, state),
                hsv_to_rgb(Hsv::new(hue, saturation, value)),
                "pitch {pitch}, {state:?}"
            );
        }
        assert_eq!(colours::note(pitch, NoteState::Idle), piano_idle(pitch));
    }
    for hue in 0..=255 {
        assert_eq!(
            colours::control(hue, false),
            hsv_to_rgb(Hsv::new(hue, 210, 16))
        );
        assert_eq!(
            colours::control(hue, true),
            hsv_to_rgb(Hsv::new(hue, 120, 144))
        );
    }
    assert_eq!(colours::stop(false), Rgb::new(32, 0, 0));
    assert_eq!(colours::stop(true), Rgb::new(144, 0, 0));
}

#[test]
fn idle_piano_classification_covers_every_midi_note_and_keeps_c_references_brighter() {
    use instrument::colours::{self, NoteState};
    const NATURALS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
    for pitch in 0..=127 {
        let rgb = colours::note(pitch, NoteState::Idle);
        assert_eq!(rgb, piano_idle(pitch));
        if NATURALS.contains(&(pitch % 12)) {
            assert_eq!(rgb.r, rgb.g);
            assert_eq!(rgb.g, rgb.b);
            assert_eq!(rgb.r, if pitch % 12 == 0 { 40 } else { 24 });
        } else {
            assert_eq!(rgb, Rgb::BLACK, "accidental MIDI note {pitch}");
        }
    }
    for pc in [1, 3, 6, 8, 10] {
        for octave in 0..=10 {
            let pitch = pc + octave * 12;
            if pitch > 127 {
                continue;
            }
            for state in [
                NoteState::Held,
                NoteState::Retained,
                NoteState::ExactHeld,
                NoteState::OctaveHeld,
                NoteState::GeneratedExact,
                NoteState::GeneratedOctave,
                NoteState::Sustained,
                NoteState::Hint,
            ] {
                let rgb = colours::note(pitch, state);
                assert_ne!(rgb, Rgb::BLACK, "{pitch}, {state:?}");
                assert!(
                    rgb.r != rgb.g || rgb.g != rgb.b,
                    "active accidental keeps its pitch colour"
                );
            }
        }
    }
}

#[test]
fn black_idle_keys_still_show_accidental_attacks_duplicates_and_harmonic_hints() {
    for orientation in ORIENTATIONS {
        let mut instrument = Instrument::new(orientation);
        assert_eq!(
            instrument.frame()[3],
            Rgb::BLACK,
            "A-flat is an idle black key"
        );
        if orientation == Orientation::Mirrored {
            assert_eq!(update(&mut instrument, &[3], true), [on(56)]);
            assert_eq!(
                instrument.frame()[46],
                colour(8, 112, false),
                "exact A-flat3 duplicate"
            );
        } else {
            assert_eq!(update(&mut instrument, &[3, UP], true), [on(68)]);
            assert!(update(&mut instrument, &[3, DOWN], true).is_empty());
            assert_eq!(
                instrument.frame()[35],
                colour(8, 112, false),
                "right A-flat5 down an octave matches held A-flat4"
            );
        }
        assert_eq!(instrument.frame()[3], colour(8, 144, true));
        assert_eq!(
            instrument.frame()[21],
            colour(8, 96, false),
            "octave-equivalent A-flat remains coloured"
        );
        update(&mut instrument, &[], true);
        assert_eq!(instrument.frame()[3], Rgb::BLACK);

        let b_and_d = [pad_for_pc(11), pad_for_pc(2)];
        update(&mut instrument, &b_and_d, true);
        let f_sharp = pad_for_pc(6);
        assert_eq!(
            instrument.frame()[f_sharp],
            colour(6, 48, false),
            "B/D suggests F-sharp to complete B minor"
        );
        update(&mut instrument, &[], true);
        assert_eq!(instrument.frame()[f_sharp], Rgb::BLACK);
    }
}
