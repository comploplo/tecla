#![allow(
    clippy::tests_outside_test_module,
    missing_docs,
    reason = "integration tests exercise typing statistics through physical events and HID output"
)]
#![expect(clippy::expect_used, reason = "test assertions")]

mod common;

use common::processor;
use tecla::accessibility::AccessibilityConfig;
use tecla::action::{Action, DksKeys, KeyCode, LayerOp, MidiAction, ModifierKey};
use tecla::devices::keyboard::combo::{Combo, ComboConfig};
use tecla::keyboard::{KeyboardProcessor, RolloverMode};
use tecla::matrix::TriggerPhase;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::report::HidReport;

type Processor = KeyboardProcessor<1, 16, 2, 16>;

fn configured(actions: &[Action]) -> Processor {
    processor(&[actions]).with_typing_stats()
}

fn event(proc: &mut Processor, key: u8, down: bool, timestamp: u64) -> Vec<HidReport> {
    delayed_event(proc, key, down, timestamp, timestamp)
}

fn delayed_event(
    proc: &mut Processor,
    key: u8,
    down: bool,
    timestamp: u64,
    processed_at: u64,
) -> Vec<HidReport> {
    let mut reports = Vec::new();
    let pos = KeyPos::new(0, key);
    let input = if down {
        KeyEvent::press(pos, timestamp)
    } else {
        KeyEvent::release(pos, timestamp)
    };
    proc.process_event(input, processed_at, &mut |report| reports.push(report));
    proc.drain_wpm_readout(&mut |report| reports.push(report));
    reports
}

fn tap(proc: &mut Processor, key: u8, timestamp: u64) -> Vec<HidReport> {
    let mut reports = event(proc, key, true, timestamp);
    reports.extend(event(proc, key, false, timestamp));
    reports
}

fn tick(proc: &mut Processor, timestamp: u64) -> Vec<HidReport> {
    let mut reports = Vec::new();
    proc.tick(timestamp, &mut |report| reports.push(report));
    proc.drain_wpm_readout(&mut |report| reports.push(report));
    reports
}

fn seed_120_wpm(proc: &mut Processor) {
    for timestamp in (0..=1_000).step_by(100) {
        tap(proc, 0, timestamp);
    }
    let stats = proc.typing_stats().expect("typing enabled");
    assert_eq!(stats.active_wpm, 120);
    assert_eq!(stats.active_ms, 1_000);
    assert_eq!(stats.characters, 11);
}

/// Decode wire-level keyboard changes as a host would: only rising usage edges
/// create characters. This catches missing releases and accidental reattacks.
struct Host {
    down: [bool; 128],
    text: String,
    attacks: Vec<(u8, u8)>,
    modifiers: u8,
}

impl Host {
    fn new() -> Self {
        Self {
            down: [false; 128],
            text: String::new(),
            attacks: Vec::new(),
            modifiers: 0,
        }
    }

    fn receive(&mut self, reports: &[HidReport]) {
        for report in reports {
            let mut down = [false; 128];
            let modifiers = match report {
                HidReport::Keyboard(report) => {
                    let bytes = report.to_bytes();
                    for &usage in &bytes[2..] {
                        if usage != 0 {
                            down[usize::from(usage)] = true;
                        }
                    }
                    bytes[0]
                }
                HidReport::NkroKeyboard(report) => {
                    let bytes = report.to_bytes();
                    for (usage, pressed) in down.iter_mut().enumerate() {
                        *pressed = bytes[1 + usage / 8] & (1 << (usage % 8)) != 0;
                    }
                    bytes[0]
                }
                _ => continue,
            };
            for usage in 1u8..128 {
                if down[usize::from(usage)] && !self.down[usize::from(usage)] {
                    self.attacks.push((usage, modifiers));
                    // Ctrl, Alt, and GUI produce shortcuts rather than text.
                    if modifiers & !0x22 == 0 {
                        let shifted = modifiers & 0x22 != 0;
                        let ch = match usage {
                            0x04..=0x1D => {
                                let base = if shifted { b'A' } else { b'a' };
                                Some(char::from(base + usage - 0x04))
                            }
                            0x1E..=0x27 => {
                                Some(char::from(b"1234567890"[usize::from(usage - 0x1E)]))
                            }
                            0x2C => Some(' '),
                            _ => None,
                        };
                        if let Some(ch) = ch {
                            self.text.push(ch);
                        }
                    }
                }
            }
            self.down = down;
            self.modifiers = modifiers;
        }
    }
}

