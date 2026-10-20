#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test assertions")]

// Exercise the physical firmware mapper and the actual musical program together.
#[path = "../../../keyboards/tecla/asterisk_v1_4_autochord/controls.rs"]
mod controls;

use std::collections::BTreeSet;
use tecla::matrix::capacitive::CapacitiveFilter;
use tecla::matrix::MatrixState;
use tecla::music::autochord::AutoChord;
use tecla::music::Key;
use tecla::pipeline::report::MidiEvent;

fn controller() -> controls::Controls {
    controls::Controls::new(
        AutoChord::new(Key::C_MAJOR, controls::CHANNEL, controls::VELOCITY).unwrap(),
    )
}

fn frame(controller: &mut controls::Controls, pressed: &[usize], ready: bool) -> Vec<MidiEvent> {
    let mut stable = MatrixState::<1, { controls::KEYS }>::new();
    for &col in pressed {
        assert!(stable.set(0, col, true));
    }
    let mut events = Vec::new();
    controller
        .update(&stable, ready, &mut |event| events.push(event))
        .unwrap();
    events
}

fn frame_at(
    controller: &mut controls::Controls,
    pressed: &[usize],
    now_ms: u64,
    ready: bool,
) -> Vec<MidiEvent> {
    let mut stable = MatrixState::<1, { controls::KEYS }>::new();
    for &col in pressed {
        assert!(stable.set(0, col, true));
    }
    let mut events = Vec::new();
    controller
        .update_at(&stable, now_ms, ready, &mut |event| events.push(event))
        .unwrap();
    events
}

fn counts(pressed: &[usize]) -> [u32; controls::KEYS] {
    let mut counts = [100; controls::KEYS];
    for &col in pressed {
        counts[col] = 160;
    }
    counts
}

fn raw_frame(
    controller: &mut controls::Controls,
    filter: &mut CapacitiveFilter<{ controls::KEYS }>,
    counts: &[u32; controls::KEYS],
) -> Vec<MidiEvent> {
    let mut stable = MatrixState::<1, { controls::KEYS }>::new();
    for (col, pressed) in filter.update(counts).into_iter().enumerate() {
        assert!(stable.set(0, col, pressed));
    }
    let mut events = Vec::new();
    controller
        .update(&stable, true, &mut |event| events.push(event))
        .unwrap();
    events
}

fn note_on(note: u8) -> MidiEvent {
    MidiEvent::NoteOn {
        channel: controls::CHANNEL,
        note,
        velocity: controls::VELOCITY,
    }
}

fn note_off(note: u8) -> MidiEvent {
    MidiEvent::NoteOff {
        channel: controls::CHANNEL,
        note,
    }
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
                assert_eq!(channel, controls::CHANNEL);
                assert_eq!(velocity, controls::VELOCITY);
                attacked = true;
                assert!(note <= 127);
                assert!(active.insert(note), "duplicate attack for {note}");
            }
            MidiEvent::NoteOff { channel, note } => {
                assert!(!attacked, "note-off must precede all new attacks");
                assert_eq!(channel, controls::CHANNEL);
                assert!(active.remove(&note), "release without an attack for {note}");
            }
            _ => panic!("unexpected controller message: {event:?}"),
        }
    }
}

fn assert_no_pitch_events(events: &[MidiEvent], pitches: &[u8]) {
    for event in events {
        match event {
            MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } => {
                assert!(
                    !pitches.contains(note),
                    "held note was articulated: {event:?}"
                );
            }
            _ => panic!("unexpected controller message: {event:?}"),
        }
    }
}

fn selected_pitches(pads: &[usize]) -> Vec<u8> {
    let bass = controls::BASSES
        .iter()
        .filter_map(|(col, note)| pads.contains(col).then_some(*note))
        .min();
    let melody = controls::MELODIES
        .iter()
        .filter_map(|(col, note)| pads.contains(col).then_some(*note))
        .max();
    bass.into_iter().chain(melody).collect()
}

fn assert_selected_held(active: &BTreeSet<u8>, pads: &[usize]) {
    for note in selected_pitches(pads) {
        assert!(
            active.contains(&note),
            "selected note {note} missing for {pads:?}"
        );
    }
}

#[test]
fn every_note_pad_sounds_its_pitch_while_held_and_releases_exactly() {
    for &(col, note) in controls::BASSES.iter().chain(&controls::MELODIES) {
        let mut controller = controller();
        assert!(Key::C_MAJOR.contains(note));
        assert_eq!(frame(&mut controller, &[col], true), [note_on(note)]);
        assert!(frame(&mut controller, &[col], true).is_empty());
        assert_eq!(frame(&mut controller, &[], true), [note_off(note)]);
    }
}

#[test]
fn either_number_pad_alone_preserves_the_played_pitch() {
    for modifier in controls::NUMBER_COLS {
        for (col, note) in [(13, 36), (17, 60)] {
            let mut controller = controller();
            assert_eq!(
                frame(&mut controller, &[col, modifier], true),
                [note_on(note)]
            );
            assert!(frame(&mut controller, &[col], true).is_empty());
            assert!(frame(&mut controller, &[col, modifier], true).is_empty());
            assert_eq!(frame(&mut controller, &[], true), [note_off(note)]);
        }
    }
}

#[test]
fn rolling_fingers_preserves_boundary_notes_then_hands_off_when_they_are_released() {
    for pads in [[13, 14, 15], [17, 18, 19]] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        let [a, b, c_pad] = pads;
        for held in [
            vec![a],
            vec![a, b],
            vec![a, b, c_pad],
            vec![a, c_pad],
            vec![c_pad],
            vec![],
        ] {
            apply(&mut active, &frame(&mut c, &held, true));
            assert_eq!(active, selected_pitches(&held).into_iter().collect());
            assert!(frame(&mut c, &held, true).is_empty());
        }
        assert!(active.is_empty());
    }
}

#[test]
fn simultaneous_selectors_and_modifier_use_one_coherent_snapshot() {
    for (pressed, expected) in [
        (vec![2, 16, controls::NUMBER_COLS[0]], vec![41]),
        (vec![8, 21, controls::NUMBER_COLS[1]], vec![69]),
    ] {
        let mut controller = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut controller, &pressed, true));
        assert_eq!(active, expected.into_iter().collect());
        assert!(frame(&mut controller, &pressed, true).is_empty());
        apply(&mut active, &frame(&mut controller, &[], true));
        assert!(active.is_empty());
    }
    let mut controller = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut controller, &[13, 17], true));
    assert_eq!(active.len(), 4);
    assert!(active.contains(&36) && active.contains(&60));
    assert!(frame(&mut controller, &[13, 17], true).is_empty());
    apply(&mut active, &frame(&mut controller, &[13], true));
    assert_eq!(active, BTreeSet::from([36]));
    apply(&mut active, &frame(&mut controller, &[], true));
    assert!(active.is_empty());
}

