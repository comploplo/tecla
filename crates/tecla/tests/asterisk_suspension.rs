#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test assertions")]

#[path = "../../../keyboards/tecla/asterisk_v1_4_suspension/controls.rs"]
mod controls;

use std::collections::BTreeSet;

use tecla::matrix::MatrixState;
use tecla::music::realization::{Diagnostics, Direction, Motion};
use tecla::pipeline::report::MidiEvent;

// Physical labels in the hardware-verified scanner order.
const ARRIVE: usize = 13; // Left lower S.
const PREPARE: usize = 2; // Left upper S.
const LEAN: usize = 3; // Left T.
const HIGHER: usize = 4; // Left P.
const OPEN: usize = 9; // Right upper P.
const RETAIN: usize = 19; // Right lower G.
const LETTERS: [usize; 18] = [
    2, 3, 4, 5, 13, 14, 15, 16, 8, 9, 10, 11, 12, 17, 18, 19, 20, 21,
];

fn matrix(keys: &[usize]) -> MatrixState<1, 26> {
    let mut frame = MatrixState::new();
    for &key in keys {
        assert!(frame.set(0, key, true));
    }
    frame
}

struct Player {
    instrument: controls::Controls,
    active: BTreeSet<u8>,
    now: u64,
}

impl Player {
    fn new() -> Self {
        Self {
            instrument: controls::Controls::new(),
            active: BTreeSet::new(),
            now: 0,
        }
    }

    fn at(&mut self, keys: &[usize], now: u64, ready: bool) -> Vec<MidiEvent> {
        assert!(now >= self.now);
        self.now = now;
        let mut events = Vec::new();
        self.instrument
            .update_at(&matrix(keys), now, ready, &mut |event| events.push(event));
        assert!(events.len() <= 64, "shared MIDI scan batch overflow");
        let mut attacked = false;
        for &event in &events {
            match event {
                MidiEvent::NoteOff { channel, note } => {
                    assert_eq!(channel, 0);
                    assert!(!attacked, "release after attack: {events:?}");
                    assert!(self.active.remove(&note), "released unowned note {note}");
                }
                MidiEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                } => {
                    attacked = true;
                    assert_eq!(channel, 0);
                    assert!((1..=127).contains(&velocity));
                    assert!(note <= 127);
                    assert!(
                        self.active.insert(note),
                        "double-owned MIDI note {note}: {events:?}"
                    );
                }
                _ => panic!("unexpected MIDI event: {event:?}"),
            }
        }
        events
    }

    fn play(&mut self, keys: &[usize]) -> Vec<MidiEvent> {
        self.at(keys, self.now + 10, true)
    }

    fn diagnostics(&self) -> Diagnostics {
        self.instrument.diagnostics()
    }

    fn body(&self) {
        assert!(
            self.active.len() >= 4,
            "harmonic body thinned: {:?}; {:?}",
            self.active,
            self.diagnostics()
        );
        assert_eq!(
            self.active,
            self.diagnostics()
                .current
                .notes
                .into_iter()
                .flatten()
                .collect(),
            "planner and heard MIDI diverged"
        );
    }
}

#[test]
fn every_letter_engages_a_complete_body_without_becoming_a_note_keyboard() {
    for pad in LETTERS {
        let mut player = Player::new();
        assert!(!player.play(&[pad]).is_empty(), "silent physical pad {pad}");
        player.body();
        player.play(&[]);
        player.at(&[], player.now + 100, true);
        assert!(player.active.is_empty(), "pad {pad} left a note behind");
    }
}

#[test]
fn modifiers_alone_never_start_a_harmony() {
    for keys in [vec![0], vec![1], vec![6], vec![7], vec![22, 23, 24, 25]] {
        let mut player = Player::new();
        assert!(player.play(&keys).is_empty());
        assert!(player.active.is_empty());
    }
}

#[test]
fn held_contacts_do_not_advance_or_rearticulate_a_phrase_as_time_passes() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, OPEN, RETAIN]);
    let before = player.diagnostics();
    let heard = player.active.clone();
    for time in [100, 1_000, 60_000, 3_600_000] {
        assert!(player.at(&[ARRIVE, OPEN, RETAIN], time, true).is_empty());
        assert_eq!(player.diagnostics(), before);
        assert_eq!(player.active, heard);
    }
}

