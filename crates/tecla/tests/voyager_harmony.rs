#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]

#[path = "../../../keyboards/tecla/voyager_harmony/instrument.rs"]
mod harmony_instrument;
#[allow(
    dead_code,
    reason = "the new profile reuses the physical map without the manual instrument"
)]
#[path = "../../../keyboards/tecla/voyager_tonnetz/instrument.rs"]
mod instrument;
#[path = "../../../keyboards/tecla/voyager_harmony/telemetry.rs"]
mod telemetry;

use std::collections::BTreeSet;

use harmony_instrument::{control, Control, HarmonyInstrument, ARRIVE, RESTRIKE};
use tecla::matrix::MatrixState;
use tecla::music::realization::{Colour, Direction, Foundation, Motion, Reharm};
use tecla::pipeline::report::MidiEvent;
use tecla::rgb::{hsv_to_rgb, Hsv, Rgb};

// Independently transcribed physical order, including N at electrical (10,2).
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
const C_MAJOR: [usize; 3] = [15, 9, 10]; // C3 E3 G3

fn matrix(leds: &[usize]) -> MatrixState<12, 7> {
    let mut matrix = MatrixState::new();
    for &led in leds {
        let (row, col) = POSITIONS[led];
        assert!(matrix.set(row, col, true));
    }
    matrix
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

struct Player {
    instrument: HarmonyInstrument,
    active: BTreeSet<u8>,
    now: u64,
}

impl Player {
    fn new() -> Self {
        Self {
            instrument: HarmonyInstrument::new(),
            active: BTreeSet::new(),
            now: 0,
        }
    }

    fn play(&mut self, leds: &[usize]) -> Vec<MidiEvent> {
        self.scan(leds, true)
    }

    fn scan(&mut self, leds: &[usize], ready: bool) -> Vec<MidiEvent> {
        self.now += 7;
        let mut events = Vec::new();
        self.instrument
            .update_at(&matrix(leds), self.now, ready, &mut |event| {
                events.push(event)
            });
        assert!(events.len() <= 128);
        let mut attacked = false;
        for &event in &events {
            match event {
                MidiEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                } => {
                    assert_eq!((channel, velocity), (0, 88));
                    assert!(
                        self.active.insert(note),
                        "duplicate attack without release: {events:?}"
                    );
                    attacked = true;
                }
                MidiEvent::NoteOff { channel, note } => {
                    assert_eq!(channel, 0);
                    assert!(!attacked, "releases must precede all attacks: {events:?}");
                    assert!(
                        self.active.remove(&note),
                        "release without owner: {events:?}"
                    );
                }
                _ => panic!("unexpected MIDI event {event:?}"),
            }
        }
        events
    }

    fn assert_manual(&self, leds: &[usize]) {
        for &led in leds {
            assert!(
                self.active.contains(&LEFT[led]),
                "literal note {} disappeared",
                LEFT[led]
            );
        }
    }
}

fn with(base: &[usize], extra: &[usize]) -> Vec<usize> {
    base.iter().chain(extra).copied().collect()
}

fn pitch_colour(note: u8, saturation: u8, value: u8) -> Rgb {
    const FIFTHS: [u16; 12] = [0, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];
    let hue = ((32 + 256 * FIFTHS[usize::from(note % 12)] / 12) % 256) as u8;
    hsv_to_rgb(Hsv::new(hue, saturation, value))
}

fn diagnostic_status() -> instrument::diagnostics::DiagnosticStatus {
    instrument::diagnostics::DiagnosticStatus {
        now_ms: u64::MAX,
        ready: true,
        right_age_ms: u64::MAX,
        right_ok: true,
        usb_session: u32::MAX,
        midi_ready: true,
        recovered: false,
        tx_fault: false,
        dropped: u32::MAX,
        update_us: u32::MAX,
    }
}

#[test]
fn every_physical_key_has_the_documented_musical_role() {
    assert_eq!(POSITIONS, instrument::POSITIONS);
    assert_eq!(POSITIONS.into_iter().collect::<BTreeSet<_>>().len(), 52);
    let expected = [
        Control::Prepare,
        Control::Colour(Colour::Extended),
        Control::Foundation(Foundation::Seventh),
        Control::Motion(Motion::Outward),
        Control::Reharm(Reharm::AppliedDominant),
        Control::Lean,
        Control::Delay,
        Control::Colour(Colour::Seventh),
        Control::Foundation(Foundation::Root),
        Control::Motion(Motion::Inward),
        Control::Reharm(Reharm::Borrowed),
        Control::Resolve,
        Control::Continue,
        Control::Colour(Colour::Sixth),
        Control::Foundation(Foundation::Fifth),
        Control::Motion(Motion::Rising),
        Control::Reharm(Reharm::CommonTone),
        Control::Substitute,
        Control::Pedal,
        Control::Colour(Colour::SecondsFourths),
        Control::Foundation(Foundation::Third),
        Control::Motion(Motion::Falling),
        Control::Reharm(Reharm::Relative),
        Control::Deceive,
    ];
    for (led, _) in POSITIONS.iter().enumerate() {
        assert_eq!(
            control(led),
            led.checked_sub(26)
                .and_then(|local| expected.get(local).copied())
        );
        let mut player = Player::new();
        let events = player.play(&[led]);
        if (1..24).contains(&led) {
            assert!(events.contains(&on(LEFT[led])), "LED {led}");
            assert_eq!(
                player.instrument.frame()[led],
                pitch_colour(LEFT[led], 120, 144)
            );
        } else {
            assert!(
                events.is_empty(),
                "control {led} generated a chord without left-hand material"
            );
        }
        player.play(&[]);
        assert!(player.active.is_empty());
    }
    assert_eq!((RESTRIKE, ARRIVE), (50, 51));
}