#[test]
fn every_one_hand_shape_sounds_its_selected_boundary_pitch_only() {
    for bank in [&controls::BASSES[..], &controls::MELODIES[..]] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        for shape in 0..(1_u32 << bank.len()) {
            let held: Vec<_> = bank
                .iter()
                .enumerate()
                .filter_map(|(bit, (col, _))| (shape & (1 << bit) != 0).then_some(*col))
                .collect();
            apply(&mut active, &frame(&mut c, &held, true));
            assert_eq!(active, selected_pitches(&held).into_iter().collect());
            assert!(frame(&mut c, &held, true).is_empty());
        }
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn thumb_bars_and_stars_keep_both_selected_pitches_and_attacks_intact_with_extra_keys() {
    for letters in [[13, 15, 19, 21], [14, 16, 17, 10], [2, 5, 18, 12]] {
        for stars in [vec![], vec![6], vec![7], vec![6, 7]] {
            let mut c = controller();
            let mut active = BTreeSet::new();
            let mut base = letters.to_vec();
            base.extend(&stars);
            apply(&mut active, &frame(&mut c, &base, true));
            assert_selected_held(&active, &letters);
            for command in [0, 1, 22, 23, 24, 25] {
                let mut held = base.clone();
                held.push(command);
                let events = frame(&mut c, &held, true);
                assert_no_pitch_events(&events, &selected_pitches(&letters));
                apply(&mut active, &events);
                assert_selected_held(&active, &letters);
                assert_eq!(active.len(), 4);
                assert!(frame(&mut c, &held, true).is_empty());
                assert!(frame(&mut c, &base, true).is_empty());
            }
            apply(&mut active, &frame(&mut c, &[], true));
            assert!(active.is_empty());
        }
    }
}

#[test]
fn extra_left_hand_tones_choose_different_harmonies_between_the_same_held_anchors() {
    for (pad, expected, after_release) in [
        (3, [36, 57, 60, 64], [36, 57, 60, 64]), // Upper T suggests Am/C.
        (4, [36, 55, 59, 64], [36, 55, 60, 64]), // Upper P suggests Cmaj7.
    ] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut c, &[13, 19], true)); // Lower S/-G: C2/E4.
        assert_eq!(active, BTreeSet::from([36, 55, 60, 64]));
        let added = frame(&mut c, &[13, 19, pad], true);
        assert_no_pitch_events(&added, &[36, 64]);
        apply(&mut active, &added);
        assert_eq!(active, BTreeSet::from(expected));
        assert!(frame(&mut c, &[13, 19, pad], true).is_empty());
        let released = frame(&mut c, &[13, 19], true);
        assert_no_pitch_events(&released, &[36, 64]);
        apply(&mut active, &released);
        assert_eq!(active, BTreeSet::from(after_release));
        assert!(frame(&mut c, &[13, 19], true).is_empty());
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn a_right_hand_hint_guides_harmony_then_becomes_literal_when_the_top_pad_is_lifted() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 12], true)); // C2/E5.
    assert_eq!(active, BTreeSet::from([36, 55, 64, 76]));
    let added = frame(&mut c, &[13, 12, 8], true); // A4 below E5 suggests Am/C.
    assert_no_pitch_events(&added, &[36, 76]);
    apply(&mut active, &added);
    assert_eq!(active, BTreeSet::from([36, 57, 64, 76]));
    assert!(
        !active.contains(&69),
        "the hint must not sound as a manual note"
    );
    let handed_off = frame(&mut c, &[13, 8], true);
    assert_no_pitch_events(&handed_off, &[36]);
    assert!(handed_off.contains(&note_off(76)) && handed_off.contains(&note_on(69)));
    apply(&mut active, &handed_off);
    assert_eq!(active.len(), 4);
    assert!(active.contains(&36) && active.contains(&69));
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn a_guidance_key_sharing_a_generated_pitch_does_not_reattack_it() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 21], true)); // C2/G4, generated E4.
    assert_eq!(active, BTreeSet::from([36, 55, 64, 67]));
    let added = frame(&mut c, &[13, 19, 21], true); // E4 is below held G4: guidance.
    assert_no_pitch_events(&added, &[36, 64, 67]);
    apply(&mut active, &added);
    assert_eq!(active, BTreeSet::from([36, 55, 64, 67]));
    let released = frame(&mut c, &[13, 21], true);
    assert_no_pitch_events(&released, &[36, 64, 67]);
    apply(&mut active, &released);
    assert_eq!(active, BTreeSet::from([36, 55, 64, 67]));
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn shape_streams_fit_the_existing_midi_batch_and_stop_releases_all_owners() {
    let pads: Vec<_> = controls::BASSES
        .iter()
        .chain(&controls::MELODIES)
        .copied()
        .collect();
    let mut c = controller();
    let mut active = BTreeSet::new();
    let mut random = 0x49ed_10e5_u32;
    for step in 0..256 {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        let mut held: Vec<_> = pads
            .iter()
            .enumerate()
            .filter_map(|(bit, (col, _))| (random & (1 << bit) != 0).then_some(*col))
            .collect();
        // Include star palettes and actual command edges in dense shape changes.
        if step % 3 == 0 {
            held.push(6);
        }
        if step % 4 == 0 {
            held.push(7);
        }
        if step % 2 == 0 {
            held.push(22 + step % 4);
        }
        let events = frame(&mut c, &held, true);
        assert!(
            events.len() <= 8,
            "batch overflow at shape {held:?}: {events:?}"
        );
        apply(&mut active, &events);
        assert_selected_held(&active, &held);
        assert!(active.len() <= 4);
    }
    let mut all: Vec<_> = pads.iter().map(|(col, _)| *col).collect();
    let events = frame(&mut c, &all, true);
    assert!(events.len() <= 8);
    apply(&mut active, &events);
    assert_selected_held(&active, &all);
    assert_eq!(active.len(), 4);
    all.extend([0, 1]);
    let stopped = frame(&mut c, &all, true);
    assert!(stopped.len() <= 4);
    assert!(stopped
        .iter()
        .all(|event| matches!(event, MidiEvent::NoteOff { .. })));
    apply(&mut active, &stopped);
    assert!(active.is_empty());
    assert!(frame(&mut c, &all[..18], true).is_empty());
}

#[test]
fn multikey_recovery_requires_each_held_pad_to_be_rearmed_independently() {
    let mut c = controller();
    let held = [13, 15, 19, 21];
    assert!(frame(&mut c, &held, false).is_empty());
    assert!(frame(&mut c, &held, true).is_empty());
    assert!(frame(&mut c, &[15, 19, 21], true).is_empty());
    assert_eq!(frame(&mut c, &held, true), [note_on(36)]);
    assert!(frame(&mut c, &[13, 15, 21], true).is_empty());
    let mut active = BTreeSet::from([36]);
    apply(&mut active, &frame(&mut c, &held, true));
    assert!(active.contains(&36) && active.contains(&64));
    assert!(!active.contains(&40) && !active.contains(&67));
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn offline_and_recovery_require_fresh_note_attacks_in_both_banks() {
    for (pressed, voices) in [(vec![13], 1), (vec![17], 1), (vec![13, 17], 4)] {
        let mut controller = controller();
        assert!(frame(&mut controller, &pressed, false).is_empty());
        assert!(frame(&mut controller, &pressed, true).is_empty());
        assert!(frame(&mut controller, &[], true).is_empty());
        assert_eq!(frame(&mut controller, &pressed, true).len(), voices);
        // The transport owns recovery's Note Off sweep; the mapper resets silently.
        assert!(frame(&mut controller, &pressed, false).is_empty());
        assert!(frame(&mut controller, &pressed, true).is_empty());
        assert!(frame(&mut controller, &[], true).is_empty());
        assert_eq!(frame(&mut controller, &pressed, true).len(), voices);
    }
}

#[test]
fn both_number_pads_stop_before_commands_and_require_fresh_note_attacks() {
    let mut fresh = controller();
    let mut controller = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut controller, &[13, 17], true));
    let stopped = frame(
        &mut controller,
        &[13, 17, 0, 1, controls::SUBSTITUTE_COL],
        true,
    );
    assert!(stopped
        .iter()
        .all(|event| matches!(event, MidiEvent::NoteOff { .. })));
    apply(&mut active, &stopped);
    assert!(active.is_empty());
    assert!(frame(&mut controller, &[13, 17], true).is_empty());
    // The transport owns Stop's sweep; a fresh melody cannot revive old bass.
    assert!(frame(&mut controller, &[13], true).is_empty());
    assert_eq!(frame(&mut controller, &[13, 17], true), [note_on(60)]);
    assert_eq!(frame(&mut controller, &[], true), [note_off(60)]);
    assert_eq!(frame(&mut controller, &[13, 17], true).len(), 4);
    assert!(frame(&mut fresh, &[13, 17, 0, 1, controls::RESOLVE_COL], true).is_empty());
    assert!(frame(&mut fresh, &[13, 17], true).is_empty());
}

