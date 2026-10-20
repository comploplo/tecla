#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test assertions")]

#[path = "../../../keyboards/tecla/asterisk_v1_4_chromatica/controls.rs"]
mod controls;

use std::collections::BTreeSet;

use tecla::matrix::capacitive::CapacitiveFilter;
use tecla::matrix::MatrixState;
use tecla::pipeline::report::MidiEvent;

// Independent oracle from the solo-tuning chart and physical steno labels.
// Left lower S/K/W/R, left upper S/T/P/H; right lower R/B/G/S/Z, upper F/P/L/T/D.
const HOLES: [(usize, u8, u8); 18] = [
    (13, 48, 50),
    (14, 52, 53),
    (15, 55, 57),
    (16, 60, 59),
    (2, 60, 62),
    (3, 64, 65),
    (4, 67, 69),
    (5, 72, 71),
    (17, 60, 62),
    (18, 64, 65),
    (19, 67, 69),
    (20, 72, 71),
    (21, 72, 74),
    (8, 72, 74),
    (9, 76, 77),
    (10, 79, 81),
    (11, 84, 83),
    (12, 84, 86),
];
const MODIFIERS: [usize; 8] = [0, 1, 6, 7, 22, 23, 24, 25];

fn matrix(keys: &[usize]) -> MatrixState<1, 26> {
    let mut stable = MatrixState::new();
    for &key in keys {
        assert!(stable.set(0, key, true));
    }
    stable
}

fn on(note: u8) -> MidiEvent {
    MidiEvent::NoteOn {
        channel: 0,
        note,
        velocity: 100,
    }
}

fn off(note: u8) -> MidiEvent {
    MidiEvent::NoteOff { channel: 0, note }
}

fn modifier_keys(mask: u8) -> Vec<usize> {
    MODIFIERS
        .into_iter()
        .enumerate()
        .filter_map(|(bit, pad)| (mask & (1 << bit) != 0).then_some(pad))
        .collect()
}

fn pitch(blow: u8, draw: u8, keys: &[usize]) -> u8 {
    let inhale = keys.contains(&6) || keys.contains(&7);
    let sharp = keys.contains(&23) || keys.contains(&24);
    let flat = keys.contains(&22) || keys.contains(&25);
    let shift = i16::from(sharp) - i16::from(flat)
        + 12 * (i16::from(keys.contains(&1)) - i16::from(keys.contains(&0)));
    u8::try_from(i16::from(if inhale { draw } else { blow }) + shift).unwrap()
}

fn pitches(keys: &[usize]) -> BTreeSet<u8> {
    if keys.contains(&0) && keys.contains(&1) {
        return BTreeSet::new();
    }
    HOLES
        .into_iter()
        .filter(|(pad, _, _)| keys.contains(pad))
        .map(|(_, blow, draw)| pitch(blow, draw, keys))
        .collect()
}

struct Player {
    instrument: controls::Controls,
    active: BTreeSet<u8>,
}

impl Player {
    fn new() -> Self {
        Self {
            instrument: controls::Controls::new(),
            active: BTreeSet::new(),
        }
    }

    fn stable(&mut self, stable: &MatrixState<1, 26>, ready: bool) -> Vec<MidiEvent> {
        let mut events = Vec::new();
        self.instrument.update(stable, ready, &mut |event| {
            assert!(events.len() < 36, "scan exceeded MIDI event bound");
            events.push(event);
        });
        let mut attacked = false;
        for &event in &events {
            match event {
                MidiEvent::NoteOff { channel, note } => {
                    assert_eq!(channel, 0);
                    assert!(!attacked, "release followed an attack: {events:?}");
                    assert!(self.active.remove(&note), "released unowned pitch {note}");
                }
                MidiEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                } => {
                    assert_eq!(channel, 0);
                    assert_eq!(velocity, 100);
                    assert!(note <= 127);
                    attacked = true;
                    assert!(self.active.insert(note), "duplicate attack {note}");
                }
                _ => panic!("unexpected MIDI message {event:?}"),
            }
        }
        events
    }

    fn frame(&mut self, keys: &[usize], ready: bool) -> Vec<MidiEvent> {
        self.stable(&matrix(keys), ready)
    }

    fn play(&mut self, keys: &[usize]) -> Vec<MidiEvent> {
        self.frame(keys, true)
    }
}