#[test]
fn foundation_changes_the_role_of_the_literal_lowest_note_and_the_chord_root() {
    for (led, role, root) in [
        (34, Foundation::Root, 0),
        (46, Foundation::Third, 9),
        (40, Foundation::Fifth, 5),
        (28, Foundation::Seventh, 2),
    ] {
        let mut player = Player::new();
        player.play(&C_MAJOR);
        let events = player.play(&with(&C_MAJOR, &[led]));
        let snapshot = player.instrument.snapshot();
        assert_eq!(snapshot.requested.foundation, role);
        let chord = snapshot.diagnostics.current.chord.unwrap();
        assert_eq!(chord.root, root, "C E G with {role:?}: {chord:?}");
        assert_eq!(
            player.active.first(),
            Some(&36),
            "{role:?} must keep C as the audible bass of its new chord"
        );
        match role {
            Foundation::Root => assert_eq!(chord.root, 0),
            Foundation::Third => assert_eq!(chord.third(), 0),
            Foundation::Fifth => assert_eq!(chord.fifth(), 0),
            Foundation::Seventh => assert_eq!(chord.seventh(), Some(0)),
            Foundation::Natural => unreachable!(),
        }
        player.assert_manual(&C_MAJOR);
        for note in [48, 52, 55] {
            assert!(!events.contains(&off(note)));
            assert!(!events.contains(&on(note)));
        }
        assert_eq!(
            snapshot.diagnostics.limited & 2,
            0,
            "feasible foundation was marked unavailable"
        );
    }
}

#[test]
fn colour_keys_supply_the_named_tone_classes_under_a_fixed_c_root() {
    for (led, colour, intervals) in [
        (45, Colour::SecondsFourths, &[2, 5][..]),
        (39, Colour::Sixth, &[9][..]),
        (33, Colour::Seventh, &[10, 11][..]),
        (27, Colour::Extended, &[1, 2, 3, 8, 9][..]),
    ] {
        let mut player = Player::new();
        player.play(&C_MAJOR);
        let events = player.play(&with(&C_MAJOR, &[34, led]));
        let snapshot = player.instrument.snapshot();
        assert_eq!(snapshot.requested.colour, colour);
        assert_eq!(snapshot.diagnostics.current.chord.unwrap().root, 0);
        assert!(
            snapshot
                .support
                .iter()
                .flatten()
                .any(|note| intervals.contains(&(note % 12))),
            "{colour:?} did not add its named tone: {:?}",
            snapshot.support
        );
        assert_eq!(
            snapshot.diagnostics.limited & 1,
            0,
            "feasible colour was marked unavailable"
        );
        player.assert_manual(&C_MAJOR);
        for note in [48, 52, 55] {
            assert!(!events.contains(&off(note)));
            assert!(!events.contains(&on(note)));
        }
    }
}

#[test]
fn reharm_keys_make_the_named_relationship_audible_beneath_literal_c_major() {
    for (led, relationship) in [
        (48, Reharm::Relative),
        (42, Reharm::CommonTone),
        (36, Reharm::Borrowed),
        (30, Reharm::AppliedDominant),
    ] {
        let mut player = Player::new();
        player.play(&C_MAJOR);
        let reference = player
            .instrument
            .snapshot()
            .diagnostics
            .current
            .chord
            .unwrap();
        assert_eq!(reference.root, 0);
        player.play(&with(&C_MAJOR, &[led]));
        let snapshot = player.instrument.snapshot();
        let chord = snapshot.diagnostics.current.chord.unwrap();
        assert_eq!(snapshot.requested.reharm, relationship);
        assert_eq!(snapshot.diagnostics.limited & 8, 0);
        match relationship {
            Reharm::Relative => {
                assert_eq!(chord.root, 9);
                assert_eq!(chord.third(), 0);
            }
            Reharm::CommonTone => {
                assert_ne!(chord.root, reference.root);
                assert!((chord.tones().bits() & reference.tones().bits()).count_ones() >= 2);
            }
            Reharm::Borrowed => {
                assert_ne!(chord.tones().bits() & !reference.tones().bits(), 0);
                assert!(matches!(chord.root, 0 | 5 | 8 | 10));
                if chord.root == 0 {
                    assert_eq!(chord.third(), 3);
                }
            }
            Reharm::AppliedDominant => {
                assert_eq!(chord.root, 7);
                assert_eq!(chord.third(), 11);
                assert_eq!(chord.seventh(), Some(5));
                assert_eq!(snapshot.diagnostics.current.destination.unwrap().root, 0);
            }
            Reharm::Faithful => unreachable!(),
        }
        for class in 0..12 {
            if chord.essential().contains(class) {
                assert!(
                    player.active.iter().any(|note| note % 12 == class),
                    "{relationship:?} declared an inaudible structural tone {class}"
                );
            }
        }
        player.assert_manual(&C_MAJOR);
    }
}