#[test]
fn stop_and_usb_recovery_do_not_restore_held_stars_thumbs_or_bars() {
    for control in [0, 1, 6, 7, 22, 23, 24, 25] {
        for stop in [false, true] {
            let mut c = controller();
            frame(&mut c, &[13, 19, control], true);
            if stop {
                frame(&mut c, &[13, 19, control, 0, 1], true);
            } else {
                // USB recovery owns the note-off sweep; the mapper forgets the phrase.
                frame(&mut c, &[13, 19, control], false);
            }
            assert!(frame(&mut c, &[13, 19, control], true).is_empty());
            assert!(frame(&mut c, &[control], true).is_empty());
            let mut fresh = controller();
            for notes in [[16, 10], [13, 19]] {
                let expected = frame(&mut fresh, &notes, true);
                let actual = frame(&mut c, &[notes[0], notes[1], control], true);
                assert_eq!(actual, expected, "held control {control}, stop={stop}");
            }
            assert!(frame(&mut c, &[13, 19], true).is_empty());
        }
    }
}

#[test]
fn raw_short_note_taps_and_threshold_jitter_do_not_duplicate_or_stick_notes() {
    let mut controller = controller();
    let mut filter = CapacitiveFilter::new([100; controls::KEYS], [0; controls::KEYS]);
    let mut raw = counts(&[]);
    // Isolated excursions above the press threshold never become a note.
    for level in [131, 100, 132, 130, 150, 100] {
        raw[13] = level;
        assert!(raw_frame(&mut controller, &mut filter, &raw).is_empty());
    }
    raw[13] = 160;
    assert!(raw_frame(&mut controller, &mut filter, &raw).is_empty());
    assert_eq!(raw_frame(&mut controller, &mut filter, &raw), [note_on(36)]);
    // A release spike followed by a deadband sample cannot split a held note.
    for level in [114, 120, 132, 116, 160] {
        raw[13] = level;
        assert!(raw_frame(&mut controller, &mut filter, &raw).is_empty());
    }
    raw[13] = 100;
    assert!(raw_frame(&mut controller, &mut filter, &raw).is_empty());
    assert_eq!(
        raw_frame(&mut controller, &mut filter, &raw),
        [note_off(36)]
    );

    // Two-sample taps with two-sample gaps remain distinct end to end.
    for _ in 0..3 {
        raw[13] = 160;
        assert!(raw_frame(&mut controller, &mut filter, &raw).is_empty());
        assert_eq!(raw_frame(&mut controller, &mut filter, &raw), [note_on(36)]);
        raw[13] = 100;
        assert!(raw_frame(&mut controller, &mut filter, &raw).is_empty());
        assert_eq!(
            raw_frame(&mut controller, &mut filter, &raw),
            [note_off(36)]
        );
    }
}