#[test]
fn every_physical_hole_and_modifier_combination_matches_solo_tuning() {
    assert_eq!(controls::KEYS, 26);
    assert_eq!(controls::CHANNEL, 0);
    assert_eq!(controls::VELOCITY, 100);
    assert_eq!(
        HOLES
            .iter()
            .map(|hole| hole.0)
            .collect::<BTreeSet<_>>()
            .len(),
        18
    );
    for (pad, blow, draw) in HOLES {
        for mask in 0..=u8::MAX {
            let mut player = Player::new();
            let mut keys = modifier_keys(mask);
            keys.push(pad);
            let events = player.play(&keys);
            let stopped = keys.contains(&0) && keys.contains(&1);
            assert_eq!(controls::is_stop(&matrix(&keys)), stopped);
            if stopped {
                assert!(events.is_empty());
            } else {
                let note = pitch(blow, draw, &keys);
                assert_eq!(events, [on(note)], "pad {pad}, modifier mask {mask}");
                assert_eq!(player.play(&[]), [off(note)]);
            }
            assert!(player.active.is_empty());
        }
    }
}

#[test]
fn every_modifier_combination_revoices_an_already_held_polyphonic_keyboard() {
    let letters: Vec<_> = HOLES.iter().map(|hole| hole.0).collect();
    for mask in 0..=u8::MAX {
        let modifiers = modifier_keys(mask);
        if modifiers.contains(&0) && modifiers.contains(&1) {
            continue;
        }
        let mut player = Player::new();
        player.play(&letters);
        let original = player.active.clone();
        let mut keys = letters.clone();
        keys.extend(modifiers);
        let events = player.play(&keys);
        let expected = pitches(&keys);
        assert_eq!(player.active, expected, "modifier mask {mask}");
        let released: BTreeSet<_> = events
            .iter()
            .filter_map(|event| match event {
                MidiEvent::NoteOff { note, .. } => Some(*note),
                _ => None,
            })
            .collect();
        let started: BTreeSet<_> = events
            .iter()
            .filter_map(|event| match event {
                MidiEvent::NoteOn { note, .. } => Some(*note),
                _ => None,
            })
            .collect();
        assert_eq!(released, original.difference(&expected).copied().collect());
        assert_eq!(started, expected.difference(&original).copied().collect());
        assert!(
            player.play(&keys).is_empty(),
            "idle scan rearticulated notes"
        );
        player.play(&letters);
        assert_eq!(player.active, original);
        player.play(&[]);
        assert!(player.active.is_empty());
    }
}

#[test]
fn one_four_hole_manual_plays_a_complete_chromatic_octave() {
    let phrase: &[(u8, &[usize])] = &[
        (48, &[13]),        // S lower: C3.
        (49, &[13, 23]),    // O slide: C#3.
        (50, &[13, 6]),     // Inhale: D3.
        (51, &[13, 6, 23]), // Inhale + slide: Eb3.
        (52, &[14]),        // K: E3.
        (53, &[14, 6]),     // F3.
        (54, &[14, 6, 23]), // F#3.
        (55, &[15]),        // W: G3.
        (56, &[15, 23]),    // Ab3.
        (57, &[15, 6]),     // A3.
        (58, &[15, 6, 23]), // Bb3.
        (59, &[16, 6]),     // R inhaled: B3.
        (60, &[16]),        // Exhale raises this particular hole to C4.
    ];
    let mut player = Player::new();
    let mut previous = None;
    for &(note, keys) in phrase {
        let events = player.play(keys);
        let expected: Vec<_> = previous.into_iter().map(off).chain([on(note)]).collect();
        assert_eq!(events, expected);
        assert_eq!(player.active, BTreeSet::from([note]));
        previous = Some(note);
    }
    assert_eq!(player.play(&[]), [off(60)]);
}

#[test]
fn breathing_a_held_chord_repeats_without_losing_notes_or_accumulating_history() {
    let mut player = Player::new();
    let exhale = [13, 14, 15, 16];
    player.play(&exhale);
    assert_eq!(player.active, BTreeSet::from([48, 52, 55, 60]));
    for _ in 0..128 {
        player.play(&[13, 14, 15, 16, 6]);
        assert_eq!(player.active, BTreeSet::from([50, 53, 57, 59]));
        assert!(player.play(&[13, 14, 15, 16, 6, 7]).is_empty());
        assert!(player.play(&[13, 14, 15, 16, 7]).is_empty());
        player.play(&exhale);
        assert_eq!(player.active, BTreeSet::from([48, 52, 55, 60]));
    }
    player.play(&[]);
    assert!(player.active.is_empty());
}