#[test]
fn preparation_dominant_and_arrival_follow_the_sounded_destination() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    let home = player.diagnostics().current.chord.unwrap();
    player.play(&[ARRIVE, PREPARE]);
    let prepared = player.diagnostics().current;
    assert_eq!(prepared.destination, Some(home));
    assert_eq!(prepared.chord.unwrap().root, (home.root + 2) % 12);
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, LEAN]);
    let dominant = player.diagnostics().current;
    assert_eq!(dominant.destination, Some(home));
    assert_eq!(dominant.chord.unwrap().root, (home.root + 7) % 12);
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, 24]); // E asks for the established arrival.
    assert_eq!(player.diagnostics().current.chord, Some(home));
    assert_eq!(player.diagnostics().current.destination, None);
    player.body();
}

#[test]
fn a_heard_preparation_survives_the_foundation_move_then_resolves_by_step() {
    let mut player = Player::new();
    player.play(&[5]); // H continues from the tonal home to F major.
    let preparation = player.diagnostics().current;
    let prepared_midi = player.active.clone();
    assert_eq!(preparation.chord.unwrap().root, 5);
    player.play(&[5, RETAIN]);
    player.play(&[RETAIN]);
    let movement = player.play(&[RETAIN, HIGHER]); // The footing moves F -> G.
    let displaced = player.diagnostics();
    assert_eq!(displaced.current.chord.unwrap().root, 7);
    assert!(
        displaced
            .current
            .suspended
            .into_iter()
            .filter(|held| *held)
            .count()
            >= 2
    );
    for (voice, held) in displaced.current.suspended.into_iter().enumerate() {
        if !held {
            continue;
        }
        let pitch = preparation.notes[voice].unwrap();
        let target = displaced.resolved[voice].unwrap();
        assert!(
            prepared_midi.contains(&pitch),
            "preparation was never heard"
        );
        assert_eq!(displaced.current.notes[voice], Some(pitch));
        assert!(player.active.contains(&pitch));
        assert!(pitch > target && pitch - target <= 2);
        assert!(
            !movement.iter().any(|event| matches!(
                event,
                MidiEvent::NoteOff { note, .. } if *note == pitch
            )),
            "prepared note was retriggered during displacement"
        );
    }
    player.body();

    let arrival = player.play(&[RETAIN, HIGHER, 24]);
    for (voice, held) in displaced.current.suspended.into_iter().enumerate() {
        if held {
            let pitch = displaced.current.notes[voice].unwrap();
            assert_eq!(player.diagnostics().current.notes[voice], Some(pitch));
            assert!(player.diagnostics().current.suspended[voice]);
            assert!(
                !arrival.iter().any(|event| matches!(
                    event,
                    MidiEvent::NoteOff { note, .. } if *note == pitch
                )),
                "arrival stole a physically retained preparation"
            );
        }
    }

    player.play(&[HIGHER]); // Lift right G, keeping the footing contact down.
    let resolved = player.diagnostics().current;
    assert_eq!(resolved.chord, displaced.current.chord);
    assert_eq!(resolved.destination, displaced.current.destination);
    assert!(!resolved.suspended.into_iter().any(|held| held));
    for (voice, held) in displaced.current.suspended.into_iter().enumerate() {
        if held {
            assert_eq!(resolved.notes[voice], displaced.resolved[voice]);
        }
    }
    player.body();
}