#[test]
fn connected_note_slides_keep_four_voices() {
    for (start, finish, bass, melody) in [
        ([14, 18], [15, 18], 40, 62), // D2/D4 to E2/D4.
        ([13, 10], [14, 11], 38, 74), // Both hands: C2/C5 to D2/D5.
    ] {
        let mut controller = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut controller, &start, true));
        assert_eq!(active.len(), 4);
        apply(&mut active, &frame(&mut controller, &finish, true));
        assert_eq!(active.len(), 4, "slide {start:?} -> {finish:?}");
        assert!(active.contains(&bass) && active.contains(&melody));
        apply(&mut active, &frame(&mut controller, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn each_hash_bar_reinterprets_harmony_directly_and_its_release_is_quiet() {
    for bar in controls::NUMBER_COLS {
        let mut c = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut c, &[13, 19], true));
        for _ in 0..4 {
            let before = active.clone();
            let events = frame(&mut c, &[13, 19, bar], true);
            apply(&mut active, &events);
            assert_ne!(active, before, "bar {bar} did not reinterpret C2/E4");
            assert_eq!(active.len(), 4);
            assert!(active.contains(&36) && active.contains(&64));
            assert!(frame(&mut c, &[13, 19, bar], true).is_empty());
            assert!(frame(&mut c, &[13, 19], true).is_empty());
        }
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn every_pair_keeps_both_played_pitches_under_direct_bars_and_all_thumbs() {
    for &(bass, bass_pitch) in &controls::BASSES {
        for &(melody, melody_pitch) in &controls::MELODIES {
            let mut c = controller();
            let mut active = BTreeSet::new();
            apply(&mut active, &frame(&mut c, &[bass, melody], true));
            for modifier in [None, Some(0), Some(1)] {
                let mut anchors = vec![bass, melody];
                if let Some(modifier) = modifier {
                    anchors.push(modifier);
                }
                apply(&mut active, &frame(&mut c, &anchors, true));
                for command in [
                    controls::PREPARE_COL,
                    controls::TENSION_COL,
                    controls::RESOLVE_COL,
                    controls::SUBSTITUTE_COL,
                ] {
                    let mut pressed = anchors.clone();
                    pressed.push(command);
                    apply(&mut active, &frame(&mut c, &pressed, true));
                    assert!(
                        active.contains(&bass_pitch),
                        "bass changed for pair{bass}/{melody}"
                    );
                    assert!(
                        active.contains(&melody_pitch),
                        "melody changed for pair{bass}/{melody}"
                    );
                    assert!(frame(&mut c, &pressed, true).is_empty());
                    assert!(frame(&mut c, &anchors, true).is_empty());
                }
                assert!(frame(&mut c, &[bass, melody], true).is_empty());
            }
            apply(&mut active, &frame(&mut c, &[], true));
            assert!(active.is_empty());
        }
    }
}

#[test]
fn long_thumb_rolls_keep_full_chords_and_respect_the_highest_held_thumb() {
    for &(bass, bass_pitch) in &controls::BASSES {
        for &(melody, melody_pitch) in &controls::MELODIES {
            let mut c = controller();
            let mut active = BTreeSet::new();
            apply(&mut active, &frame(&mut c, &[bass, melody], true));
            let mut previous_thumb = None;
            let mut previous_bar = None;
            for step in 0..64 {
                let thumb = 22 + step % 4;
                let bar = match step / 4 % 3 {
                    0 => None,
                    1 => Some(0),
                    _ => Some(1),
                };
                let mut held = vec![bass, melody, thumb];
                if let Some(bar) = bar {
                    held.push(bar);
                }
                let mut rolled = held.clone();
                if let Some(previous) = previous_thumb {
                    rolled.push(previous);
                }
                let highest = [24, 23, 22, 25]
                    .into_iter()
                    .find(|col| rolled.contains(col));
                let acted = bar.is_some() && bar != previous_bar || highest == Some(thumb);
                let before = active.clone();
                let events = frame(&mut c, &rolled, true);
                let attacks: BTreeSet<_> = events
                    .iter()
                    .filter_map(|event| match event {
                        MidiEvent::NoteOn { note, .. } => Some(*note),
                        _ => None,
                    })
                    .collect();
                apply(&mut active, &events);
                assert_eq!(active.len(), 4, "{bass}/{melody}, step {step}");
                if acted {
                    let inner: BTreeSet<_> = active
                        .difference(&BTreeSet::from([bass_pitch, melody_pitch]))
                        .copied()
                        .collect();
                    assert_eq!(attacks, inner, "{bass}/{melody}, step {step}");
                } else {
                    assert!(events.is_empty());
                    assert_eq!(active, before);
                }
                assert!(active.contains(&bass_pitch) && active.contains(&melody_pitch));
                assert!(frame(&mut c, &rolled, true).is_empty());
                assert!(frame(&mut c, &held, true).is_empty());
                previous_thumb = Some(thumb);
                previous_bar = bar;
            }
            assert!(frame(&mut c, &[bass, melody], true).is_empty());
            apply(&mut active, &frame(&mut c, &[], true));
            assert!(active.is_empty());
        }
    }
}

#[test]
fn bar_and_thumb_edges_route_independent_commands_in_one_coherent_snapshot() {
    use tecla::music::autochord::{AnchorVoice, AutoChordInput, HarmonyCommand, Steering};
    for (bar, voice) in [(0, AnchorVoice::Bass), (1, AnchorVoice::Melody)] {
        for (thumb, command, steering) in [
            (22, HarmonyCommand::Prepare, Steering::Prepare),
            (23, HarmonyCommand::Tension, Steering::Lean),
            (24, HarmonyCommand::Resolve, Steering::Arrive),
            (25, HarmonyCommand::Substitute, Steering::Substitute),
        ] {
            let mut c = controller();
            let mut p =
                AutoChord::new(Key::C_MAJOR, controls::CHANNEL, controls::VELOCITY).unwrap();
            let input = AutoChordInput {
                bass: Some(36),
                melody: Some(64),
                gates: 15,
                ..Default::default()
            };
            let mut expected = Vec::new();
            p.update(
                AutoChordInput {
                    revoice: true,
                    retrigger: 9,
                    ..input
                },
                &mut |e| expected.push(e),
            )
            .unwrap();
            assert_eq!(frame(&mut c, &[13, 19], true), expected);
            expected.clear();
            p.update(
                AutoChordInput {
                    command: Some(HarmonyCommand::Reinterpret(voice)),
                    interpretation: Some(voice),
                    revoice: true,
                    ..input
                },
                &mut |e| expected.push(e),
            )
            .unwrap();
            assert_eq!(frame(&mut c, &[13, 19, bar], true), expected);
            expected.clear();
            p.update(
                AutoChordInput {
                    command: Some(command),
                    interpretation: Some(voice),
                    steering,
                    revoice: true,
                    ..input
                },
                &mut |e| expected.push(e),
            )
            .unwrap();
            assert_eq!(frame(&mut c, &[13, 19, bar, thumb], true), expected);
            assert!(frame(&mut c, &[13, 19, thumb], true).is_empty());
            assert!(frame(&mut c, &[13, 19], true).is_empty());
        }
    }
}

#[test]
fn raw_substitution_taps_make_distinct_changes_without_hold_or_release_repeat() {
    for gap in [2, 10, 20] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        let mut filter = CapacitiveFilter::new([100; controls::KEYS], [0; controls::KEYS]);
        let anchors = counts(&[13, 19]);
        assert!(raw_frame(&mut c, &mut filter, &anchors).is_empty());
        apply(&mut active, &raw_frame(&mut c, &mut filter, &anchors));
        for tap in 0..6 {
            let before = active.clone();
            let pressed = counts(&[13, 19, 25]);
            assert!(raw_frame(&mut c, &mut filter, &pressed).is_empty());
            apply(&mut active, &raw_frame(&mut c, &mut filter, &pressed));
            assert_ne!(active, before, "tap{tap}, gap{gap}");
            assert!(active.contains(&36) && active.contains(&64));
            for _ in 0..gap {
                assert!(raw_frame(&mut c, &mut filter, &pressed).is_empty());
            }
            for _ in 0..gap {
                assert!(raw_frame(&mut c, &mut filter, &anchors).is_empty());
            }
        }
        assert!(raw_frame(&mut c, &mut filter, &counts(&[])).is_empty());
        apply(&mut active, &raw_frame(&mut c, &mut filter, &counts(&[])));
        assert!(active.is_empty());
    }
}

#[test]
fn literal_held_notes_reharm_into_chromatic_tension_and_follow_its_resolution() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 19], true));
    apply(&mut active, &frame(&mut c, &[13, 19, 23], true));
    assert!(frame(&mut c, &[13, 19], true).is_empty());
    assert_eq!(active, BTreeSet::from([36, 56, 62, 64]));
    let events = frame(&mut c, &[13, 19, 24], true);
    assert_eq!(
        events,
        [note_off(56), note_off(62), note_on(57), note_on(60)]
    );
    apply(&mut active, &events);
    assert_eq!(active, BTreeSet::from([36, 57, 60, 64]));
}

#[test]
fn melody_neighbours_float_over_the_held_harmony_without_attacking_inner_voices() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 19], true)); // Lower S + -G: C2/E4.
    let lower: BTreeSet<_> = active.iter().copied().filter(|note| *note != 64).collect();
    let mut previous = 64;
    for (pad, pitch) in [(20, 65), (19, 64), (18, 62), (19, 64)] {
        let events = frame(&mut c, &[13, pad], true); // -S/-G/-B/-G: F4/E4/D4/E4.
        assert_eq!(events, [note_off(previous), note_on(pitch)]);
        apply(&mut active, &events);
        assert_eq!(
            active,
            lower.union(&BTreeSet::from([pitch])).copied().collect()
        );
        assert!(frame(&mut c, &[13, pad], true).is_empty());
        previous = pitch;
    }
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn a_flowing_neighbour_and_a_spaced_melody_attack_have_different_harmonic_authority() {
    let mut flowing = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame_at(&mut flowing, &[13, 19], 0, true));
    let lower: BTreeSet<_> = active.iter().copied().filter(|note| *note != 64).collect();
    let neighbour = frame_at(&mut flowing, &[13, 20], 100, true);
    assert_eq!(neighbour, [note_off(64), note_on(65)]);
    apply(&mut active, &neighbour);
    let decorated = active.clone();
    // Time is evidence at a later gesture; it never schedules a chord change.
    for now_ms in [200, 700, 1_500] {
        assert!(frame_at(&mut flowing, &[13, 20], now_ms, true).is_empty());
    }
    let returned = frame_at(&mut flowing, &[13, 19], 1_600, true);
    assert_eq!(returned, [note_off(65), note_on(64)]);
    apply(&mut active, &returned);
    assert_eq!(
        active,
        lower.union(&BTreeSet::from([64])).copied().collect()
    );

    let mut spaced = controller();
    let mut structural = BTreeSet::new();
    apply(&mut structural, &frame_at(&mut spaced, &[13, 19], 0, true));
    let attack = frame_at(&mut spaced, &[13, 20], 700, true);
    assert_no_pitch_events(&attack, &[36]);
    assert!(
        attack.contains(&note_on(65)),
        "the melody must sound immediately"
    );
    apply(&mut structural, &attack);
    assert_eq!(structural.len(), 4);
    assert!(structural.contains(&36) && structural.contains(&65));
    assert_ne!(
        structural, decorated,
        "a spaced attack remained a passing note"
    );
    assert!(frame_at(&mut spaced, &[13, 20], 1_700, true).is_empty());
    apply(&mut active, &frame_at(&mut flowing, &[], 1_800, true));
    apply(&mut structural, &frame_at(&mut spaced, &[], 1_800, true));
    assert!(active.is_empty() && structural.is_empty());
}