#[test]
fn mirrored_slide_buttons_handover_and_opposing_directions_cancel() {
    let mut player = Player::new();
    assert_eq!(player.play(&[13]), [on(48)]);
    assert_eq!(player.play(&[13, 23]), [off(48), on(49)]);
    assert!(player.play(&[13, 23, 24]).is_empty());
    assert!(player.play(&[13, 24]).is_empty());
    assert_eq!(player.play(&[13, 22, 24]), [off(49), on(48)]);
    assert!(player.play(&[13, 22, 23, 24, 25]).is_empty());
    assert_eq!(player.play(&[13, 22, 25]), [off(48), on(47)]);
    assert!(player.play(&[13, 25]).is_empty());
    assert_eq!(player.play(&[13]), [off(47), on(48)]);
    // Applying two opposite controls in one coherent scan changes nothing.
    assert!(player.play(&[13, 22, 23]).is_empty());
    assert!(player.play(&[13]).is_empty());
}

#[test]
fn number_bars_move_held_notes_and_both_bars_override_every_modifier() {
    let mut player = Player::new();
    assert_eq!(player.play(&[13]), [on(48)]);
    assert_eq!(player.play(&[13, 0]), [off(48), on(36)]);
    assert_eq!(player.play(&[13, 1]), [off(36), on(60)]);
    assert_eq!(player.play(&[13, 1, 7, 24]), [off(60), on(63)]);
    assert_eq!(player.play(&[13, 0, 1, 7, 24]), [off(63)]);
    assert!(player.play(&[13]).is_empty());
    assert!(player.play(&[]).is_empty());
    assert_eq!(player.play(&[13]), [on(48)]);
}

#[test]
fn overlapping_fingers_preserve_polyphony_and_duplicate_presses_rearticulate_once() {
    let mut player = Player::new();
    assert_eq!(player.play(&[13]), [on(48)]);
    assert_eq!(player.play(&[13, 14]), [on(52)]);
    assert_eq!(player.play(&[14]), [off(48)]);
    assert_eq!(player.play(&[14, 16]), [on(60)]);
    assert_eq!(player.play(&[14, 16, 2]), [off(60), on(60)]);
    assert!(player.play(&[14, 2]).is_empty());
    assert_eq!(player.play(&[14, 2, 17]), [off(60), on(60)]);
    assert!(player.play(&[14, 17]).is_empty());
    assert_eq!(player.play(&[14]), [off(60)]);
    assert_eq!(player.play(&[]), [off(52)]);
}

#[test]
fn shared_blow_pitches_split_into_distinct_draw_notes_without_cutting_other_owners() {
    let mut player = Player::new();
    assert_eq!(player.play(&[20, 21]), [on(72)]);
    player.play(&[20, 21, 6]);
    assert_eq!(player.active, BTreeSet::from([71, 74]));
    assert_eq!(player.play(&[21, 6]), [off(71)]);
    assert_eq!(player.play(&[21]), [off(74), on(72)]);
    assert_eq!(player.play(&[20, 21]), [off(72), on(72)]);
    assert!(player.play(&[20]).is_empty());
    assert_eq!(player.play(&[]), [off(72)]);
}

#[test]
fn every_note_and_control_requires_its_own_fresh_press_after_stop_or_usb_recovery() {
    for stop in [false, true] {
        for held in 0..26 {
            let mut player = Player::new();
            player.play(&[held]);
            if stop {
                player.play(&[held, 0, 1]);
            } else {
                player.frame(&[held], false);
            }
            assert!(player.active.is_empty());
            assert!(player.play(&[held]).is_empty());

            let fresh = if held == 13 { 14 } else { 13 };
            player.play(&[held, fresh]);
            assert_eq!(
                player.active,
                pitches(&[fresh]),
                "held pad {held}, stop {stop}"
            );
            assert!(player.play(&[fresh]).is_empty());
            player.play(&[fresh, held]);
            assert_eq!(player.active, pitches(&[fresh, held]));
            player.play(&[]);
            assert!(player.active.is_empty());
        }
    }
}

#[test]
fn blocked_breath_and_slide_do_not_change_new_notes_until_each_is_repressed() {
    let mut player = Player::new();
    player.play(&[13, 6, 23]);
    assert_eq!(player.active, BTreeSet::from([51]));
    player.frame(&[13, 6, 23], false);
    assert!(player.play(&[13, 6, 23]).is_empty());
    assert_eq!(player.play(&[13, 14, 6, 23]), [on(52)]);
    assert!(player.play(&[13, 14, 23]).is_empty());
    assert_eq!(player.play(&[13, 14, 6, 23]), [off(52), on(53)]);
    assert!(player.play(&[13, 14, 6]).is_empty());
    assert_eq!(player.play(&[13, 14, 6, 23]), [off(53), on(54)]);
    assert!(player.play(&[14, 6, 23]).is_empty());
    assert_eq!(player.play(&[13, 14, 6, 23]), [on(51)]);
    assert_eq!(player.active, BTreeSet::from([51, 54]));
}

