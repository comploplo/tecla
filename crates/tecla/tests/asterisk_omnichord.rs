#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]

#[path = "../../../keyboards/tecla/asterisk_v1_4_omnichord/controls.rs"]
mod controls;

use std::collections::BTreeSet;

use tecla::matrix::MatrixState;
use tecla::pipeline::report::MidiEvent;

// Independent oracle from the hardware-verified steno GPIO order, not the profile.
// Upper S/T/P/H; lower S/K/W/R.
const ROOTS: [(usize, u8); 8] = [
    (2, 43),
    (3, 45),
    (4, 47),
    (5, 48),
    (13, 36),
    (14, 38),
    (15, 40),
    (16, 41),
];
// Ascending path: lower R/B/G/S/Z, then upper D/T/L/P/F back toward the star.
const STRINGS: [usize; 10] = [17, 18, 19, 20, 21, 12, 11, 10, 9, 8];

fn matrix(keys: &[usize]) -> MatrixState<1, 26> {
    let mut frame = MatrixState::new();
    for &key in keys {
        assert!(frame.set(0, key, true));
    }
    frame
}

fn on(note: u8, velocity: u8) -> MidiEvent {
    MidiEvent::NoteOn {
        channel: 0,
        note,
        velocity,
    }
}

fn off(note: u8) -> MidiEvent {
    MidiEvent::NoteOff { channel: 0, note }
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
        assert!(
            events.len() <= 64,
            "one scan overflowed the shared MIDI batch"
        );
        let mut attacked = false;
        for &event in &events {
            match event {
                MidiEvent::NoteOff { channel, note } => {
                    assert_eq!(channel, 0);
                    assert!(!attacked, "all releases must precede attacks: {events:?}");
                    assert!(
                        self.active.remove(&note),
                        "released an unowned pitch {note}"
                    );
                }
                MidiEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                } => {
                    attacked = true;
                    assert_eq!(channel, 0);
                    assert!(matches!(velocity, 64 | 100));
                    assert!(note <= 127);
                    assert!(
                        self.active.insert(note),
                        "pitch {note} attacked twice without a release: {events:?}"
                    );
                }
                _ => panic!("unexpected MIDI message {event:?}"),
            }
        }
        events
    }

    fn play(&mut self, keys: &[usize]) -> Vec<MidiEvent> {
        self.at(keys, self.now + 1, true)
    }
}

fn joined(left: &[usize], right: &[usize]) -> Vec<usize> {
    left.iter().chain(right).copied().collect()
}

fn attacks(events: &[MidiEvent], wanted_velocity: u8) -> Vec<u8> {
    events
        .iter()
        .filter_map(|event| match *event {
            MidiEvent::NoteOn { note, velocity, .. } if velocity == wanted_velocity => Some(note),
            _ => None,
        })
        .collect()
}

fn quality(mask: u8) -> Vec<u8> {
    let mut intervals = vec![0, if mask & 1 == 0 { 4 } else { 3 }, 7];
    if mask & 4 != 0 {
        intervals.push(11);
    } else if mask & 2 != 0 {
        intervals.push(10);
    }
    if mask & 8 != 0 {
        intervals.push(2);
    }
    intervals.sort_unstable();
    intervals
}

fn bed(bass: u8, intervals: &[u8]) -> BTreeSet<u8> {
    intervals
        .iter()
        .map(|interval| 48 + (bass + interval) % 12)
        .chain([bass])
        .collect()
}

fn harp(bass: u8, intervals: &[u8]) -> Vec<u8> {
    (0..10)
        .map(|rank| {
            60 + bass % 12 + intervals[rank % intervals.len()] + 12 * (rank / intervals.len()) as u8
        })
        .collect()
}