#[test]
fn lifting_one_right_hand_group_resolves_it_while_another_group_stays_prepared() {
    let mut player = Player::new();
    player.play(&[5]);
    player.play(&[5, 17, 21]); // Lower right R and Z catch opposite regions.
    player.play(&[17, 21]);
    player.play(&[17, 21, HIGHER]);
    let displaced = player.diagnostics();
    assert!(
        displaced
            .current
            .suspended
            .into_iter()
            .filter(|held| *held)
            .count()
            >= 2
    );

    player.play(&[17, HIGHER]); // Release the upper-region contact, retaining the overlap.
    let partial = player.diagnostics().current;
    let mut released = 0;
    let mut retained = 0;
    for (voice, held) in displaced.current.suspended.into_iter().enumerate() {
        if !held {
            continue;
        }
        if partial.suspended[voice] {
            retained += 1;
            assert_eq!(partial.notes[voice], displaced.current.notes[voice]);
        } else {
            released += 1;
            assert_eq!(partial.notes[voice], displaced.resolved[voice]);
        }
    }
    assert!(released > 0, "upper region never resolved: {partial:?}");
    assert!(
        retained > 0,
        "releasing one region stole the other: {partial:?}"
    );
    assert_eq!(partial.chord, displaced.current.chord);
    assert_eq!(partial.destination, displaced.current.destination);
    player.body();

    player.play(&[HIGHER]);
    assert!(!player
        .diagnostics()
        .current
        .suspended
        .into_iter()
        .any(|held| held));
    player.body();
}

#[test]
fn overlapping_right_contacts_share_their_retained_relationships() {
    let mut player = Player::new();
    player.play(&[5]);
    player.play(&[5, 17, 18]); // Adjacent right R/B share a lower region.
    player.play(&[17, 18]);
    player.play(&[17, 18, HIGHER]);
    let caught = player.diagnostics().current;
    assert!(caught.suspended.into_iter().any(|held| held));
    let heard = player.active.clone();
    assert!(player.play(&[18, HIGHER]).is_empty());
    assert_eq!(player.active, heard);
    assert_eq!(player.diagnostics().current, caught);
    player.play(&[HIGHER]);
    assert!(!player
        .diagnostics()
        .current
        .suspended
        .into_iter()
        .any(|held| held));
    player.body();
}

#[test]
fn the_same_arrival_gesture_respects_different_preceding_phrases() {
    let mut tonic = Player::new();
    tonic.play(&[ARRIVE]);
    tonic.play(&[ARRIVE, 23]);
    tonic.play(&[ARRIVE]);
    tonic.play(&[ARRIVE, 24]);

    let mut local = Player::new();
    local.play(&[ARRIVE]);
    local.play(&[ARRIVE, HIGHER]);
    let destination = local.diagnostics().current.chord;
    local.play(&[ARRIVE]);
    local.play(&[ARRIVE, 23]);
    assert_eq!(local.diagnostics().current.destination, destination);
    local.play(&[ARRIVE]);
    local.play(&[ARRIVE, 24]);

    assert_eq!(local.diagnostics().current.chord, destination);
    assert_ne!(
        local.diagnostics().current.chord,
        tonic.diagnostics().current.chord
    );
    assert_ne!(local.active, tonic.active);
}

#[test]
fn a_rest_ends_sound_but_keeps_the_destination_and_clears_physical_preparation() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, 23]);
    let destination = player.diagnostics().current.destination;
    player.play(&[]);
    player.at(&[], player.now + 100, true);
    assert!(player.active.is_empty());
    assert_eq!(player.diagnostics().current.destination, destination);
    assert!(player
        .diagnostics()
        .current
        .notes
        .into_iter()
        .all(|note| note.is_none()));
    assert!(!player
        .diagnostics()
        .current
        .suspended
        .into_iter()
        .any(|held| held));
    player.play(&[ARRIVE, RETAIN, 24]);
    assert_eq!(player.diagnostics().current.chord, destination);
    assert!(!player
        .diagnostics()
        .current
        .suspended
        .into_iter()
        .any(|held| held));
    player.body();
}

#[test]
fn a_short_gap_during_a_finger_transfer_keeps_the_sounding_body() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    let heard = player.active.clone();
    assert!(player.play(&[]).is_empty());
    assert_eq!(player.active, heard);
    player.at(&[ARRIVE], player.now + 5, true);
    player.body();
}

#[test]
fn releasing_phrase_and_colour_modifiers_does_not_undo_the_music() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    for modifier in [22, 23, 24, 25, 6, 7, 0, 1] {
        player.play(&[ARRIVE, modifier]);
        let heard = player.active.clone();
        let state = player.diagnostics();
        assert!(
            player.play(&[ARRIVE]).is_empty(),
            "release of {modifier} replayed music"
        );
        assert_eq!(player.active, heard);
        assert_eq!(player.diagnostics(), state);
        player.body();
    }
}