#[test]
fn repeating_a_melody_decoration_can_make_it_structural_without_restarting_the_bass() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame_at(&mut c, &[13, 19], 0, true));
    apply(&mut active, &frame_at(&mut c, &[13, 20], 100, true));
    let decorated = active.clone();
    let released = frame_at(&mut c, &[13], 150, true);
    assert_no_pitch_events(&released, &[36]);
    apply(&mut active, &released);
    assert_eq!(active, BTreeSet::from([36]));
    let repeated = frame_at(&mut c, &[13, 20], 200, true);
    assert_no_pitch_events(&repeated, &[36]);
    assert!(repeated.contains(&note_on(65)));
    apply(&mut active, &repeated);
    assert_eq!(active.len(), 4);
    assert!(active.contains(&36) && active.contains(&65));
    assert_ne!(
        active, decorated,
        "a repeated melody attack had no harmonic effect"
    );
    assert!(frame_at(&mut c, &[13, 20], 1_000, true).is_empty());
    apply(&mut active, &frame_at(&mut c, &[], 1_100, true));
    assert!(active.is_empty());
}

#[test]
fn reharm_can_move_under_a_held_melody_decoration_and_e_can_settle_it() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 19], true)); // C2/E4.
    apply(&mut active, &frame(&mut c, &[13, 20], true)); // F4 decorates the C chord.
    for _ in 0..4 {
        let before = active.clone();
        apply(
            &mut active,
            &frame(&mut c, &[13, 20, controls::SUBSTITUTE_COL], true),
        );
        assert_ne!(active, before, "U ignored the held melody decoration");
        assert_eq!(active.len(), 4);
        assert!(active.contains(&36) && active.contains(&65));
        assert!(frame(&mut c, &[13, 20, controls::SUBSTITUTE_COL], true).is_empty());
        assert!(frame(&mut c, &[13, 20], true).is_empty());
    }
    apply(
        &mut active,
        &frame(&mut c, &[13, 20, controls::RESOLVE_COL], true),
    );
    assert!(active.contains(&36) && active.contains(&64) && active.contains(&65));
    assert!(pitch_classes(&active).is_subset(&BTreeSet::from([0, 4, 5, 7])));
    assert!(frame(&mut c, &[13, 20], true).is_empty());
    apply(&mut active, &frame(&mut c, &[13, 19], true));
    assert!(active.contains(&36) && active.contains(&64));
    assert!(pitch_classes(&active).is_subset(&BTreeSet::from([0, 4, 7])));
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn harmonic_commands_articulate_the_accompaniment_without_interrupting_held_keyboards() {
    for command in [0, 1, 22, 23, 24, 25] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut c, &[13, 19], true));
        apply(&mut active, &frame(&mut c, &[13, 20], true)); // C2/F4, a prepared neighbour.
        let events = frame(&mut c, &[13, 20, command], true);
        assert_no_pitch_events(&events, &[36, 65]);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, MidiEvent::NoteOn { .. }))
                .count(),
            2,
            "control {command} must articulate the two generated voices"
        );
        apply(&mut active, &events);
        assert_eq!(active.len(), 4);
        assert!(active.contains(&36) && active.contains(&65));
        assert!(frame(&mut c, &[13, 20, command], true).is_empty());
        assert!(frame(&mut c, &[13, 20], true).is_empty());
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn each_keyboard_owns_its_fresh_attacks_while_the_other_hand_sustains() {
    for (held, tapped, held_pitch, tapped_pitch) in [(13, 19, 36, 64), (19, 13, 64, 36)] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut c, &[13, 19], true));
        for _ in 0..3 {
            let released = frame(&mut c, &[held], true);
            assert_no_pitch_events(&released, &[held_pitch]);
            assert!(released.contains(&note_off(tapped_pitch)));
            apply(&mut active, &released);
            assert_eq!(active, BTreeSet::from([held_pitch]));
            let attacked = frame(&mut c, &[held, tapped], true);
            assert_no_pitch_events(&attacked, &[held_pitch]);
            assert!(attacked.contains(&note_on(tapped_pitch)));
            assert!(attacked
                .iter()
                .all(|event| matches!(event, MidiEvent::NoteOn { .. })));
            apply(&mut active, &attacked);
            assert_eq!(active.len(), 4);
            assert!(active.contains(&held_pitch) && active.contains(&tapped_pitch));
        }
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn both_stars_are_independent_quiet_permissions_consumed_by_the_next_command() {
    use tecla::music::autochord::{AutoChordInput, HarmonyCommand, Steering};
    let mut outcomes = Vec::new();
    for (borrowing, extensions) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut c = controller();
        let mut direct =
            AutoChord::new(Key::C_MAJOR, controls::CHANNEL, controls::VELOCITY).unwrap();
        let initial = AutoChordInput {
            bass: Some(36),
            melody: Some(64),
            gates: 15,
            revoice: true,
            retrigger: 9,
            ..AutoChordInput::default()
        };
        let mut expected = Vec::new();
        direct
            .update(initial, &mut |event| expected.push(event))
            .unwrap();
        let mut active = BTreeSet::new();
        let played = frame(&mut c, &[13, 19], true);
        assert_eq!(played, expected);
        apply(&mut active, &played);
        let mut pads = vec![13, 19];
        if borrowing {
            pads.push(controls::BORROW_COL);
        }
        if extensions {
            pads.push(controls::EXTEND_COL);
        }
        assert!(frame(&mut c, &pads, true).is_empty());
        let licensed = AutoChordInput {
            revoice: true,
            retrigger: 0,
            borrowing,
            extensions,
            ..initial
        };
        // A permission edge cannot freeze either generated voice or retrigger a note.
        direct
            .update(licensed, &mut |_| panic!("a star edge must be quiet"))
            .unwrap();
        pads.push(controls::SUBSTITUTE_COL);
        expected.clear();
        direct
            .update(
                AutoChordInput {
                    command: Some(HarmonyCommand::Substitute),
                    steering: Steering::Substitute,
                    ..licensed
                },
                &mut |event| expected.push(event),
            )
            .unwrap();
        let changed = frame(&mut c, &pads, true);
        assert_eq!(
            changed, expected,
            "borrowing={borrowing} extensions={extensions}"
        );
        apply(&mut active, &changed);
        assert_eq!(active.len(), 4);
        assert!(active.contains(&36) && active.contains(&64));
        outcomes.push(active.clone());
        pads.pop();
        assert!(frame(&mut c, &pads, true).is_empty());
        for star in [controls::BORROW_COL, controls::EXTEND_COL] {
            pads.retain(|col| *col != star);
            assert!(frame(&mut c, &pads, true).is_empty());
        }
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
    assert_ne!(
        outcomes[0], outcomes[1],
        "left star must open a different palette"
    );
    assert_ne!(
        outcomes[0], outcomes[2],
        "right star must open a different palette"
    );
    assert_ne!(
        outcomes[1], outcomes[3],
        "right star must also work under borrowing"
    );
    assert_ne!(
        outcomes[2], outcomes[3],
        "left star must also work with extensions"
    );
}

fn pitch_classes(active: &BTreeSet<u8>) -> BTreeSet<u8> {
    active.iter().map(|pitch| pitch % 12).collect()
}

fn assert_four_voice_shell(
    active: &BTreeSet<u8>,
    bass: u8,
    melody: u8,
    essential: &[u8],
    permitted: &[u8],
) {
    assert_eq!(active.len(), 4, "missing voice: {active:?}");
    assert_eq!(active.first(), Some(&bass));
    assert_eq!(active.last(), Some(&melody));
    let tones = pitch_classes(active);
    assert!(
        essential.iter().all(|tone| tones.contains(tone)),
        "missing chord tone: {active:?}"
    );
    assert!(
        tones.is_subset(&permitted.iter().copied().collect()),
        "unexpected chord tone: {active:?}"
    );
}