#[test]
fn every_root_thumb_quality_accidental_and_string_uses_the_physical_map() {
    assert_eq!(controls::KEYS, 26);
    assert_eq!(controls::CHANNEL, 0);
    assert_eq!(controls::PLUCK_MS, 500);
    for (root_pad, natural_bass) in ROOTS {
        for mask in 0..16_u8 {
            for (accidental, alter) in [(None, 0_i16), (Some(0), -1), (Some(1), 1)] {
                let mut player = Player::new();
                let mut keys = vec![root_pad];
                keys.extend(
                    (0..4)
                        .filter(|bit| mask & (1 << bit) != 0)
                        .map(|bit| 22 + bit),
                );
                keys.extend(accidental);
                let bass = (i16::from(natural_bass) + alter) as u8;
                let intervals = quality(mask);
                let events = player.play(&keys);
                assert_eq!(
                    attacks(&events, 64).into_iter().collect::<BTreeSet<_>>(),
                    bed(bass, &intervals),
                    "root {root_pad}, thumbs {mask}, accidental {accidental:?}"
                );
                assert_eq!(player.active, bed(bass, &intervals));
                assert!(attacks(&events, 100).is_empty());

                let expected = harp(bass, &intervals);
                let mut captured = Vec::new();
                for (rank, pad) in STRINGS.into_iter().enumerate() {
                    let events = player.play(&joined(&keys, &[pad]));
                    assert_eq!(attacks(&events, 100), [expected[rank]], "string pad {pad}");
                    captured.push(expected[rank]);
                    assert!(
                        player.play(&keys).is_empty(),
                        "touch release ended a harp tail"
                    );
                }
                assert!(captured.windows(2).all(|pair| pair[0] < pair[1]));
                player.at(&[], 1000, true);
                assert!(player.active.is_empty());
            }
        }
    }
}

#[test]
fn c_f_g_a_minor_sweeps_change_the_bed_and_strum_the_new_chord_in_the_same_scan() {
    let mut player = Player::new();
    for (root, bass, modifiers, intervals) in [
        (13, 36, vec![], vec![0, 4, 7]),
        (16, 41, vec![], vec![0, 4, 7]),
        (2, 43, vec![23], vec![0, 4, 7, 10]),
        (3, 45, vec![22], vec![0, 3, 7]),
    ] {
        let selectors = joined(&[root], &modifiers);
        let events = player.play(&joined(&selectors, &STRINGS));
        assert_eq!(attacks(&events, 100), harp(bass, &intervals));
        assert_eq!(
            attacks(&events, 64).into_iter().collect::<BTreeSet<_>>(),
            bed(bass, &intervals)
        );
        assert!(player.play(&selectors).is_empty());
    }
    player.at(&[], 1000, true);
    assert!(player.active.is_empty());
}

#[test]
fn the_minor_major_seventh_ninth_is_present_in_both_bed_and_harp_and_e_wins_over_o() {
    let mut player = Player::new();
    let events = player.play(&joined(&[13, 22, 23, 24, 25], &STRINGS));
    assert_eq!(
        attacks(&events, 64).into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([36, 48, 50, 51, 55, 59])
    );
    assert_eq!(
        attacks(&events, 100),
        [60, 62, 63, 67, 71, 72, 74, 75, 79, 83]
    );
    assert!(!player.active.contains(&58), "O overrode E's major seventh");
    player.play(&[13, 22, 23, 25]);
    assert!(player.active.contains(&58));
    assert!(!player.active.contains(&59));
}

#[test]
fn the_newest_held_seventh_thumb_wins_then_falls_back_to_the_older_one() {
    for (older, newer, first_seventh, second_seventh) in [(23, 24, 58, 59), (24, 23, 59, 58)] {
        let mut player = Player::new();
        player.play(&[13, older]);
        assert!(player.active.contains(&first_seventh));
        player.play(&[13, older, newer]);
        assert!(player.active.contains(&second_seventh));
        assert!(!player.active.contains(&first_seventh));
        player.play(&[13, older]);
        assert!(player.active.contains(&first_seventh));
        assert!(!player.active.contains(&second_seventh));
    }
}

#[test]
fn the_latest_held_root_wins_and_legato_release_falls_back_without_forgetting_the_harp() {
    let mut player = Player::new();
    player.play(&[13]);
    assert_eq!(player.active, bed(36, &[0, 4, 7]));
    player.play(&[13, 16]);
    assert_eq!(player.active, bed(41, &[0, 4, 7]));
    player.play(&[13, 16, 2]);
    assert_eq!(player.active, bed(43, &[0, 4, 7]));
    player.play(&[13, 16]);
    assert_eq!(player.active, bed(41, &[0, 4, 7]));
    player.play(&[13]);
    assert_eq!(player.active, bed(36, &[0, 4, 7]));
    player.play(&[]);
    assert!(player.active.is_empty());
    assert_eq!(player.play(&[17]), [on(60, 100)]);
}

