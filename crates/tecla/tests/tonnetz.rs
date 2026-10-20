#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test assertions")]

use std::collections::{BTreeSet, VecDeque};
use tecla::music::tonnetz::{pitch_class, Transformation, Triad};
use tecla::music::{ChordQuality, PitchClassSet};

const QUALITIES: [ChordQuality; 2] = [ChordQuality::Major, ChordQuality::Minor];
const OPERATIONS: [Transformation; 3] = [
    Transformation::Parallel,
    Transformation::Relative,
    Transformation::LeadingToneExchange,
];

fn triad(root: u8, quality: ChordQuality) -> Triad {
    Triad::new(root, quality).unwrap()
}

fn state(triad: Triad) -> (u8, bool) {
    (triad.root(), triad.quality() == ChordQuality::Minor)
}

fn pitches(notes: &[u8]) -> PitchClassSet {
    let mut set = PitchClassSet::default();
    for &note in notes {
        set.insert(note);
    }
    set
}

#[test]
fn lattice_has_named_fifths_thirds_and_negative_neighbours() {
    for (coordinate, expected) in [
        ((0, 0), 0),  // C
        ((1, 0), 7),  // G, one perfect fifth up
        ((2, 0), 2),  // D
        ((0, 1), 4),  // E, one major third up
        ((0, 2), 8),  // G sharp
        ((-1, 0), 5), // F
        ((0, -1), 8), // A flat
        ((1, -1), 3), // E flat, the minor third beside C
        ((-1, 1), 9), // A
        ((-2, -1), 6),
    ] {
        assert_eq!(pitch_class(coordinate.0, coordinate.1), expected);
    }
}

#[test]
fn lattice_periods_and_enharmonic_routes_agree() {
    for fifths in -24..=24 {
        for thirds in -12..=12 {
            let pitch = pitch_class(fifths, thirds);
            assert_eq!(pitch_class(fifths + 12, thirds), pitch);
            assert_eq!(pitch_class(fifths, thirds + 3), pitch);
            // Four fifths and one descending major third differ by two octaves.
            assert_eq!(pitch_class(fifths + 4, thirds - 1), pitch);
            assert_eq!(pitch_class(fifths + 1, thirds), (pitch + 7) % 12);
            assert_eq!(pitch_class(fifths, thirds + 1), (pitch + 4) % 12);
        }
    }
}

#[test]
fn lattice_extreme_coordinates_do_not_overflow() {
    let coordinates = [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX];
    for fifths in coordinates {
        for thirds in coordinates {
            // The oracle deliberately widens before doing the unreduced arithmetic.
            let expected = (7 * i64::from(fifths) + 4 * i64::from(thirds)).rem_euclid(12);
            let actual = pitch_class(fifths, thirds);
            assert!(actual < 12);
            assert_eq!(i64::from(actual), expected);
        }
    }
}

#[test]
fn only_the_twenty_four_major_and_minor_triads_are_constructible() {
    for root in 0..12 {
        for (quality, intervals) in [
            (ChordQuality::Major, [0, 4, 7]),
            (ChordQuality::Minor, [0, 3, 7]),
        ] {
            let chord = triad(root, quality);
            assert_eq!(chord.root(), root);
            assert_eq!(chord.quality(), quality);
            assert_eq!(chord.tones().len(), 3);
            assert_eq!(
                chord.tones(),
                pitches(&intervals.map(|interval| (root + interval) % 12))
            );
        }
    }
    for root in 12..=u8::MAX {
        for quality in QUALITIES {
            assert_eq!(Triad::new(root, quality), None);
        }
    }
    for root in 0..=u8::MAX {
        for quality in [ChordQuality::Diminished, ChordQuality::Augmented] {
            assert_eq!(Triad::new(root, quality), None);
        }
    }
}