#[test]
fn automatic_keyboard_cadence_moves_from_tonic_through_predominant_and_dominant_home() {
    for (last_bass, pitch) in [(13, 36), (5, 48)] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        for (pads, bass, melody, essential, permitted) in [
            ([13, 19], 36, 64, &[0, 4][..], &[0, 4, 7][..]), // C2/E4: I.
            ([14, 20], 38, 65, &[2, 5][..], &[2, 5, 9][..]), // D2/F4: ii.
            ([2, 20], 43, 65, &[5, 7, 11][..], &[2, 5, 7, 11][..]), // G2/F4: V7.
            ([last_bass, 19], pitch, 64, &[0, 4][..], &[0, 4, 7][..]), // I.
        ] {
            apply(&mut active, &frame(&mut c, &pads, true));
            assert_four_voice_shell(&active, bass, melody, essential, permitted);
        }
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn automatic_c_major_inversions_keep_the_held_e4_and_literal_c_e_g_c_bass() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    for (bass, pitch) in [(13, 36), (15, 40), (2, 43), (5, 48)] {
        apply(&mut active, &frame(&mut c, &[bass, 19], true));
        assert_four_voice_shell(&active, pitch, 64, &[0, 4], &[0, 4, 7]);
    }
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn automatic_jazz_cadence_keeps_the_seventh_as_a_guide_tone_then_resolves_it() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 19], true));
    // D2/C5: the played C is ii7's seventh, not a demand for a C-root chord.
    apply(&mut active, &frame(&mut c, &[14, 10], true));
    assert_four_voice_shell(&active, 38, 72, &[0, 2, 5], &[0, 2, 5, 9]);
    let prepared: Vec<_> = active
        .iter()
        .copied()
        .filter(|note| note % 12 == 5)
        .collect();
    // G2/B4: C5 descends to V7's third while an inner F remains sounding.
    let dominant = frame(&mut c, &[2, 9], true);
    apply(&mut active, &dominant);
    assert_four_voice_shell(&active, 43, 71, &[5, 7, 11], &[2, 5, 7, 11]);
    let common: Vec<_> = prepared
        .into_iter()
        .filter(|note| active.contains(note) && !dominant.contains(&note_off(*note)))
        .collect();
    assert!(!common.is_empty(), "ii7's third should become V7's seventh");
    // C2/B4: retain the literal major seventh while F falls to tonic's E.
    let tonic = frame(&mut c, &[13, 9], true);
    assert!(common
        .iter()
        .any(|&note| { tonic.contains(&note_off(note)) && tonic.contains(&note_on(note - 1)) }));
    assert!(!tonic.contains(&note_off(71)));
    apply(&mut active, &tonic);
    assert_four_voice_shell(&active, 36, 71, &[0, 4, 11], &[0, 4, 7, 11]);
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn automatic_applied_dominant_resolves_when_new_bass_unlocks_a_minor_without_e() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 19], true));
    apply(
        &mut active,
        &frame(&mut c, &[13, 19, controls::TENSION_COL], true),
    );
    assert!(frame(&mut c, &[13, 19], true).is_empty());
    assert_eq!(active, BTreeSet::from([36, 56, 62, 64]));
    // A fresh A2 unlocks the applied dominant's destination. G# rises and D falls.
    apply(&mut active, &frame(&mut c, &[3, 19], true));
    assert_eq!(active, BTreeSet::from([45, 57, 60, 64]));
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn automatic_iv_to_i_resolves_both_middle_voices_when_the_bass_moves() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[16, 10], true));
    assert_eq!(active, BTreeSet::from([41, 57, 65, 72]));
    apply(&mut active, &frame(&mut c, &[13, 10], true));
    assert_eq!(active, BTreeSet::from([36, 55, 64, 72]));
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn right_star_opens_a_major_thirteenth_and_a_dominant_thirteenth_without_changing_the_keys() {
    for (letters, thumb, expected) in [
        ([13, 19], controls::SUBSTITUTE_COL, [36, 57, 59, 64]), // Cmaj13: C/A/B/E.
        ([2, 12], controls::TENSION_COL, [43, 59, 65, 76]),     // G13: G/B/F/E.
    ] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut c, &letters, true));
        assert!(frame(
            &mut c,
            &[letters[0], letters[1], controls::EXTEND_COL],
            true
        )
        .is_empty());
        let pads = [letters[0], letters[1], controls::EXTEND_COL, thumb];
        apply(&mut active, &frame(&mut c, &pads, true));
        assert_eq!(active, BTreeSet::from(expected));
        assert!(frame(&mut c, &pads[..3], true).is_empty());
        assert!(frame(&mut c, &letters, true).is_empty());
        assert_eq!(active, BTreeSet::from(expected));
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn licensed_substitutions_keep_their_directed_resolutions_under_literal_played_notes() {
    // Left star opens an augmented applied chord, then E resolves G#3 to A3.
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 19], true));
    assert!(frame(&mut c, &[13, 19, controls::BORROW_COL], true).is_empty());
    apply(
        &mut active,
        &frame(
            &mut c,
            &[13, 19, controls::BORROW_COL, controls::SUBSTITUTE_COL],
            true,
        ),
    );
    assert_eq!(active, BTreeSet::from([36, 56, 60, 64]));
    assert!(frame(&mut c, &[13, 19, controls::BORROW_COL], true).is_empty());
    let arrival = frame(
        &mut c,
        &[13, 19, controls::BORROW_COL, controls::RESOLVE_COL],
        true,
    );
    assert!(arrival.contains(&note_off(56)) && arrival.contains(&note_on(57)));
    apply(&mut active, &arrival);
    assert_eq!(active, BTreeSet::from([36, 57, 60, 64]));
    assert!(frame(&mut c, &[13, 19], true).is_empty());
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());

    // Both stars open C13; changing only -G to -S unlocks F/C and Bb3-A3.
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 19], true));
    assert!(frame(
        &mut c,
        &[13, 19, controls::BORROW_COL, controls::EXTEND_COL],
        true
    )
    .is_empty());
    apply(
        &mut active,
        &frame(
            &mut c,
            &[
                13,
                19,
                controls::BORROW_COL,
                controls::EXTEND_COL,
                controls::SUBSTITUTE_COL,
            ],
            true,
        ),
    );
    assert_eq!(active, BTreeSet::from([36, 57, 58, 64]));
    assert!(frame(
        &mut c,
        &[13, 19, controls::BORROW_COL, controls::EXTEND_COL],
        true
    )
    .is_empty());
    let arrival = frame(
        &mut c,
        &[13, 20, controls::BORROW_COL, controls::EXTEND_COL],
        true,
    );
    assert!(arrival.contains(&note_off(58)));
    apply(&mut active, &arrival);
    assert_eq!(active, BTreeSet::from([36, 53, 57, 65]));
    assert!(frame(&mut c, &[13, 20], true).is_empty());
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn automatic_cadence_survives_connected_and_both_orders_of_overlapping_key_slides() {
    for order in [None, Some([0, 1]), Some([1, 0])] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        let mut current = [13, 19]; // C2/E4.
        apply(&mut active, &frame(&mut c, &current, true));
        for next in [[14, 20], [2, 20], [13, 19]] {
            if let Some(order) = order {
                for bank in order {
                    if current[bank] == next[bank] {
                        continue;
                    }
                    let mut overlap = current.to_vec();
                    overlap.push(next[bank]);
                    let added = frame(&mut c, &overlap, true);
                    let previous = selected_pitches(&current);
                    let next_selected = selected_pitches(&overlap);
                    let sustained: Vec<_> = previous
                        .into_iter()
                        .filter(|pitch| next_selected.contains(pitch))
                        .collect();
                    assert_no_pitch_events(&added, &sustained);
                    apply(&mut active, &added);
                    assert_selected_held(&active, &overlap);
                    current[bank] = next[bank];
                    let released = frame(&mut c, &current, true);
                    let sustained: Vec<_> = selected_pitches(&overlap)
                        .into_iter()
                        .filter(|pitch| selected_pitches(&current).contains(pitch))
                        .collect();
                    assert_no_pitch_events(&released, &sustained);
                    apply(&mut active, &released);
                    assert_eq!(active.len(), 4);
                }
            } else {
                apply(&mut active, &frame(&mut c, &next, true));
                current = next;
            }
            assert_eq!(current, next);
            assert_eq!(active.len(), 4);
            if next == [2, 20] {
                assert_four_voice_shell(&active, 43, 65, &[5, 7, 11], &[2, 5, 7, 11]);
            }
        }
        assert_four_voice_shell(&active, 36, 64, &[0, 4], &[0, 4, 7]);
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn held_thumb_guidance_changes_later_keyboard_harmony_and_releases_quietly() {
    let mut outcomes = Vec::new();
    for guide in [controls::PREPARE_COL, controls::TENSION_COL] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut c, &[13, 19], true));
        apply(&mut active, &frame(&mut c, &[13, 19, guide], true));
        assert!(frame(&mut c, &[13, 19, guide], true).is_empty());
        apply(&mut active, &frame(&mut c, &[14, 20, guide], true));
        assert_eq!(active.len(), 4);
        assert!(active.contains(&38) && active.contains(&65));
        outcomes.push(pitch_classes(&active));
        assert!(frame(&mut c, &[14, 20], true).is_empty());
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
    let d_minor = BTreeSet::from([2, 5, 9]);
    assert!(
        outcomes[0].is_subset(&d_minor),
        "guidance outcomes: {outcomes:?}"
    );
    // Lean may choose a seventh or a different directed interpretation while held.
    assert!(!outcomes[1].is_subset(&d_minor));
}