#[test]
fn a_fresh_root_articulates_the_bed_without_retriggering_a_retained_harp_pitch() {
    let mut player = Player::new();
    player.play(&[13, 6, 17]);
    player.play(&[6, 17]);
    assert_eq!(player.active, BTreeSet::from([60]));
    let events = player.play(&[13, 6, 17]);
    assert_eq!(
        attacks(&events, 64).into_iter().collect::<BTreeSet<_>>(),
        bed(36, &[0, 4, 7])
    );
    assert!(!events.contains(&off(60)));
    assert!(attacks(&events, 100).is_empty());
}

#[test]
fn moving_between_c_root_registers_or_lifting_the_left_hand_preserves_identical_harp_tails() {
    let mut player = Player::new();
    player.play(&[13, 17]);
    let events = player.play(&[5]); // C2 -> C3 changes accompaniment register.
    assert!(!events.contains(&off(60)));
    assert!(player.active.contains(&60));
    assert_eq!(
        attacks(&events, 64).into_iter().collect::<BTreeSet<_>>(),
        bed(48, &[0, 4, 7])
    );
    let events = player.play(&[]);
    assert!(!events.contains(&off(60)));
    assert_eq!(player.active, BTreeSet::from([60]));
    assert_eq!(player.play(&[18]), [on(64, 100)]);
    player.at(&[], 1000, true);
    assert!(player.active.is_empty());
}

#[test]
fn plucks_release_at_the_deadline_even_if_the_pad_is_still_down_and_never_repeat_on_a_timer() {
    for held in [false, true] {
        let mut player = Player::new();
        player.at(&[13, 17], 10, true);
        let keys = if held { vec![13, 17] } else { vec![13] };
        assert!(player.at(&keys, 509, true).is_empty());
        assert_eq!(player.at(&keys, 510, true), [off(60)]);
        assert!(player.at(&keys, 5000, true).is_empty());
        assert_eq!(player.active, bed(36, &[0, 4, 7]));
        player.at(&[13], 5001, true);
        assert_eq!(player.at(&[13, 17], 5002, true), [on(60, 100)]);
    }
}

#[test]
fn a_fresh_touch_rearticulates_its_pitch_once_and_renews_the_deadline() {
    let mut player = Player::new();
    player.at(&[13, 17], 0, true);
    assert!(player.at(&[13], 100, true).is_empty());
    assert_eq!(player.at(&[13, 17], 300, true), [off(60), on(60, 100)]);
    assert!(player.at(&[13], 500, true).is_empty());
    assert_eq!(player.at(&[13], 800, true), [off(60)]);
}

#[test]
fn let_ring_retains_expired_and_old_chord_pitches_until_released() {
    let mut player = Player::new();
    player.at(&[13, 6, 17], 0, true);
    let events = player.at(&[16, 6], 100, true);
    assert!(!events.contains(&off(60)));
    assert!(player.active.contains(&60));
    assert!(player.at(&[16, 6], 900, true).is_empty());
    assert_eq!(player.at(&[16], 901, true), [off(60)]);
    assert_eq!(player.active, bed(41, &[0, 4, 7]));
}

#[test]
fn lifting_let_ring_releases_only_expired_tails() {
    let mut player = Player::new();
    player.at(&[13, 6, 17], 0, true);
    player.at(&[13, 6, 18], 400, true);
    assert_eq!(player.at(&[13], 600, true), [off(60)]);
    assert!(player.active.contains(&64));
    assert_eq!(player.at(&[13], 900, true), [off(64)]);
}

#[test]
fn damping_overrides_let_ring_and_consumes_touches_until_they_are_fresh() {
    let mut player = Player::new();
    player.play(&[13, 6, 17]);
    let events = player.play(&[13, 6, 7, 17, 18]);
    assert_eq!(events, [off(60)]);
    assert!(player.play(&[13, 6, 7, 19]).is_empty());
    assert!(player.play(&[13, 6, 19]).is_empty());
    assert!(player.play(&[13, 6]).is_empty());
    assert_eq!(player.play(&[13, 6, 19]), [on(67, 100)]);
}