#[test]
fn text_whitespace_and_correction_effort_form_a_gross_active_average() {
    let mut proc = configured(&[
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::Space),
        Action::Key(KeyCode::Tab),
        Action::Key(KeyCode::Enter),
        Action::Key(KeyCode::Backspace),
        Action::Key(KeyCode::Delete),
    ]);
    for (key, timestamp) in [(0, 0), (1, 100), (2, 200), (3, 300)] {
        tap(&mut proc, key, timestamp);
    }
    assert_eq!(proc.typing_stats().expect("enabled").characters, 4);
    assert_eq!(proc.typing_stats().expect("enabled").active_wpm, 120);
    tap(&mut proc, 4, 400);
    tap(&mut proc, 5, 500);
    let stats = proc.typing_stats().expect("enabled");
    assert_eq!(stats.characters, 4);
    assert_eq!(stats.active_ms, 500);
    assert_eq!(stats.active_wpm, 72);
}

#[test]
fn hours_of_idle_preserve_the_average_and_new_bursts_join_by_active_duration() {
    let mut proc = configured(&[Action::Key(KeyCode::A)]);
    seed_120_wpm(&mut proc);
    let before = proc.typing_stats();
    tick(&mut proc, 3_600_000);
    assert_eq!(proc.typing_stats(), before);
    tap(&mut proc, 0, 3_600_000);
    tap(&mut proc, 0, 3_601_000);
    let stats = proc.typing_stats().expect("enabled");
    assert_eq!(stats.characters, 13);
    assert_eq!(stats.active_ms, 2_000);
    assert_eq!(stats.active_wpm, 66);
}

#[test]
fn exactly_two_seconds_is_active_and_a_longer_gap_starts_a_fresh_burst() {
    let mut proc = configured(&[Action::Key(KeyCode::A)]);
    for timestamp in [0, 2_000, 4_001] {
        tap(&mut proc, 0, timestamp);
    }
    let stats = proc.typing_stats().expect("enabled");
    assert_eq!(stats.active_ms, 2_000);
    assert_eq!(stats.active_wpm, 6);
    tap(&mut proc, 0, 4_201);
    assert_eq!(proc.typing_stats().expect("enabled").active_ms, 2_200);
}

#[test]
fn modifiers_navigation_layers_and_midi_are_not_typed_characters() {
    let mut proc = configured(&[
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::Left),
        Action::Key(KeyCode::F1),
        Action::Modifier(ModifierKey::LSHIFT),
        Action::Layer(LayerOp::Momentary(1)),
        Action::Midi(MidiAction::NoteOn {
            channel: 0,
            note: 60,
            velocity: 88,
        }),
    ]);
    tap(&mut proc, 0, 0);
    for key in 1..6 {
        tap(&mut proc, key, 100 * u64::from(key));
    }
    let stats = proc.typing_stats().expect("enabled");
    assert_eq!(stats.characters, 1);
    assert_eq!(stats.active_ms, 0);
}

#[test]
fn control_alt_and_gui_shortcuts_are_ignored_but_shifted_text_counts() {
    let mut proc = configured(&[
        Action::Key(KeyCode::A),
        Action::Modifier(ModifierKey::LCTRL),
        Action::Modifier(ModifierKey::RALT),
        Action::Key(KeyCode::LGui),
        Action::Modifier(ModifierKey::LSHIFT),
    ]);
    for key in 1..=3 {
        let timestamp = 100 * u64::from(key);
        event(&mut proc, key, true, timestamp);
        tap(&mut proc, 0, timestamp + 10);
        event(&mut proc, key, false, timestamp + 20);
    }
    assert_eq!(proc.typing_stats().expect("enabled").characters, 0);
    event(&mut proc, 4, true, 400);
    tap(&mut proc, 0, 410);
    tap(&mut proc, 0, 610);
    event(&mut proc, 4, false, 620);
    let stats = proc.typing_stats().expect("enabled");
    assert_eq!(stats.characters, 2);
    assert_eq!(stats.active_wpm, 60);
}