#[test]
fn capacitive_taps_chatter_star_handover_and_finger_overlap_keep_the_phrase_intact() {
    let mut player = Player::new();
    let mut filter = CapacitiveFilter::new([100; 26], [0; 26]);
    let mut sample = |keys: &[usize]| {
        let counts = core::array::from_fn(|pad| if keys.contains(&pad) { 160 } else { 100 });
        let touched = filter.update(&counts);
        let mut stable = MatrixState::new();
        for (pad, pressed) in touched.into_iter().enumerate() {
            assert!(stable.set(0, pad, pressed));
        }
        player.stable(&stable, true)
    };
    assert!(sample(&[13]).is_empty());
    assert_eq!(sample(&[13]), [on(48)]);
    assert!(sample(&[13, 6]).is_empty());
    assert!(sample(&[13]).is_empty()); // One-scan star noise is not an inhale.
    assert!(sample(&[13, 6]).is_empty());
    assert_eq!(sample(&[13, 6]), [off(48), on(50)]);
    for keys in [&[13, 6, 7][..], &[13, 7][..]] {
        assert!(sample(keys).is_empty());
        assert!(sample(keys).is_empty());
    }
    assert!(sample(&[13, 7, 23]).is_empty());
    assert_eq!(sample(&[13, 7, 23]), [off(50), on(51)]);
    assert!(sample(&[13, 14, 7, 23]).is_empty());
    assert_eq!(sample(&[13, 14, 7, 23]), [on(54)]);
    assert!(sample(&[14, 7, 23]).is_empty());
    assert_eq!(sample(&[14, 7, 23]), [off(51)]);
    assert!(sample(&[7, 23]).is_empty());
    assert!(sample(&[14, 7, 23]).is_empty()); // Brief contact gap stays legato.
    assert!(sample(&[7, 23]).is_empty());
    assert_eq!(sample(&[7, 23]), [off(54)]);
    assert!(sample(&[14, 7, 23]).is_empty());
    assert_eq!(sample(&[14, 7, 23]), [on(54)]);
    assert!(sample(&[]).is_empty());
    assert_eq!(sample(&[]), [off(54)]);
    assert!(player.active.is_empty());
}

#[test]
fn dense_phrases_match_an_independent_ownership_and_recovery_oracle() {
    let mut player = Player::new();
    let mut previous = BTreeSet::new();
    let mut blocked = BTreeSet::new();
    let mut random = 0x35a7_12b9_u32;
    for step in 0..2400 {
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut keys: Vec<_> = (0..26).filter(|pad| random & (1 << pad) != 0).collect();
        if step % 31 == 0 {
            keys.extend(HOLES.iter().map(|hole| hole.0));
            keys.sort_unstable();
            keys.dedup();
        }
        let held: BTreeSet<_> = keys.iter().copied().collect();
        let ready = step % 79 != 0;
        let suppressed = !ready || (held.contains(&0) && held.contains(&1));
        if suppressed {
            blocked = held.clone();
        } else {
            blocked.retain(|pad| held.contains(pad));
        }
        let accepted: Vec<_> = held.difference(&blocked).copied().collect();
        let desired = if suppressed {
            BTreeSet::new()
        } else {
            pitches(&accepted)
        };
        let retriggered: BTreeSet<_> = HOLES
            .iter()
            .filter(|(pad, _, _)| accepted.contains(pad) && !previous.contains(pad))
            .map(|(_, blow, draw)| pitch(*blow, *draw, &accepted))
            .collect();
        let before = player.active.clone();
        let events = player.frame(&keys, ready);
        assert_eq!(
            player.active, desired,
            "frame {step}: {keys:?}, ready {ready}"
        );
        let offs: BTreeSet<_> = events
            .iter()
            .filter_map(|event| match event {
                MidiEvent::NoteOff { note, .. } => Some(*note),
                _ => None,
            })
            .collect();
        let ons: BTreeSet<_> = events
            .iter()
            .filter_map(|event| match event {
                MidiEvent::NoteOn { note, .. } => Some(*note),
                _ => None,
            })
            .collect();
        let expected_offs = before
            .iter()
            .filter(|note| !desired.contains(note) || retriggered.contains(note))
            .copied()
            .collect::<BTreeSet<_>>();
        let expected_ons = desired
            .iter()
            .filter(|note| !before.contains(note) || retriggered.contains(note))
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(offs, expected_offs, "frame {step}: releases");
        assert_eq!(ons, expected_ons, "frame {step}: attacks");
        previous = held;
    }
    player.play(&[]);
    assert!(player.active.is_empty());
}