#[test]
fn adjacent_lattice_triangles_are_the_expected_triads() {
    for fifths in -12..=12 {
        for thirds in -6..=6 {
            let root = pitch_class(fifths, thirds);
            let fifth = pitch_class(fifths + 1, thirds);
            let major_third = pitch_class(fifths, thirds + 1);
            let minor_third = pitch_class(fifths + 1, thirds - 1);
            assert_eq!(
                triad(root, ChordQuality::Major).tones(),
                pitches(&[root, fifth, major_third])
            );
            assert_eq!(
                triad(root, ChordQuality::Minor).tones(),
                pitches(&[root, fifth, minor_third])
            );
        }
    }
}

#[test]
fn transformations_match_all_named_chromatic_relations() {
    // Rows are roots C through B; columns are P, R, L. These are independent
    // named relations: C major -> C minor / A minor / E minor, for example.
    let major_targets = [
        [0, 9, 4],
        [1, 10, 5],
        [2, 11, 6],
        [3, 0, 7],
        [4, 1, 8],
        [5, 2, 9],
        [6, 3, 10],
        [7, 4, 11],
        [8, 5, 0],
        [9, 6, 1],
        [10, 7, 2],
        [11, 8, 3],
    ];
    let minor_targets = [
        [0, 3, 8],
        [1, 4, 9],
        [2, 5, 10],
        [3, 6, 11],
        [4, 7, 0],
        [5, 8, 1],
        [6, 9, 2],
        [7, 10, 3],
        [8, 11, 4],
        [9, 0, 5],
        [10, 1, 6],
        [11, 2, 7],
    ];
    for (quality, targets, target_quality) in [
        (ChordQuality::Major, major_targets, ChordQuality::Minor),
        (ChordQuality::Minor, minor_targets, ChordQuality::Major),
    ] {
        for (root, expected_roots) in targets.into_iter().enumerate() {
            let chord = triad(root as u8, quality);
            for (operation, expected_root) in OPERATIONS.into_iter().zip(expected_roots) {
                let transformed = chord.transform(operation);
                assert_eq!(transformed.root(), expected_root);
                assert_eq!(transformed.quality(), target_quality);
                assert_eq!(transformed.transform(operation), chord);
            }
        }
    }
}

#[test]
fn each_transformation_keeps_two_tones_and_moves_the_correct_voice() {
    for root in 0..12 {
        for quality in QUALITIES {
            let chord = triad(root, quality);
            for operation in OPERATIONS {
                let transformed = chord.transform(operation);
                let before = chord.tones().bits();
                let after = transformed.tones().bits();
                assert_eq!((before & after).count_ones(), 2);
                assert_eq!((before & !after).count_ones(), 1);
                assert_eq!((after & !before).count_ones(), 1);
                let removed = (before & !after).trailing_zeros() as u8;
                let added = (after & !before).trailing_zeros() as u8;
                // P moves the third; R moves a fifth (major) or root (minor);
                // L moves a root (major) or fifth (minor). R moves a tone,
                // whereas the other two operations move a semitone.
                let (removed_interval, directed_motion) = match (quality, operation) {
                    (ChordQuality::Major, Transformation::Parallel) => (4, 11),
                    (ChordQuality::Minor, Transformation::Parallel) => (3, 1),
                    (ChordQuality::Major, Transformation::Relative) => (7, 2),
                    (ChordQuality::Minor, Transformation::Relative) => (0, 10),
                    (ChordQuality::Major, Transformation::LeadingToneExchange) => (0, 11),
                    (ChordQuality::Minor, Transformation::LeadingToneExchange) => (7, 1),
                    _ => unreachable!("constructible triads are major or minor"),
                };
                assert_eq!(removed, (root + removed_interval) % 12);
                assert_eq!((added + 12 - removed) % 12, directed_motion);
            }
        }
    }
}