#[test]
fn explicit_modified_text_and_physical_repeat_count_but_playback_does_not() {
    static WORD: &[KeyCode] = &[KeyCode::W, KeyCode::O, KeyCode::R, KeyCode::D];
    let mut proc = configured(&[
        Action::ModifiedKey {
            modifier: ModifierKey::LSHIFT,
            key: KeyCode::A,
        },
        Action::RepeatKey,
        Action::SendString(WORD),
        Action::MacroRecord(0),
        Action::MacroStop,
        Action::MacroPlay(0),
        Action::Key(KeyCode::B),
    ]);
    tap(&mut proc, 0, 0);
    tap(&mut proc, 1, 100);
    tap(&mut proc, 2, 200);
    tap(&mut proc, 3, 300);
    tap(&mut proc, 6, 400);
    tap(&mut proc, 4, 500);
    let before = proc.typing_stats();
    tap(&mut proc, 5, 600);
    assert_eq!(proc.typing_stats(), before);
    assert_eq!(before.expect("enabled").characters, 3);
}

#[test]
fn duplicate_scanner_down_and_analog_phase_changes_do_not_inflate_typing() {
    let mut proc = configured(&[Action::Dks(DksKeys {
        press: KeyCode::A,
        bottom_out: KeyCode::B,
        release_from_bottom: KeyCode::C,
        release: KeyCode::D,
    })]);
    event(&mut proc, 0, true, 0);
    event(&mut proc, 0, true, 10);
    proc.process_dks_phase(KeyPos::new(0, 0), TriggerPhase::BottomOut, &mut |_| {});
    proc.process_dks_phase(
        KeyPos::new(0, 0),
        TriggerPhase::ReleaseFromBottom,
        &mut |_| {},
    );
    event(&mut proc, 0, false, 100);
    assert_eq!(proc.typing_stats().expect("enabled").characters, 1);
}

#[test]
fn different_letters_in_one_scan_count_while_duplicate_down_edges_do_not() {
    let mut proc = configured(&[
        Action::Key(KeyCode::A),
        Action::Key(KeyCode::B),
        Action::Key(KeyCode::C),
        Action::Key(KeyCode::D),
    ]);
    event(&mut proc, 0, true, 0);
    event(&mut proc, 1, true, 0);
    event(&mut proc, 0, true, 10);
    event(&mut proc, 0, false, 20);
    event(&mut proc, 1, false, 20);
    event(&mut proc, 2, true, 200);
    event(&mut proc, 3, true, 200);
    event(&mut proc, 2, false, 220);
    event(&mut proc, 3, false, 220);
    let stats = proc.typing_stats().expect("enabled");
    assert_eq!(stats.characters, 4);
    assert_eq!(stats.active_ms, 200);
    assert_eq!(stats.active_wpm, 180);
}

#[test]
fn matched_tap_hold_combos_count_their_emitted_character_once() {
    static MEMBERS: &[KeyCode] = &[KeyCode::D, KeyCode::F];
    static MOD_TAP: &[Combo] = &[Combo::new(
        MEMBERS,
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::A,
        },
    )];
    static LAYER_TAP: &[Combo] = &[Combo::new(
        MEMBERS,
        Action::LayerTap {
            layer: 1,
            tap: KeyCode::A,
        },
    )];
    for combos in [MOD_TAP, LAYER_TAP] {
        let mut proc = configured(&[
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::F),
        ])
        .with_combos(ComboConfig::new(combos));
        let mut host = Host::new();
        host.receive(&tap(&mut proc, 0, 0));
        for timestamp in [100, 200] {
            host.receive(&event(&mut proc, 1, true, timestamp - 20));
            host.receive(&event(&mut proc, 2, true, timestamp));
            host.receive(&event(&mut proc, 1, false, timestamp + 10));
            host.receive(&event(&mut proc, 2, false, timestamp + 20));
        }
        assert_eq!(host.text, "caa");
        let stats = proc.typing_stats().expect("enabled");
        assert_eq!(stats.characters, 3);
        assert_eq!(stats.active_ms, 200);
        assert_eq!(stats.active_wpm, 120);
    }
}