#[test]
fn chord_changes_cut_old_tails_and_new_plucks_use_the_new_root_and_quality() {
    let mut player = Player::new();
    player.play(&[13, 17]);
    let events = player.play(&[16, 22, 18]);
    assert!(events.contains(&off(60)));
    assert_eq!(attacks(&events, 100), [68]); // F minor's A-flat.
    assert!(player.active.contains(&68));
    assert!(!player.active.contains(&60));
    player.play(&[16, 22]);
    let events = player.play(&[16, 22, 1, 17]);
    assert!(events.contains(&off(68)));
    assert_eq!(attacks(&events, 100), [66]); // F-sharp minor's root.
}

#[test]
fn overlapping_old_and_new_chord_tails_share_a_pitch_without_early_release() {
    let mut player = Player::new();
    player.at(&[13, 6, 20], 0, true); // C major's fourth string: C5.
    assert!(player.active.contains(&72));
    player.at(&[16, 6], 100, true); // Retain C5 across a change to F major.
    let events = player.at(&[16, 6, 19], 300, true); // F major's third string: C5.
    assert_eq!(attacks(&events, 100), [72]);
    assert_eq!(events.iter().filter(|event| **event == off(72)).count(), 1);
    let events = player.at(&[16], 600, true);
    assert!(
        !events.contains(&off(72)),
        "expired old owner cut the fresh owner"
    );
    assert!(player.active.contains(&72));
    assert_eq!(player.at(&[16], 800, true), [off(72)]);
}

#[test]
fn startup_stop_and_output_recovery_require_a_fresh_root_before_any_strum() {
    let mut fresh = Player::new();
    assert!(fresh
        .play(&joined(&[6, 22, 23, 24, 25], &STRINGS))
        .is_empty());
    assert!(fresh.active.is_empty());
    assert!(!controls::is_stop(&matrix(&[0])));
    assert!(!controls::is_stop(&matrix(&[1])));
    assert!(controls::is_stop(&matrix(&[0, 1])));
    for stop in [false, true] {
        let mut player = Player::new();
        let held = [13, 6, 22, 17];
        player.play(&held);
        let blocked = if stop {
            joined(&held, &[0, 1])
        } else {
            held.to_vec()
        };
        player.at(&blocked, 10, stop);
        assert!(player.active.is_empty());
        assert!(player.at(&held, 20, true).is_empty());
        assert!(player.at(&[13, 6, 22, 18], 30, true).is_empty());
        player.at(&[], 40, true);
        assert!(
            player.at(&[17], 50, true).is_empty(),
            "old root survived reset"
        );
        player.at(&[], 60, true);
        let events = player.at(&[16, 17], 70, true);
        assert_eq!(attacks(&events, 100), [65]);
        assert_eq!(
            attacks(&events, 64).into_iter().collect::<BTreeSet<_>>(),
            bed(41, &[0, 4, 7])
        );
    }
}

#[test]
fn a_full_six_note_bed_and_ten_string_restrike_fit_one_shared_batch() {
    let mut player = Player::new();
    let c_minor_ninth = [13, 6, 22, 24, 25];
    let first = player.play(&joined(&c_minor_ninth, &STRINGS));
    assert_eq!(first.len(), 16);
    assert_eq!(player.active.len(), 16);
    assert!(player.play(&c_minor_ninth).is_empty());
    let events = player.play(&joined(&[16, 1, 6, 24, 25], &STRINGS));
    assert_eq!(events.len(), 32);
    assert_eq!(attacks(&events, 100), harp(42, &[0, 2, 4, 7, 11]));
    assert_eq!(player.active.len(), 16);
    assert_eq!(player.play(&[0, 1]).len(), 16);
    assert!(player.active.is_empty());
}

#[test]
fn dense_touch_changes_and_recovery_never_exceed_the_shared_batch_or_leak_note_owners() {
    let mut player = Player::new();
    let mut state = 0x17ac_935d_u32;
    for tick in 1..=1500_u64 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let keys: Vec<_> = (0..26).filter(|bit| state & (1 << bit) != 0).collect();
        player.at(&keys, tick * 17, tick % 59 != 0);
    }
    player.at(&[], 26_000, true);
    player.at(&[], 27_000, true);
    assert!(player.active.is_empty());
}