#[test]
fn stopping_or_losing_usb_requires_fresh_playing_contacts() {
    let mut player = Player::new();
    player.play(&[ARRIVE, RETAIN]);
    player.play(&[ARRIVE, RETAIN, 0, 1]);
    assert!(player.active.is_empty());
    assert!(player.play(&[ARRIVE, RETAIN]).is_empty());
    player.play(&[]);
    player.play(&[ARRIVE]);
    player.body();
    player.at(&[ARRIVE], player.now + 1, false);
    assert!(player.active.is_empty());
    assert!(player.at(&[ARRIVE], player.now + 1, true).is_empty());
    player.play(&[]);
    player.play(&[ARRIVE]);
    player.body();
}

#[test]
fn long_playing_and_dense_overlapping_contacts_never_thin_or_leak_voices() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    let gestures: [&[usize]; 9] = [
        &[ARRIVE, PREPARE],
        &[ARRIVE, LEAN],
        &[ARRIVE, 24],
        &[ARRIVE, HIGHER, 8],
        &[ARRIVE, 15, 9],
        &[ARRIVE, 5, 11, 7],
        &[ARRIVE, 16, 10],
        &[ARRIVE, 14, 12],
        &[ARRIVE, 2, 3, 4, 5, 8, 9, 10, 11, 12],
    ];
    for turn in 0..108 {
        player.play(&[ARRIVE]);
        player.play(gestures[turn % gestures.len()]);
        player.body();
    }
    player.play(&[]);
    player.at(&[], player.now + 100, true);
    assert!(player.active.is_empty());
}

#[test]
fn a_foundation_step_still_changes_harmony_while_the_right_hand_holds_it_open() {
    let mut player = Player::new();
    player.play(&[5]); // H establishes F major from the tonal home.
    player.play(&[5, OPEN]);
    player.play(&[OPEN]);
    let before = player.active.clone();
    assert_eq!(player.diagnostics().current.chord.unwrap().root, 5);

    let movement = player.play(&[OPEN, HIGHER]);
    assert_eq!(player.diagnostics().current.chord.unwrap().root, 7);
    assert_eq!(player.diagnostics().applied.motion, Motion::Outward);
    assert!(
        !movement.is_empty(),
        "foundation request disappeared into spacing"
    );
    assert_ne!(player.active, before);
    player.body();
}

#[test]
fn fresh_vertical_gestures_speak_while_a_spacing_contact_remains_held() {
    let mut player = Player::new();
    player.play(&[ARRIVE, OPEN]);
    let before_rise = player.active.clone();
    assert!(!player.play(&[ARRIVE, OPEN, 8]).is_empty()); // Right F rises.
    assert_eq!(player.diagnostics().applied.motion, Motion::Rising);
    assert_ne!(player.active, before_rise);
    player.body();

    player.play(&[ARRIVE, OPEN]);
    let before_fall = player.active.clone();
    assert!(!player.play(&[ARRIVE, OPEN, 12]).is_empty()); // Right D falls.
    assert_eq!(player.diagnostics().applied.motion, Motion::Falling);
    assert_ne!(player.active, before_fall);
    player.body();
}

#[test]
fn a_borrowed_foundation_can_continue_by_a_fourth_after_borrow_is_released() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, 5]); // Establish F before borrowing its flat-side family.
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, 6]);
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, 14]); // K takes the borrowed minor's deceptive arrival.
    let borrowed = player.diagnostics().current.chord.unwrap().root;
    assert!(!matches!(borrowed, 0 | 2 | 4 | 5 | 7 | 9 | 11));
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, 5]); // H continues the descending-fifths sequence.
    let next = player.diagnostics().current.chord.unwrap().root;
    assert_eq!((next + 12 - borrowed) % 12, 5);
    player.body();
}