#[test]
fn transformations_commute_with_every_chromatic_transposition() {
    for root in 0..12 {
        for quality in QUALITIES {
            let chord = triad(root, quality);
            for operation in OPERATIONS {
                let transformed = chord.transform(operation);
                for semitones in 0..12 {
                    let transposed = triad((root + semitones) % 12, quality);
                    let expected =
                        triad((transformed.root() + semitones) % 12, transformed.quality());
                    assert_eq!(transposed.transform(operation), expected);
                    let expected_tones = pitches(
                        &(0..12)
                            .filter(|note| transformed.tones().contains(*note))
                            .map(|note| (note + semitones) % 12)
                            .collect::<Vec<_>>(),
                    );
                    assert_eq!(expected.tones(), expected_tones);
                }
            }
        }
    }
}

#[test]
fn plr_reaches_all_twenty_four_triads_from_c_major() {
    let start = triad(0, ChordQuality::Major);
    let mut visited = BTreeSet::from([state(start)]);
    let mut queue = VecDeque::from([start]);
    while let Some(chord) = queue.pop_front() {
        for operation in OPERATIONS {
            let next = chord.transform(operation);
            if visited.insert(state(next)) {
                queue.push_back(next);
            }
        }
    }
    let expected = (0..12)
        .flat_map(|root| [(root, false), (root, true)])
        .collect::<BTreeSet<_>>();
    assert_eq!(visited, expected);
}

#[test]
fn alternating_pl_pr_and_lr_have_six_eight_and_twenty_four_states() {
    // Read each pair in performance order, left to right: PL means first P,
    // then L. Counts below are individual transformations, not pairs.
    for (pair, length) in [
        (
            [
                Transformation::Parallel,
                Transformation::LeadingToneExchange,
            ],
            6,
        ),
        ([Transformation::Parallel, Transformation::Relative], 8),
        (
            [
                Transformation::LeadingToneExchange,
                Transformation::Relative,
            ],
            24,
        ),
    ] {
        for root in 0..12 {
            for quality in QUALITIES {
                let start = triad(root, quality);
                let mut chord = start;
                let mut visited = BTreeSet::new();
                for step in 0..length {
                    assert!(visited.insert(state(chord)), "cycle closed too early");
                    chord = chord.transform(pair[step % 2]);
                }
                assert_eq!(chord, start);
                assert_eq!(visited.len(), length);
            }
        }
    }
}

#[test]
fn named_pl_pr_and_lr_cycles_have_the_documented_direction() {
    // PL, applied from left to right: C, Cm, Ab, Abm, E, Em, C.
    // PR: C, Cm, Eb, Ebm, Gb, Gbm, A, Am, C.
    // LR alternates the minor mediant and the next major chord around fifths.
    for (pair, expected) in [
        (
            [
                Transformation::Parallel,
                Transformation::LeadingToneExchange,
            ],
            vec![
                (0, false),
                (0, true),
                (8, false),
                (8, true),
                (4, false),
                (4, true),
                (0, false),
            ],
        ),
        (
            [Transformation::Parallel, Transformation::Relative],
            vec![
                (0, false),
                (0, true),
                (3, false),
                (3, true),
                (6, false),
                (6, true),
                (9, false),
                (9, true),
                (0, false),
            ],
        ),
        (
            [
                Transformation::LeadingToneExchange,
                Transformation::Relative,
            ],
            vec![
                (0, false),
                (4, true),
                (7, false),
                (11, true),
                (2, false),
                (6, true),
                (9, false),
                (1, true),
                (4, false),
                (8, true),
                (11, false),
                (3, true),
                (6, false),
                (10, true),
                (1, false),
                (5, true),
                (8, false),
                (0, true),
                (3, false),
                (7, true),
                (10, false),
                (2, true),
                (5, false),
                (9, true),
                (0, false),
            ],
        ),
    ] {
        let mut chord = triad(0, ChordQuality::Major);
        let mut actual = vec![state(chord)];
        for step in 0..expected.len() - 1 {
            chord = chord.transform(pair[step % 2]);
            actual.push(state(chord));
        }
        assert_eq!(actual, expected);
    }
}