#[test]
fn automatic_pending_arrival_survives_a_rest_but_stop_starts_a_fresh_phrase() {
    for stop in [false, true] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut c, &[13, 19], true));
        apply(
            &mut active,
            &frame(&mut c, &[13, 19, controls::TENSION_COL], true),
        );
        assert!(frame(&mut c, &[13, 19], true).is_empty());
        if stop {
            apply(&mut active, &frame(&mut c, &[13, 19, 0, 1], true));
            assert!(active.is_empty());
            assert!(frame(&mut c, &[13, 19], true).is_empty());
        }
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
        if stop {
            let mut fresh = controller();
            // Stop clears old pressure and tendencies, not only sounding notes.
            let expected = frame(&mut fresh, &[16, 10], true);
            let actual = frame(&mut c, &[16, 10], true);
            assert_eq!(actual, expected);
            apply(&mut active, &actual);
        } else {
            // A brief rest preserves the applied dominant's remembered destination.
            apply(&mut active, &frame(&mut c, &[3, 19], true));
            assert_eq!(active, BTreeSet::from([45, 57, 60, 64]));
        }
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn an_established_dominant_can_keep_its_guides_when_the_melody_takes_its_root_away() {
    for held_o in [false, true] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        apply(&mut active, &frame(&mut c, &[13, 21], true)); // Lower S + -Z: C2/G4.
        let dominant = frame(&mut c, &[13, 21, controls::TENSION_COL], true);
        assert_no_pitch_events(&dominant, &[36, 67]);
        apply(&mut active, &dominant);
        assert_eq!(active, BTreeSet::from([36, 59, 65, 67])); // G7/C, B3 and F4 inside.
        if !held_o {
            assert!(frame(&mut c, &[13, 21], true).is_empty());
        }

        let mut pads = vec![13, 12]; // Upper -D: E5 supplies the thirteenth.
        if held_o {
            pads.push(controls::TENSION_COL);
        }
        let melody = frame(&mut c, &pads, true);
        assert_eq!(melody, [note_off(67), note_on(76)], "held O={held_o}");
        apply(&mut active, &melody);
        assert_eq!(active, BTreeSet::from([36, 59, 65, 76]));
        assert!(frame(&mut c, &pads, true).is_empty());

        pads.push(controls::RESOLVE_COL);
        let arrival = frame(&mut c, &pads, true);
        assert_no_pitch_events(&arrival, &[36, 76]);
        assert!(arrival.contains(&note_off(59)) && arrival.contains(&note_on(60)));
        assert!(arrival.contains(&note_off(65)) && arrival.contains(&note_on(64)));
        apply(&mut active, &arrival);
        assert_eq!(active, BTreeSet::from([36, 60, 64, 76]));
        assert!(frame(&mut c, &[13, 12], true).is_empty());
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn held_c_pedal_supports_a_dominant_and_resolves_both_middle_voices_on_e() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 21], true));
    assert_eq!(active, BTreeSet::from([36, 55, 64, 67]));
    apply(
        &mut active,
        &frame(&mut c, &[13, 21, controls::TENSION_COL], true),
    );
    assert_eq!(active, BTreeSet::from([36, 59, 65, 67]));
    assert!(frame(&mut c, &[13, 21], true).is_empty());
    apply(
        &mut active,
        &frame(&mut c, &[13, 21, controls::RESOLVE_COL], true),
    );
    assert_eq!(active, BTreeSet::from([36, 60, 64, 67]));
    assert!(frame(&mut c, &[13, 21], true).is_empty());
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn rising_and_falling_bass_figures_connect_inversions_under_literal_melody() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    // Lower-left S-K-W while lower-right -Z remains held: C-D-E below G.
    for (pads, bass, essential, permitted) in [
        ([13, 21], 36, &[0, 4][..], &[0, 4, 7][..]),
        ([14, 21], 38, &[7, 11][..], &[2, 5, 7, 11][..]),
        ([15, 21], 40, &[0, 4][..], &[0, 4, 7][..]),
    ] {
        apply(&mut active, &frame(&mut c, &pads, true));
        assert_four_voice_shell(&active, bass, 67, essential, permitted);
    }
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());

    for order in [None, Some([0, 1]), Some([1, 0])] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        let mut current = [15, 21]; // E2/G4: C/E.
        apply(&mut active, &frame(&mut c, &current, true));
        let mut dominant_guides = Vec::new();
        for next in [[14, 20], [13, 19]] {
            let mut heard_during_transition = BTreeSet::new();
            if let Some(order) = order {
                for bank in order {
                    let mut overlap = current.to_vec();
                    overlap.push(next[bank]);
                    let added = frame(&mut c, &overlap, true);
                    let previous = selected_pitches(&current);
                    let next_selected = selected_pitches(&overlap);
                    let sustained: Vec<_> = previous
                        .into_iter()
                        .filter(|pitch| next_selected.contains(pitch))
                        .collect();
                    assert_no_pitch_events(&added, &sustained);
                    apply(&mut active, &added);
                    heard_during_transition.extend(active.iter().copied());
                    assert_selected_held(&active, &overlap);
                    current[bank] = next[bank];
                    let released = frame(&mut c, &current, true);
                    let sustained: Vec<_> = selected_pitches(&overlap)
                        .into_iter()
                        .filter(|pitch| selected_pitches(&current).contains(pitch))
                        .collect();
                    assert_no_pitch_events(&released, &sustained);
                    apply(&mut active, &released);
                    heard_during_transition.extend(active.iter().copied());
                    assert_eq!(active.len(), 4);
                }
            } else {
                apply(&mut active, &frame(&mut c, &next, true));
                heard_during_transition.extend(active.iter().copied());
                current = next;
            }
            if next == [14, 20] {
                if order.is_some() && !pitch_classes(&active).contains(&7) {
                    // The genuinely sounded overlapping E/G notes can select vii°6
                    // rather than V4/3. Both complete chords have dominant function.
                    assert_four_voice_shell(&active, 38, 65, &[2, 5, 11], &[2, 5, 11]);
                } else {
                    assert_four_voice_shell(&active, 38, 65, &[5, 7, 11], &[2, 5, 7, 11]);
                }
                dominant_guides = active
                    .iter()
                    .copied()
                    .filter(|note| *note != 65 && matches!(note % 12, 5 | 11))
                    .collect();
                assert!(!dominant_guides.is_empty());
            } else {
                assert_four_voice_shell(&active, 36, 64, &[0, 4], &[0, 4, 7]);
                for guide in &dominant_guides {
                    let target = if guide % 12 == 11 {
                        guide + 1
                    } else {
                        guide - 1
                    };
                    // C2 can resolve both guides before the old D2 is lifted.
                    // That later release may revoice the already-arrived tonic.
                    assert!(heard_during_transition.contains(&target),
                        "dominant guide {guide} did not resolve to {target}, order {order:?}: {heard_during_transition:?}");
                }
            }
        }
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn generated_inner_passing_line_advances_on_melody_changes_and_keeps_the_alto_tied() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    // Lower-left S held; right lower -Z, upper -L, lower -Z.
    apply(&mut active, &frame(&mut c, &[13, 21], true));
    assert_eq!(active, BTreeSet::from([36, 55, 64, 67]));
    let passing = frame(&mut c, &[13, 10], true);
    assert!(!passing.contains(&note_off(64)));
    apply(&mut active, &passing);
    assert_eq!(active, BTreeSet::from([36, 53, 64, 72]));
    for _ in 0..16 {
        assert!(frame(&mut c, &[13, 10], true).is_empty());
    }
    let arrival = frame(&mut c, &[13, 21], true);
    assert!(arrival.contains(&note_off(53)) && arrival.contains(&note_on(52)));
    assert!(!arrival.contains(&note_off(64)));
    apply(&mut active, &arrival);
    assert_eq!(active, BTreeSet::from([36, 52, 64, 67]));
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn borrowing_permission_does_not_freeze_a_prepared_passing_note() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 21], true));
    apply(&mut active, &frame(&mut c, &[13, 10], true));
    assert_eq!(active, BTreeSet::from([36, 53, 64, 72]));
    assert!(frame(&mut c, &[13, 10, controls::BORROW_COL], true).is_empty());
    let movement = frame(&mut c, &[13, 21, controls::BORROW_COL], true);
    assert!(movement.contains(&note_off(53)) && movement.contains(&note_on(52)));
    apply(&mut active, &movement);
    assert_eq!(active, BTreeSet::from([36, 52, 64, 67]));
    assert!(frame(&mut c, &[13, 21], true).is_empty());
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn an_arrived_major_seventh_keeps_its_colour_when_the_played_melody_requires_it() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    // C2/B4 is a coloured tonic, not an instruction to replace C with G/C.
    apply(&mut active, &frame(&mut c, &[13, 9], true));
    assert_four_voice_shell(&active, 36, 71, &[0, 4, 11], &[0, 4, 7, 11]);
    apply(
        &mut active,
        &frame(&mut c, &[13, 9, controls::RESOLVE_COL], true),
    );
    assert_four_voice_shell(&active, 36, 71, &[0, 4, 11], &[0, 4, 7, 11]);
    assert!(frame(&mut c, &[13, 9, controls::RESOLVE_COL], true).is_empty());
    assert!(frame(&mut c, &[13, 9], true).is_empty());
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn cadential_six_four_resolves_its_sixth_even_when_extensions_are_licensed() {
    for extensions in [false, true] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        // Lower S/-G, lower K/-S, upper S/-L: C, ii, cadential C/G.
        for pads in [[13, 19], [14, 20], [2, 10]] {
            apply(&mut active, &frame(&mut c, &pads, true));
        }
        assert_eq!(active, BTreeSet::from([43, 55, 64, 72]));
        if extensions {
            assert!(frame(&mut c, &[2, 10, controls::EXTEND_COL], true).is_empty());
        }
        let mut pads = vec![2, 9];
        if extensions {
            pads.push(controls::EXTEND_COL);
        }
        let movement = frame(&mut c, &pads, true);
        assert!(!movement.contains(&note_off(55)));
        assert!(movement.contains(&note_off(64)) && movement.contains(&note_on(62)));
        apply(&mut active, &movement);
        assert_eq!(active, BTreeSet::from([43, 55, 62, 71]));
        assert!(frame(&mut c, &[2, 9], true).is_empty());
        apply(&mut active, &frame(&mut c, &[13, 10], true));
        assert_four_voice_shell(&active, 36, 72, &[0, 4], &[0, 4, 7]);
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn unlicensed_u_substitutes_a_predominant_without_inventing_an_applied_dominant() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    // Lower-left K and upper-right -F: D2/A4 leaves room for ii or IV substitutions.
    apply(&mut active, &frame(&mut c, &[14, 8], true));
    apply(
        &mut active,
        &frame(&mut c, &[14, 8, controls::SUBSTITUTE_COL], true),
    );
    assert_four_voice_shell(&active, 38, 69, &[5], &[0, 2, 5, 9]);
    assert!(frame(&mut c, &[14, 8], true).is_empty());
    apply(&mut active, &frame(&mut c, &[2, 9], true));
    assert_four_voice_shell(&active, 43, 71, &[7, 11], &[2, 5, 7, 11]);
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}