#[test]
fn physically_retained_tones_acquire_new_targets_through_later_harmonies() {
    let mut player = Player::new();
    player.play(&[5]); // F major prepares the material.
    player.play(&[5, RETAIN]);
    player.play(&[RETAIN]);
    player.play(&[RETAIN, HIGHER]); // G major displaces it.
    let displaced = player.diagnostics();
    assert!(displaced.current.suspended.into_iter().any(|held| held));

    player.play(&[RETAIN]);
    player.play(&[RETAIN, HIGHER]); // A minor makes the same retained tones consonant.
    let common = player.diagnostics().current;
    assert_eq!(common.chord.unwrap().root, 9);
    for (voice, held) in displaced.current.suspended.into_iter().enumerate() {
        if held {
            assert_eq!(common.notes[voice], displaced.current.notes[voice]);
            assert!(!common.suspended[voice]);
        }
    }

    player.play(&[RETAIN]);
    player.play(&[RETAIN, 5]); // D minor changes their meaning again.
    let latest = player.diagnostics();
    let palette = latest.current.chord.unwrap().tones();
    assert_eq!(latest.current.chord.unwrap().root, 2);
    let mut retargeted = false;
    for (voice, held) in displaced.current.suspended.into_iter().enumerate() {
        if held {
            assert_eq!(latest.current.notes[voice], displaced.current.notes[voice]);
            assert!(palette.contains(latest.resolved[voice].unwrap()));
            retargeted |= latest.resolved[voice] != displaced.resolved[voice];
        }
    }
    assert!(retargeted, "resolution targets remained stuck in G major");
    let release = player.play(&[5]);
    assert!(
        !release.is_empty(),
        "releasing the current dissonance did nothing"
    );
    for (voice, held) in displaced.current.suspended.into_iter().enumerate() {
        if held {
            assert_eq!(
                player.diagnostics().current.notes[voice],
                latest.resolved[voice]
            );
        }
    }
    assert_eq!(player.diagnostics().current.chord, latest.current.chord);
    assert!(!player
        .diagnostics()
        .current
        .suspended
        .into_iter()
        .any(|held| held));
    player.body();
}

#[test]
fn overlapping_spacing_contacts_follow_the_new_gesture_without_a_neutral_gap() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, OPEN]);
    assert_eq!(player.diagnostics().applied.motion, Motion::Outward);
    let open = player.active.clone();
    assert!(!player.play(&[ARRIVE, OPEN, 11]).is_empty()); // P -> T overlap.
    assert_eq!(player.diagnostics().applied.motion, Motion::Inward);
    assert_ne!(player.active, open);
    let gathered = player.active.clone();
    assert!(player.play(&[ARRIVE, 11]).is_empty());
    assert_eq!(player.active, gathered);
    player.body();
}

#[test]
fn overlapping_vertical_contacts_keep_the_latest_direction_during_foundation_changes() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, 8]);
    assert_eq!(player.diagnostics().applied.motion, Motion::Rising);
    assert!(!player.play(&[ARRIVE, 8, 12]).is_empty()); // F -> D overlap.
    assert_eq!(player.diagnostics().applied.motion, Motion::Falling);
    player.play(&[ARRIVE, 8, 12, HIGHER]);
    assert_eq!(player.diagnostics().applied.motion, Motion::Falling);
    assert_eq!(player.diagnostics().current.chord.unwrap().root, 2);
    let heard = player.active.clone();
    assert!(player.play(&[ARRIVE, 12, HIGHER]).is_empty());
    assert_eq!(player.active, heard);
    player.body();
}

#[test]
fn a_fresh_thumb_overrides_an_older_held_thumb_until_it_is_released() {
    let mut player = Player::new();
    player.play(&[ARRIVE, 24]); // E holds the arrival preference.
    assert_eq!(player.diagnostics().applied.direction, Direction::Arrive);
    assert!(!player.play(&[ARRIVE, 24, 23]).is_empty()); // O enters over E.
    assert_eq!(player.diagnostics().applied.direction, Direction::Lean);
    assert_eq!(player.diagnostics().current.chord.unwrap().root, 7);
    player.play(&[ARRIVE, 24, 23, HIGHER]);
    assert_eq!(player.diagnostics().applied.direction, Direction::Lean);

    let heard = player.active.clone();
    assert!(player.play(&[ARRIVE, 24, HIGHER]).is_empty());
    assert_eq!(player.active, heard);
    assert!(!player.play(&[ARRIVE, 24, HIGHER, 5]).is_empty());
    assert_eq!(player.diagnostics().applied.direction, Direction::Arrive);
    assert_eq!(player.diagnostics().current.chord.unwrap().root, 0);
    player.body();
}