#[test]
fn matched_combos_use_the_completing_physical_press_time_despite_processing_delays() {
    static MEMBERS: &[KeyCode] = &[KeyCode::D, KeyCode::F];
    static COMBOS: &[Combo] = &[Combo::key(MEMBERS, KeyCode::A)];
    let build = || {
        configured(&[
            Action::Key(KeyCode::C),
            Action::Key(KeyCode::D),
            Action::Key(KeyCode::F),
        ])
        .with_combos(ComboConfig::new(COMBOS))
    };
    let mut prompt = build();
    let mut delayed = build();
    for proc in [&mut prompt, &mut delayed] {
        tap(proc, 0, 0);
    }
    for (physical, processed) in [(100, 1_000), (200, 3_500)] {
        for (proc, now) in [(&mut prompt, physical), (&mut delayed, processed)] {
            delayed_event(proc, 1, true, physical - 20, now);
            delayed_event(proc, 2, true, physical, now);
            delayed_event(proc, 1, false, physical + 10, now + 10);
            delayed_event(proc, 2, false, physical + 20, now + 20);
        }
    }
    let stats = prompt.typing_stats().expect("enabled");
    assert_eq!(stats.characters, 3);
    assert_eq!(stats.active_ms, 200);
    assert_eq!(stats.active_wpm, 120);
    assert_eq!(delayed.typing_stats(), Some(stats));
}

#[test]
fn tap_hold_replay_uses_press_times_instead_of_the_later_decision_time() {
    let mut proc = configured(&[
        Action::Key(KeyCode::A),
        Action::ModTap {
            hold: ModifierKey::LSHIFT,
            tap: KeyCode::B,
        },
    ]);
    tap(&mut proc, 0, 0);
    event(&mut proc, 1, true, 100);
    tap(&mut proc, 0, 150);
    event(&mut proc, 1, false, 180);
    let stats = proc.typing_stats().expect("enabled");
    assert_eq!(stats.characters, 3);
    assert_eq!(stats.active_ms, 150);
    assert_eq!(stats.active_wpm, 160);
}

#[test]
fn rejected_slow_keys_are_ignored_and_accepted_keys_use_their_acceptance_time() {
    let mut proc = configured(&[Action::Key(KeyCode::A)])
        .with_accessibility(AccessibilityConfig::default().with_slow_keys(200));
    tap(&mut proc, 0, 0);
    tick(&mut proc, 200);
    assert_eq!(proc.typing_stats().expect("enabled").characters, 0);
    event(&mut proc, 0, true, 1_000);
    tick(&mut proc, 1_200);
    event(&mut proc, 0, false, 1_250);
    event(&mut proc, 0, true, 1_500);
    tick(&mut proc, 1_800);
    event(&mut proc, 0, false, 1_850);
    let stats = proc.typing_stats().expect("enabled");
    assert_eq!(stats.characters, 2);
    assert_eq!(stats.active_ms, 600);
    assert_eq!(stats.active_wpm, 20);
}

#[test]
fn readout_types_the_lifetime_average_without_counting_its_own_output() {
    let mut proc = configured(&[Action::Key(KeyCode::A), Action::TypeWpm]);
    seed_120_wpm(&mut proc);
    let before = proc.typing_stats();
    let mut host = Host::new();
    host.receive(&tap(&mut proc, 1, 3_600_000));
    assert_eq!(host.text, "120 WPM");
    assert_eq!(proc.typing_stats(), before);
    host.receive(&tap(&mut proc, 1, 7_200_000));
    assert_eq!(host.text, "120 WPM120 WPM");
    assert_eq!(proc.typing_stats(), before);
    assert!(!host.down.into_iter().any(|key| key));
}