#[test]
fn directional_motion_moves_an_actual_upper_voice_in_the_requested_direction() {
    for (led, motion, rising) in [(41, Motion::Rising, true), (47, Motion::Falling, false)] {
        let mut player = Player::new();
        player.play(&C_MAJOR);
        let before = player.instrument.snapshot().support;
        player.play(&with(&C_MAJOR, &[led]));
        let after = player.instrument.snapshot();
        assert_eq!(after.requested.motion, motion);
        assert!(
            before
                .iter()
                .zip(after.support)
                .skip(1)
                .any(|(old, next)| match (*old, next) {
                    (Some(old), Some(next)) =>
                        if rising {
                            next > old
                        } else {
                            next < old
                        },
                    _ => false,
                }),
            "{motion:?} made no directional movement: {before:?} -> {:?}",
            after.support
        );
        player.assert_manual(&C_MAJOR);
        assert_eq!(after.diagnostics.limited & 4, 0);
    }
}

#[test]
fn fresh_choices_override_older_held_choices_and_release_restores_the_previous_request() {
    for (older, newer, family) in [(27, 45, 0), (34, 46, 1), (29, 47, 2), (30, 48, 3)] {
        let mut player = Player::new();
        player.play(&with(&C_MAJOR, &[older]));
        let before = player.instrument.snapshot().requested;
        player.play(&with(&C_MAJOR, &[older, newer]));
        let chosen = player.instrument.snapshot().requested;
        match family {
            0 => assert_eq!(chosen.colour, Colour::SecondsFourths),
            1 => assert_eq!(chosen.foundation, Foundation::Third),
            2 => assert_eq!(chosen.motion, Motion::Falling),
            _ => assert_eq!(chosen.reharm, Reharm::Relative),
        }
        assert!(player.play(&with(&C_MAJOR, &[older])).is_empty());
        assert_eq!(player.instrument.snapshot().requested, before);
        assert_eq!(player.instrument.snapshot().diagnostics.applied, chosen);
    }
}

#[test]
fn column_requests_combine_independently_without_a_hidden_strength_level() {
    let mut player = Player::new();
    player.play(&with(&C_MAJOR, &[45, 46, 41, 48]));
    let requested = player.instrument.snapshot().requested;
    assert_eq!(requested.colour, Colour::SecondsFourths);
    assert_eq!(requested.foundation, Foundation::Third);
    assert_eq!(requested.motion, Motion::Rising);
    assert_eq!(requested.reharm, Reharm::Relative);
    player.assert_manual(&C_MAJOR);
    player.play(&with(&C_MAJOR, &[45, 46, 47, 48]));
    let changed = player.instrument.snapshot().requested;
    assert_eq!(changed.colour, requested.colour);
    assert_eq!(changed.foundation, requested.foundation);
    assert_eq!(changed.reharm, requested.reharm);
    assert_eq!(changed.motion, Motion::Falling);
}

#[test]
fn phrase_actions_are_edges_and_do_not_override_later_column_taps() {
    for action in [26, 31, 37, 38, 43, 49, ARRIVE] {
        let mut player = Player::new();
        player.play(&C_MAJOR);
        player.play(&with(&C_MAJOR, &[action]));
        let events = player.play(&with(&C_MAJOR, &[action, 39]));
        let snapshot = player.instrument.snapshot();
        assert_eq!(
            snapshot.requested.direction,
            Direction::Continue,
            "held action {action}"
        );
        assert_eq!(snapshot.diagnostics.applied.colour, Colour::Sixth);
        assert!(events
            .iter()
            .any(|event| matches!(event, MidiEvent::NoteOn { .. })));
        player.assert_manual(&C_MAJOR);
    }
}

#[test]
fn a_pedal_preserves_the_generated_bass_while_the_harmony_moves_then_releases_it() {
    let mut player = Player::new();
    player.play(&C_MAJOR);
    let pedal = player.instrument.snapshot().support[0].unwrap();
    player.play(&with(&C_MAJOR, &[44]));
    assert!(player.instrument.snapshot().requested.pedal);
    let events = player.play(&[20, 14, 15, 44]); // F2 A2 C3 over retained C bass.
    assert_eq!(player.instrument.snapshot().support[0], Some(pedal));
    assert!(!events.contains(&off(pedal)));
    player.assert_manual(&[20, 14, 15]);
    let released = player.play(&[20, 14, 15]);
    assert!(!player.instrument.snapshot().requested.pedal);
    assert_ne!(player.instrument.snapshot().support[0], Some(pedal));
    assert!(released.contains(&off(pedal)));
    player.assert_manual(&[20, 14, 15]);
}

#[test]
fn a_continuous_pedal_survives_a_breath_but_releasing_it_without_left_notes_clears_it() {
    for continuous in [true, false] {
        let mut player = Player::new();
        player.play(&C_MAJOR);
        let previous_bass = player.instrument.snapshot().support[0].unwrap();
        assert_eq!(previous_bass, 36); // C2.
        player.play(&with(&C_MAJOR, &[44]));
        player.play(&[44]);
        assert!(player.active.is_empty());
        if !continuous {
            player.play(&[]);
            player.play(&[44]);
            assert!(player.active.is_empty());
        }
        player.play(&[20, 14, 15, 44]); // F2 A2 C3, after the breath.
        assert_eq!(
            player.instrument.snapshot().support[0],
            Some(if continuous { 36 } else { 29 })
        );
        player.assert_manual(&[20, 14, 15]);
        player.play(&[]);
        assert!(player.active.is_empty());
    }
}