#[test]
fn long_retained_phrases_and_group_releases_keep_a_complete_audible_body() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    player.play(&[ARRIVE, RETAIN]);
    let movements: [&[usize]; 10] = [
        &[ARRIVE, RETAIN, PREPARE],
        &[ARRIVE, RETAIN, LEAN],
        &[ARRIVE, RETAIN, 24],
        &[ARRIVE, RETAIN, HIGHER, 8],
        &[ARRIVE, RETAIN, 15, OPEN],
        &[ARRIVE, RETAIN, 5, 11, 7],
        &[ARRIVE, RETAIN, 16, 10, 6],
        &[ARRIVE, RETAIN, 14, 12],
        &[ARRIVE, RETAIN, 23, 7],
        &[ARRIVE, RETAIN, 25, 6, 7],
    ];
    for turn in 0..120 {
        player.play(&[ARRIVE, RETAIN]);
        let keys = movements[turn % movements.len()];
        player.play(keys);
        assert!(
            player.active.len() >= 4,
            "held phrase thinned at turn {turn}, keys={keys:?}, heard={:?}, state={:?}",
            player.active,
            player.diagnostics()
        );
        player.body();
    }

    for keys in [
        &[ARRIVE, RETAIN, 17, 21][..],
        &[ARRIVE, 17, 21],
        &[ARRIVE, 21],
        &[ARRIVE],
    ] {
        player.play(keys);
        assert!(
            player.active.len() >= 4,
            "retention release thinned keys={keys:?}, heard={:?}, state={:?}",
            player.active,
            player.diagnostics()
        );
        player.body();
    }
    player.play(&[]);
    player.at(&[], player.now + 100, true);
    assert!(player.active.is_empty());
}

#[test]
fn inversion_still_has_a_bass_when_lower_upper_material_is_retained() {
    let mut player = Player::new();
    player.play(&[ARRIVE]);
    for _ in 0..12 {
        player.play(&[ARRIVE, 12]); // Bring the upper material toward its low boundary.
        player.play(&[ARRIVE]);
    }
    player.play(&[ARRIVE, RETAIN]);
    let prepared = player.diagnostics().current.notes;
    for bar in [0, 1] {
        player.play(&[ARRIVE, RETAIN, bar]);
        let body = player.diagnostics().current;
        let chord = body.chord.unwrap();
        let bass = body.notes[0].expect("an inversion must not erase the foundation");
        assert_eq!(
            bass % 12,
            if bar == 0 {
                chord.third()
            } else {
                chord.fifth()
            }
        );
        for (voice, note) in prepared.into_iter().enumerate().skip(1) {
            if note.is_some() {
                assert_eq!(body.notes[voice], note, "inversion stole a retained tone");
            }
        }
        player.body();
        player.play(&[ARRIVE, RETAIN]);
    }
}

#[test]
fn releasing_g_after_a_c_to_f_phrase_keeps_a_complete_distinct_body() {
    let mut player = Player::new();
    player.play(&[ARRIVE]); // Left lower S establishes C.
    player.play(&[ARRIVE, RETAIN]); // Right lower G catches the upper voices.
    player.play(&[ARRIVE, RETAIN, 5]); // Left H moves by a fourth to F.
    assert_eq!(player.diagnostics().current.chord.unwrap().root, 5);
    assert!(player.diagnostics().current.notes[0].is_some());
    assert!(player
        .diagnostics()
        .current
        .suspended
        .into_iter()
        .any(|held| held));
    player.play(&[ARRIVE, 5]);
    assert!(!player
        .diagnostics()
        .current
        .suspended
        .into_iter()
        .any(|held| held));
    player.body();
    let notes: Vec<_> = player
        .diagnostics()
        .current
        .notes
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(notes.len(), player.active.len());
}