#[test]
fn borrowing_permission_preserves_a_diminished_fifths_downward_resolution() {
    for borrowing in [false, true] {
        let mut c = controller();
        let mut active = BTreeSet::new();
        // Upper-left T/-P: A2/B4. O selects B-half-diminished over its seventh A.
        apply(&mut active, &frame(&mut c, &[3, 9], true));
        apply(
            &mut active,
            &frame(&mut c, &[3, 9, controls::TENSION_COL], true),
        );
        assert_four_voice_shell(&active, 45, 71, &[2, 5, 9, 11], &[2, 5, 9, 11]);
        assert!(active.contains(&53));
        assert!(frame(&mut c, &[3, 9], true).is_empty());
        if borrowing {
            assert!(frame(&mut c, &[3, 9, controls::BORROW_COL], true).is_empty());
        }
        let mut pads = vec![13, 10];
        if borrowing {
            pads.push(controls::BORROW_COL);
        }
        let movement = frame(&mut c, &pads, true);
        assert!(movement.contains(&note_off(53)) && movement.contains(&note_on(52)));
        apply(&mut active, &movement);
        assert_four_voice_shell(&active, 36, 72, &[0, 4], &[0, 4, 7]);
        assert!(active.contains(&52));
        assert!(frame(&mut c, &[13, 10], true).is_empty());
        apply(&mut active, &frame(&mut c, &[], true));
        assert!(active.is_empty());
    }
}

#[test]
fn an_unprepared_opening_establishes_tonic_and_a_direct_bar_can_reinterpret_the_bass() {
    let mut c = controller();
    let mut active = BTreeSet::new();
    apply(&mut active, &frame(&mut c, &[13, 17], true));
    assert_four_voice_shell(&active, 36, 60, &[0, 4], &[0, 4, 7]);
    let first = active.clone();
    apply(&mut active, &frame(&mut c, &[13, 17, 0], true));
    assert_ne!(active, first);
    assert!(active.contains(&36) && active.contains(&60));
    assert!(frame(&mut c, &[13, 17], true).is_empty());
    apply(&mut active, &frame(&mut c, &[], true));
    assert!(active.is_empty());
}