#[test]
fn readout_waits_for_an_explicit_drain_and_is_emitted_only_once() {
    let mut proc = configured(&[Action::TypeWpm]);
    let mut ordinary = Vec::new();
    proc.process_event(KeyEvent::press(KeyPos::new(0, 0), 0), 0, &mut |report| {
        ordinary.push(report);
    });
    assert!(ordinary.is_empty());

    let mut text = Vec::new();
    proc.drain_wpm_readout(&mut |report| text.push(report));
    let mut host = Host::new();
    host.receive(&text);
    assert_eq!(host.text, "0 WPM");
    assert!(!host.down.into_iter().any(|key| key));
    proc.drain_wpm_readout(&mut |report| ordinary.push(report));
    assert!(ordinary.is_empty());
}

#[test]
#[cfg(feature = "autocorrect")]
fn readout_bypasses_the_correction_feed_and_dynamic_macro_recording() {
    let mut proc = configured(&[
        Action::Key(KeyCode::A),
        Action::TypeWpm,
        Action::MacroRecord(0),
        Action::MacroStop,
    ])
    .with_autocorrect_offload();
    seed_120_wpm(&mut proc);
    while proc.pop_typed_keycode().is_some() {}
    tap(&mut proc, 2, 1_100);
    let mut host = Host::new();
    host.receive(&tap(&mut proc, 1, 1_200));
    tap(&mut proc, 3, 1_300);
    assert_eq!(host.text, "120 WPM");
    assert_eq!(proc.pop_typed_keycode(), None);
    assert!(proc.macro_slot(0).expect("valid slot").is_empty());
    assert_eq!(proc.typing_stats().expect("enabled").characters, 11);
}

#[test]
fn unused_stats_read_zero_and_disabled_collection_has_no_readout() {
    let mut enabled = configured(&[Action::TypeWpm]);
    let mut host = Host::new();
    host.receive(&tap(&mut enabled, 0, 0));
    host.receive(&tap(&mut enabled, 0, 100));
    assert_eq!(host.text, "0 WPM0 WPM");
    let mut disabled: Processor = processor(&[&[Action::TypeWpm]]);
    assert!(tap(&mut disabled, 0, 0).is_empty());
    assert_eq!(disabled.typing_stats(), None);
    assert!(disabled.telemetry_frame().is_none());
    assert!(!disabled.clear_telemetry_deltas());
}

#[test]
fn readout_is_neutral_text_then_restores_a_physically_held_control_key() {
    let mut proc = configured(&[
        Action::Key(KeyCode::A),
        Action::TypeWpm,
        Action::Modifier(ModifierKey::LCTRL),
    ]);
    seed_120_wpm(&mut proc);
    let before = proc.typing_stats();
    let mut host = Host::new();
    host.receive(&event(&mut proc, 2, true, 1_100));
    host.receive(&tap(&mut proc, 1, 1_200));
    assert_eq!(host.text, "120 WPM");
    assert_eq!(host.modifiers, 0x01);
    assert!(host
        .attacks
        .iter()
        .all(|(_, modifiers)| modifiers & !0x22 == 0));
    host.receive(&tap(&mut proc, 0, 1_300));
    assert_eq!(host.attacks.last(), Some(&(0x04, 0x01)));
    assert_eq!(proc.typing_stats(), before);
    host.receive(&event(&mut proc, 2, false, 1_400));
    assert_eq!(host.modifiers, 0);
}