#[test]
fn deceptive_arrival_resolves_a_dominant_to_the_relative_minor_with_literal_notes_intact() {
    let mut player = Player::new();
    player.play(&C_MAJOR);
    player.play(&with(&C_MAJOR, &[31]));
    let dominant = player.instrument.snapshot();
    assert_eq!(dominant.diagnostics.current.chord.unwrap().root, 7);
    assert_eq!(dominant.diagnostics.current.destination.unwrap().root, 0);
    player.play(&C_MAJOR);
    let events = player.play(&with(&C_MAJOR, &[49]));
    let arrived = player.instrument.snapshot();
    assert_eq!(arrived.diagnostics.applied.direction, Direction::Deceive);
    let chord = arrived.diagnostics.current.chord.unwrap();
    assert_eq!(chord.root, 9);
    assert_eq!(chord.third(), 0);
    assert_eq!(arrived.diagnostics.current.destination, None);
    assert!(arrived
        .support
        .iter()
        .flatten()
        .any(|pitch| pitch % 12 == 9));
    player.assert_manual(&C_MAJOR);
    for note in [48, 52, 55] {
        assert!(!events.contains(&off(note)));
        assert!(!events.contains(&on(note)));
    }
}

#[test]
fn harmonic_taps_articulate_support_without_retriggering_literal_notes_even_with_dense_grips() {
    for grip in [
        C_MAJOR.as_slice(),
        &[15, 19, 20],
        &[14],
        &[15, 9, 10, 14, 4, 6],
    ] {
        for led in (26..50)
            .filter(|led| ![32, 44].contains(led))
            .chain([ARRIVE])
        {
            let mut player = Player::new();
            player.play(grip);
            let literal: BTreeSet<_> = player
                .instrument
                .snapshot()
                .captured
                .into_iter()
                .flatten()
                .collect();
            let events = player.play(&with(grip, &[led]));
            assert!(events.iter().any(|event| matches!(event, MidiEvent::NoteOn { note, .. } if !literal.contains(note))), "grip {grip:?}, control {led}: tap was silent");
            for note in literal {
                assert!(
                    !events.contains(&off(note)),
                    "control {led} interrupted literal {note}"
                );
                assert!(
                    !events.contains(&on(note)),
                    "control {led} retriggered literal {note}"
                );
            }
            assert!(
                player.play(&with(grip, &[led])).is_empty(),
                "held control repeated"
            );
            assert!(
                player.play(grip).is_empty(),
                "column or phrase action release was not quiet"
            );
        }
    }
}

#[test]
fn family_leds_are_stable_and_have_no_default_selected_key() {
    let mut player = Player::new();
    let idle = player.instrument.frame();
    for (column, hue) in [(1, 16), (2, 128), (3, 85), (4, 192)] {
        for row in 0..4 {
            assert_eq!(
                idle[26 + row * 6 + column],
                hsv_to_rgb(Hsv::new(hue, 210, 32))
            );
        }
    }
    player.play(&with(&C_MAJOR, &[39])); // Explicit sixth colour.
    assert_eq!(
        player.instrument.frame()[39],
        hsv_to_rgb(Hsv::new(16, 120, 144))
    );
    assert!(player.play(&C_MAJOR).is_empty());
    assert_eq!(
        player.instrument.frame()[39],
        hsv_to_rgb(Hsv::new(16, 210, 80))
    );
    player.play(&[0]);
    assert_eq!(player.instrument.frame()[0], Rgb::new(144, 0, 0));
}

#[test]
fn leds_distinguish_older_held_choices_from_the_winner_and_the_audible_choice() {
    let mut player = Player::new();
    player.play(&with(&C_MAJOR, &[39]));
    player.play(&with(&C_MAJOR, &[39, 45]));
    let frame = player.instrument.frame();
    assert_eq!(frame[39], hsv_to_rgb(Hsv::new(16, 210, 64)));
    assert_eq!(frame[45], hsv_to_rgb(Hsv::new(16, 120, 144)));

    assert!(player.play(&with(&C_MAJOR, &[39])).is_empty());
    let snapshot = player.instrument.snapshot();
    assert_eq!(snapshot.requested.colour, Colour::Sixth);
    assert_eq!(snapshot.diagnostics.applied.colour, Colour::SecondsFourths);
    let frame = player.instrument.frame();
    assert_eq!(frame[39], hsv_to_rgb(Hsv::new(16, 120, 144)));
    assert_eq!(frame[45], hsv_to_rgb(Hsv::new(16, 210, 80)));

    assert!(player.play(&C_MAJOR).is_empty());
    let frame = player.instrument.frame();
    assert_eq!(frame[39], hsv_to_rgb(Hsv::new(16, 210, 32)));
    assert_eq!(frame[45], hsv_to_rgb(Hsv::new(16, 210, 80)));
    player.play(&[]);
    assert_eq!(
        player.instrument.frame()[45],
        hsv_to_rgb(Hsv::new(16, 210, 32))
    );
}