#[test]
fn readout_preserves_one_shot_caps_word_and_repeat_history() {
    let mut proc = configured(&[
        Action::Key(KeyCode::A),
        Action::TypeWpm,
        Action::OneShotMod(ModifierKey::LSHIFT),
        Action::CapsWord,
        Action::RepeatKey,
    ]);
    tap(&mut proc, 0, 0);
    tap(&mut proc, 2, 100);
    let mut host = Host::new();
    host.receive(&tap(&mut proc, 1, 200));
    host.receive(&tap(&mut proc, 0, 300));
    assert_eq!(host.text, "0 WPMA");
    tap(&mut proc, 3, 400);
    host.receive(&tap(&mut proc, 1, 500));
    host.receive(&tap(&mut proc, 4, 600));
    assert!(host.text.ends_with(" WPMA"));
    assert_eq!(host.attacks.last(), Some(&(0x04, 0x02)));
}

#[test]
fn pending_readout_waits_for_held_letters_and_keeps_the_requested_snapshot() {
    for mode in [RolloverMode::Kro6, RolloverMode::Nkro] {
        let mut proc = configured(&[
            Action::Key(KeyCode::A),
            Action::TypeWpm,
            Action::Key(KeyCode::B),
        ])
        .with_rollover_mode(mode);
        seed_120_wpm(&mut proc);
        let mut host = Host::new();
        host.receive(&event(&mut proc, 0, true, 1_100));
        let requested = proc.typing_stats().expect("enabled").active_wpm;
        assert!(event(&mut proc, 1, true, 1_150).is_empty());
        host.receive(&tap(&mut proc, 2, 2_000));
        assert_ne!(proc.typing_stats().expect("enabled").active_wpm, requested);
        host.receive(&event(&mut proc, 1, false, 2_050));
        assert_eq!(host.text, "ab");
        host.receive(&event(&mut proc, 0, false, 2_100));
        assert_eq!(host.text, format!("ab{requested} WPM"));
        assert!(!host.down.into_iter().any(|key| key));
    }
}

#[test]
fn five_digit_readout_fits_the_runtime_report_batch_and_releases_every_usage() {
    for mode in [RolloverMode::Kro6, RolloverMode::Nkro] {
        for (characters, elapsed, expected) in [(2, 1, "12000 WPM"), (1_001, 1_080, "11111 WPM")] {
            let mut proc =
                configured(&[Action::Key(KeyCode::A), Action::TypeWpm]).with_rollover_mode(mode);
            for index in 0..characters {
                tap(&mut proc, 0, index * elapsed / (characters - 1));
            }
            let reports = tap(&mut proc, 1, elapsed + 1);
            assert!(reports.len() <= 16);
            let mut host = Host::new();
            host.receive(&reports);
            assert_eq!(host.text, expected);
            assert!(!host.down.into_iter().any(|key| key));
            assert_eq!(host.modifiers, 0);
            assert_eq!(proc.typing_stats().expect("enabled").characters, characters);
        }
    }
}

#[test]
fn lightweight_telemetry_survives_delta_acknowledgement_and_repeated_opt_in() {
    let mut proc = configured(&[Action::Key(KeyCode::A)]);
    seed_120_wpm(&mut proc);
    let expected = proc.typing_stats();
    assert!(proc.clear_telemetry_deltas());
    assert_eq!(proc.telemetry_frame().expect("enabled").typing, expected);
    proc = proc.with_typing_stats();
    assert_eq!(proc.typing_stats(), expected);
    tap(&mut proc, 0, 1_100);
    assert_eq!(proc.typing_stats().expect("enabled").characters, 12);
    assert_eq!(proc.typing_stats().expect("enabled").active_wpm, 120);
}

#[test]
#[cfg(feature = "processor-telemetry")]
fn full_telemetry_opt_in_also_enables_lightweight_typing() {
    let mut proc: Processor = processor(&[&[Action::Key(KeyCode::A)]]);
    proc = proc.with_telemetry();
    seed_120_wpm(&mut proc);
    let frame = proc.telemetry_frame().expect("enabled");
    assert_eq!(frame.core.expect("detailed telemetry").total_presses, 11);
    assert_eq!(frame.typing, proc.typing_stats());
}