#[test]
fn incompatible_root_and_relative_requests_keep_literal_notes_and_show_the_limited_family() {
    let mut player = Player::new();
    player.play(&C_MAJOR);
    let events = player.play(&with(&C_MAJOR, &[34, 48]));
    let snapshot = player.instrument.snapshot();
    assert_eq!(snapshot.requested.foundation, Foundation::Root);
    assert_eq!(snapshot.requested.reharm, Reharm::Relative);
    assert_eq!(snapshot.diagnostics.current.chord.unwrap().root, 0);
    assert_eq!(snapshot.diagnostics.limited & 2, 0);
    assert_ne!(snapshot.diagnostics.limited & 8, 0);
    let frame = player.instrument.frame();
    assert_eq!(frame[34], hsv_to_rgb(Hsv::new(128, 120, 144)));
    assert_eq!(frame[48], hsv_to_rgb(Hsv::new(24, 210, 144)));
    player.assert_manual(&C_MAJOR);
    for note in [48, 52, 55] {
        assert!(!events.contains(&off(note)));
        assert!(!events.contains(&on(note)));
    }
    assert!(player.play(&with(&C_MAJOR, &[34])).is_empty());
    assert_eq!(
        player.instrument.frame()[48],
        hsv_to_rgb(Hsv::new(24, 210, 80))
    );
}

#[test]
fn snapshots_distinguish_requested_controls_from_the_still_sounding_plan() {
    use harmony_instrument::cause;
    let mut player = Player::new();
    player.play(&with(&C_MAJOR, &[39]));
    let coloured = player.instrument.snapshot();
    assert_eq!(coloured.requested.colour, Colour::Sixth);
    assert_eq!(coloured.diagnostics.applied.colour, Colour::Sixth);
    assert_eq!(coloured.accepted, 1 << 39);
    assert_ne!(coloured.cause & cause::REPLAN, 0);
    assert!(player.play(&C_MAJOR).is_empty());
    let released = player.instrument.snapshot();
    assert_eq!(released.cause, cause::RHS_RELEASE);
    assert_eq!(released.requested.colour, Colour::Natural);
    assert_eq!(released.diagnostics.applied.colour, Colour::Sixth);
    assert_eq!(released.support, coloured.support);
    assert_eq!(released.accepted, 0);
}

#[test]
fn diagnostics_preserve_typed_requests_captured_pitches_and_actual_leds() {
    let mut player = Player::new();
    player.play(&with(&C_MAJOR, &[39]));
    player.play(&C_MAJOR);
    let snapshot = player.instrument.snapshot();
    let leds = player.instrument.frame();
    let mut out = heapless::String::new();
    telemetry::encode(snapshot, diagnostic_status(), &[], &leds, &mut out).unwrap();
    let decoded: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(out.ends_with('\n'));
    assert_eq!(decoded["v"], 2);
    assert!(decoded.get("answer").is_none());
    assert_eq!(decoded["seq"], snapshot.sequence);
    assert_eq!(decoded["captured"][15], 48);
    assert_eq!(decoded["requested"]["colour"], "Natural");
    assert_eq!(decoded["applied"]["colour"], "Sixth");
    assert_eq!(decoded["support"], serde_json::json!(snapshot.support));
    assert_eq!(decoded["limited"], snapshot.diagnostics.limited);
    let hex = decoded["leds"].as_str().unwrap();
    assert_eq!(hex.len(), 52 * 6);
    for (led, colour) in leds.iter().enumerate() {
        assert_eq!(
            &hex[led * 6..(led + 1) * 6],
            format!("{:02x}{:02x}{:02x}", colour.r, colour.g, colour.b)
        );
    }
}

#[test]
fn diagnostic_json_fits_a_full_polyphonic_restrike_without_losing_events() {
    let mut player = Player::new();
    let all: Vec<_> = (1..24).collect();
    player.play(&all);
    let events = player.play(&with(&all, &[RESTRIKE]));
    assert!(events.len() >= 40);
    let mut out = heapless::String::new();
    telemetry::encode(
        player.instrument.snapshot(),
        diagnostic_status(),
        &events,
        &player.instrument.frame(),
        &mut out,
    )
    .unwrap();
    let decoded: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(decoded["events"].as_array().unwrap().len(), events.len());
    for (actual, expected) in decoded["events"].as_array().unwrap().iter().zip(events) {
        assert_eq!(*actual, serde_json::json!(expected.to_usb_packet()));
    }
}

#[test]
fn snapshot_marks_pedal_and_deceptive_actions_instead_of_recording_or_answering() {
    use harmony_instrument::cause;
    let mut player = Player::new();
    player.play(&C_MAJOR);
    player.play(&with(&C_MAJOR, &[44]));
    let pedal = player.instrument.snapshot();
    assert!(pedal.requested.pedal);
    assert_ne!(pedal.cause & cause::RHS_ATTACK, 0);
    player.play(&with(&C_MAJOR, &[44, 49]));
    assert_eq!(
        player.instrument.snapshot().requested.direction,
        Direction::Deceive
    );
    player.play(&C_MAJOR);
    assert!(!player.instrument.snapshot().requested.pedal);
    player.play(&with(&C_MAJOR, &[RESTRIKE]));
    let restruck = player.instrument.snapshot();
    assert_eq!(restruck.cause, cause::RHS_ATTACK | cause::RESTRIKE);
    player.play(&[0]);
    let stopped = player.instrument.snapshot();
    assert_eq!(stopped.captured, [None; 24]);
    assert_eq!(stopped.support, [None; 6]);
    assert_eq!(stopped.accepted, 0);
}

#[test]
fn piano_background_distinguishes_naturals_and_accidentals_without_hiding_activity() {
    let mut player = Player::new();
    let idle = player.instrument.frame();
    let levels = [40, 0, 24, 0, 24, 24, 0, 24, 0, 24, 0, 24];
    for led in 1..24 {
        let level = levels[usize::from(LEFT[led] % 12)];
        assert_eq!(idle[led], Rgb::new(level, level, level), "LED {led}");
    }
    assert_eq!(idle[3], Rgb::BLACK); // Ab3.
    assert!(player.play(&[3]).contains(&on(56)));
    assert_eq!(player.instrument.frame()[3], pitch_colour(56, 120, 144));
    assert_eq!(player.instrument.frame()[21], pitch_colour(44, 210, 96));
    assert!(player.play(&[]).contains(&off(56)));
    assert_eq!(player.instrument.frame(), idle);
}

#[test]
fn held_notes_light_exact_duplicates_and_other_octaves_at_their_actual_pitches() {
    let mut player = Player::new();
    player.play(&[1]); // D3, duplicated at the opposite end of the left grid.
    let frame = player.instrument.frame();
    assert_eq!(frame[1], pitch_colour(50, 120, 144));
    assert_eq!(frame[23], pitch_colour(50, 160, 112));
    for (led, pitch) in [(19, 38), (5, 62)] {
        assert_eq!(frame[led], pitch_colour(pitch, 210, 96));
    }

    assert!(player.play(&[1, 25]).is_empty());
    let shifted = player.instrument.frame();
    assert_eq!(shifted[1], frame[1], "held D3 follows its attack pitch");
    assert_eq!(shifted[19], pitch_colour(50, 160, 112));
    assert_eq!(shifted[23], pitch_colour(62, 210, 96));
    assert_eq!(shifted[5], pitch_colour(74, 210, 96));

    player.play(&[25]);
    for led in [1, 19, 23, 5] {
        assert_eq!(player.instrument.frame()[led], Rgb::new(24, 24, 24));
    }
}

#[test]
fn full_left_collection_is_literal_and_releases_cleanly() {
    let mut player = Player::new();
    let all: Vec<_> = (1..24).collect();
    player.play(&all);
    player.assert_manual(&all);
    // Shared B2/D3 physical pads own only one MIDI pitch each.
    let literal_count = LEFT[1..].iter().collect::<BTreeSet<_>>().len();
    assert!((literal_count..=literal_count + 6).contains(&player.active.len()));
    assert!(player.play(&all).is_empty());
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn duplicate_manual_pitches_rearticulate_once_and_share_release() {
    let mut player = Player::new();
    player.play(&[1]);
    let events = player.play(&[1, 23]); // Both D3.
    assert!(events.contains(&off(50)));
    assert_eq!(events.iter().filter(|event| **event == on(50)).count(), 1);
    let events = player.play(&[23]);
    assert!(!events.contains(&off(50)));
    assert!(player.active.contains(&50));
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn octave_thumbs_change_new_attacks_and_pin_held_notes() {
    for (thumbs, offset) in [(vec![24], -12i16), (vec![25], 12), (vec![24, 25], 0)] {
        let mut player = Player::new();
        player.play(&[15]);
        player.play(&with(&[15], &thumbs));
        assert!(player.active.contains(&48));
        let events = player.play(&with(&[15, 9], &thumbs));
        let shifted = (52i16 + offset) as u8;
        assert!(events.contains(&on(shifted)));
        assert!(player.active.contains(&48));
        assert!(player.active.contains(&shifted));
        player.play(&[15, 9]);
        assert!(player.active.contains(&48));
        assert!(player.active.contains(&shifted));
        assert_eq!(
            player.instrument.frame()[9],
            pitch_colour(shifted, 120, 144)
        );
        player.play(&[]);
        assert!(player.active.is_empty());
    }
}

#[test]
fn restrike_repeats_exactly_current_sound_without_advancing_harmony() {
    let mut player = Player::new();
    player.play(&with(&C_MAJOR, &[27, 28, 30]));
    player.play(&C_MAJOR);
    let before = player.active.clone();
    let frame = player.instrument.frame();
    for _ in 0..16 {
        let events = player.play(&with(&C_MAJOR, &[RESTRIKE]));
        assert_eq!(player.active, before);
        assert_eq!(events.len(), before.len() * 2);
        for &note in &before {
            assert!(events.contains(&off(note)));
            assert!(events.contains(&on(note)));
        }
        assert!(player.play(&C_MAJOR).is_empty());
        assert_eq!(player.instrument.frame(), frame);
    }
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn changing_left_material_including_release_only_slides_replans_immediately() {
    let mut player = Player::new();
    let phrase: &[&[usize]] = &[
        &[15, 9, 10],    // C E G.
        &[15, 9, 10, 4], // Add B: Cmaj7.
        &[15, 10, 4],    // Release E; no fresh attack.
        &[15, 10, 4, 2], // Add F.
        &[15, 10, 2],    // Release B.
        &[20, 14, 15],   // F A C.
        &[6, 4, 1, 2],   // G B D F.
        &[15, 9, 10],    // C E G.
    ];
    let mut signatures = BTreeSet::new();
    for &notes in phrase {
        player.play(&with(notes, &[29, 33, 34]));
        player.assert_manual(notes);
        assert!(player.play(&with(notes, &[29, 33, 34])).is_empty());
        signatures.insert(player.active.clone());
    }
    assert!(signatures.len() >= 6);
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn actual_phrase_prepares_leans_delays_and_arrives_without_losing_material() {
    let mut player = Player::new();
    let phrase: &[&[usize]] = &[
        &[15, 9, 10],
        &[15, 9, 10, 26],    // Prepare.
        &[20, 14, 15, 26],   // F A C.
        &[20, 14, 15, 32],   // Delay prepared inner voice.
        &[6, 1, 10, 32, 31], // G D G, lean over retained preparation.
        &[6, 1, 10, 32],
        &[6, 1, 10],          // Releasing Delay permits resolution.
        &[15, 9, 10, ARRIVE], // C E G arrival.
        &[15, 9, 10],
    ];
    let mut changed = 0;
    for &notes in phrase {
        if !player.play(notes).is_empty() {
            changed += 1;
        }
        let literal: Vec<_> = notes
            .iter()
            .copied()
            .filter(|led| (1..24).contains(led))
            .collect();
        player.assert_manual(&literal);
    }
    assert!(changed >= 5);
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn a_prepared_c_suspension_releases_to_b_when_delay_is_lifted() {
    let mut player = Player::new();
    // F3 and A3: press F2/A2 with the octave-up thumb.
    player.play(&[20, 14, 25]);
    assert!(player.active.contains(&60), "F harmony did not prepare C4");
    // Release the octave thumb and move to literal G3/D4 while holding Delay.
    player.play(&[10, 5, 32]);
    assert!(player.active.contains(&55));
    assert!(player.active.contains(&62));
    assert!(player.active.contains(&60), "prepared C4 was not retained");
    assert_eq!(
        player.instrument.frame()[32],
        hsv_to_rgb(Hsv::new(128, 48, 144))
    );
    assert_eq!(
        player.instrument.frame()[ARRIVE],
        hsv_to_rgb(Hsv::new(85, 210, 80))
    );
    for movement in [41, 35, 29, 41, 39] {
        let events = player.play(&[10, 5, 32, movement]);
        assert!(
            !events.contains(&off(60)),
            "Movement control {movement} released a suspension while Delay remained held"
        );
        assert!(player.active.contains(&60), "held suspension disappeared");
        assert!(player.active.contains(&55));
        assert!(player.active.contains(&62));
        assert!(player.play(&[10, 5, 32]).is_empty());
    }
    let events = player.play(&[10, 5]);
    assert!(
        events.contains(&off(60)),
        "lifting Delay did not release C4"
    );
    assert!(events.contains(&on(59)), "C4 did not descend to B3");
    assert!(!events.contains(&off(55)));
    assert!(!events.contains(&off(62)));
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn stop_and_output_recovery_require_fresh_notes_thumbs_and_steering() {
    for ready in [false, true] {
        let mut player = Player::new();
        let held = with(&C_MAJOR, &[25, 27, 28, RESTRIKE, ARRIVE]);
        player.play(&held);
        let blocked = if ready {
            with(&held, &[0])
        } else {
            held.clone()
        };
        player.scan(&blocked, ready);
        assert!(player.active.is_empty());
        assert!(player.play(&held).is_empty());
        assert!(player.play(&with(&held, &[43])).is_empty());
        player.play(&[]);
        let events = player.play(&[15]);
        assert!(events.contains(&on(48)), "stale octave survived recovery");
        assert!(!events.contains(&on(60)));
        assert_eq!(
            player.instrument.frame()[27],
            hsv_to_rgb(Hsv::new(16, 210, 32))
        );
        player.play(&[]);
        assert!(player.active.is_empty());
    }
}

#[test]
fn a_manual_attack_can_take_ownership_of_an_existing_support_pitch() {
    let mut player = Player::new();
    player.play(&[15]);
    let (led, thumb, note) = (1..24)
        .filter(|led| *led != 15)
        .flat_map(|led| {
            [(24, -12i16), (25, 12), (usize::MAX, 0)]
                .into_iter()
                .map(move |(thumb, offset)| (led, thumb, (i16::from(LEFT[led]) + offset) as u8))
        })
        .find(|(_, _, note)| player.active.contains(note))
        .expect("baseline realization should contain a reachable supporting pitch");
    let mut keys = vec![15, led];
    if thumb != usize::MAX {
        keys.push(thumb);
    }
    let events = player.play(&keys);
    assert!(events.contains(&off(note)));
    assert_eq!(events.iter().filter(|event| **event == on(note)).count(), 1);
    keys.push(43);
    let events = player.play(&keys);
    assert!(
        !events.contains(&off(note)),
        "reharm cut the new literal owner"
    );
    assert!(player.active.contains(&note));
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn time_and_led_reads_never_generate_music_or_advance_the_phrase() {
    let mut player = Player::new();
    let held = with(&C_MAJOR, &[27, 28, 29, 30, 31, 32]);
    player.play(&held);
    let frame = player.instrument.frame();
    let notes = player.active.clone();
    for _ in 0..128 {
        player.now += 1000;
        assert!(player.play(&held).is_empty());
        assert_eq!(player.instrument.frame(), frame);
        assert_eq!(player.active, notes);
    }
}

#[test]
fn long_interleaved_sliding_performance_never_leaks_or_double_owns_notes() {
    let mut player = Player::new();
    let mut state = 0x579a_c234u32;
    let mut held = [false; 52];
    for iteration in 0..1600 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let led = (state as usize % 51) + 1;
        held[led] = !held[led];
        let keys: Vec<_> = held
            .iter()
            .enumerate()
            .filter_map(|(led, &pressed)| pressed.then_some(led))
            .collect();
        player.scan(&keys, iteration % 149 != 0);
        if iteration % 100 == 0 {
            player.play(&[]);
            held.fill(false);
            assert!(player.active.is_empty());
        }
    }
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn snapshots_advance_only_on_physical_or_readiness_changes() {
    use harmony_instrument::cause;
    let mut player = Player::new();
    let initial = player.instrument.snapshot();
    assert_eq!(initial.sequence, 0);
    assert!(!initial.ready);
    assert_eq!(initial.physical, 0);
    assert_eq!(initial.accepted, 0);
    player.play(&C_MAJOR);
    let attacked = player.instrument.snapshot();
    assert_eq!(attacked.sequence, 1);
    assert_eq!(attacked.time_ms, player.now);
    assert!(attacked.ready);
    assert_eq!(
        attacked.cause,
        cause::MANUAL_ATTACK | cause::RECOVERY | cause::REPLAN
    );
    assert_eq!(
        attacked.physical,
        C_MAJOR.into_iter().fold(0, |mask, led| mask | 1 << led)
    );
    assert_eq!(attacked.accepted, 0);
    for led in C_MAJOR {
        assert_eq!(attacked.captured[led], Some(LEFT[led]));
    }
    assert_eq!(attacked.support, attacked.diagnostics.current.notes);
    for _ in 0..4 {
        assert_eq!(
            player.instrument.snapshot(),
            attacked,
            "observation itself cannot advance music"
        );
        assert!(player.play(&C_MAJOR).is_empty());
        assert_eq!(
            player.instrument.snapshot(),
            attacked,
            "unchanged scans cannot create events"
        );
    }
}

#[test]
fn snapshot_records_attack_time_pitch_instead_of_current_octave_labels() {
    use harmony_instrument::cause;
    let mut player = Player::new();
    player.play(&[9]);
    assert!(player.play(&[9, 25]).is_empty());
    let shifted = player.instrument.snapshot();
    assert_eq!(shifted.captured[9], Some(52));
    assert_eq!(shifted.accepted, 1 << 25);
    assert_eq!(shifted.cause, cause::RHS_ATTACK);
    player.play(&[9, 15, 25]);
    let mixed = player.instrument.snapshot();
    assert_eq!(mixed.captured[9], Some(52));
    assert_eq!(mixed.captured[15], Some(60));
    assert_eq!(mixed.physical, (1 << 9) | (1 << 15) | (1 << 25));
    assert_eq!(mixed.cause, cause::MANUAL_ATTACK | cause::REPLAN);
}

#[test]
fn recovery_snapshot_shows_physical_keys_without_accepted_or_sounding_ownership() {
    use harmony_instrument::cause;
    let mut player = Player::new();
    let pressed = [9, 15, 25, 27];
    player.play(&pressed);
    let sounding = player.instrument.snapshot();
    player.scan(&pressed, false);
    let disconnected = player.instrument.snapshot();
    assert_eq!(disconnected.sequence, sounding.sequence + 1);
    assert_eq!(disconnected.cause, cause::RECOVERY);
    assert!(!disconnected.ready);
    assert_eq!(disconnected.physical, sounding.physical);
    assert_eq!(disconnected.accepted, 0);
    assert_eq!(disconnected.captured, [None; 24]);
    assert_eq!(disconnected.support, [None; 6]);
    assert_eq!(disconnected.diagnostics.current.chord, None);
    assert_eq!(disconnected.requested, Default::default());
    assert!(player.scan(&pressed, false).is_empty());
    assert_eq!(player.instrument.snapshot(), disconnected);
    assert!(player.play(&pressed).is_empty());
    let recovered = player.instrument.snapshot();
    assert_eq!(recovered.sequence, disconnected.sequence + 1);
    assert!(recovered.ready);
    assert_eq!(recovered.cause, cause::RECOVERY);
    assert_eq!(recovered.accepted, 0);
    assert_eq!(recovered.captured, [None; 24]);
    player.play(&[]);
    player.play(&[9]);
    assert_eq!(player.instrument.snapshot().captured[9], Some(52));
}

#[test]
fn snapshot_exposes_delayed_voice_and_exact_resolution_target() {
    use harmony_instrument::cause;
    let mut player = Player::new();
    player.play(&[20, 14, 25]); // F3 A3.
    player.play(&[10, 5, 32]); // G3 D4 plus Delay.
    let delayed = player.instrument.snapshot();
    let voice = delayed
        .diagnostics
        .current
        .suspended
        .iter()
        .position(|held| *held)
        .unwrap();
    assert_eq!(delayed.support[voice], Some(60));
    assert_eq!(delayed.diagnostics.resolved[voice], Some(59));
    assert!(delayed.requested.delay);
    player.play(&[10, 5]);
    let resolved = player.instrument.snapshot();
    assert_eq!(
        resolved.cause,
        cause::RHS_RELEASE | cause::DELAY_RELEASE | cause::REPLAN
    );
    assert_eq!(resolved.support[voice], Some(59));
    assert!(!resolved.diagnostics.current.suspended[voice]);
    assert!(!resolved.requested.delay);
}
