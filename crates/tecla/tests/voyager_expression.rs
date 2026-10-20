#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test assertions")]

#[path = "../../../keyboards/tecla/voyager_tonnetz/expression.rs"]
mod expression;
#[path = "../../../keyboards/tecla/voyager_tonnetz/instrument.rs"]
mod instrument;

use expression::{Expression, MotionGate};
use instrument::{Instrument, Orientation};
use tecla::devices::midi::output;
use tecla::matrix::MatrixState;
use tecla::pipeline::report::MidiEvent;

fn cc(controller: u8, value: u8) -> MidiEvent {
    MidiEvent::Cc {
        channel: 0,
        controller,
        value,
    }
}

fn send(expression: &mut Expression, now: u64) -> Vec<MidiEvent> {
    let Some(frame) = expression.pending(now) else {
        return Vec::new();
    };
    let events = frame.events().collect();
    assert!(expression.commit(frame));
    events
}

#[test]
fn motion_gate_preserves_its_boundary_until_disabled_and_rearms_on_recovery() {
    let mut gate = MotionGate::new();
    assert!(!gate.accepts(0));
    assert!(!gate.accepts(100));
    gate.update(true, 100);
    assert!(!gate.accepts(99));
    assert!(gate.accepts(100));
    gate.update(true, 150);
    assert!(gate.accepts(100));
    gate.update(false, 200);
    assert!(!gate.accepts(250));
    gate.update(true, 300);
    assert!(!gate.accepts(299));
    assert!(gate.accepts(300));
    gate.update(true, 350);
    assert!(gate.accepts(300));
}

#[test]
fn a_sensor_poll_straddling_stop_or_recovery_cannot_replay_old_motion() {
    let mut gate = MotionGate::new();
    gate.update(true, 100);
    // This sample arrives after enable, but includes counts accumulated before it.
    assert!(!gate.accepts(93));
    assert!(gate.accepts(100));
    gate.update(false, 115);
    assert!(!gate.accepts(114));
    gate.update(true, 120);
    assert!(!gate.accepts(114));
    // The following sensor interval lies wholly after the new boundary.
    assert!(gate.accepts(121));
    gate.update(false, 130);
    assert!(!gate.accepts(135));
}

#[test]
fn initial_controls_are_audible_and_use_existing_usb_midi_encoding() {
    let mut expression = Expression::new();
    assert!(!expression.is_synced());
    let events = send(&mut expression, 0);
    assert!(expression.is_synced());
    assert_eq!(events, [cc(74, 64), cc(11, 127)]);
    assert_eq!(events[0].to_usb_packet(), [0x0b, 0xb0, 74, 64]);
    assert_eq!(events[1].to_usb_packet(), [0x0b, 0xb0, 11, 127]);
    for now in 1..100 {
        assert!(send(&mut expression, now).is_empty());
    }
}

#[test]
fn initial_failed_write_waits_before_retrying_and_does_not_open_sync_gate() {
    let mut expression = Expression::new();
    let failed = expression.pending(0).unwrap();
    assert_eq!(
        failed.events().collect::<Vec<_>>(),
        [cc(74, 64), cc(11, 127)]
    );
    assert!(!expression.is_synced());
    expression.motion(64, 64);
    assert!(expression.pending(15).is_none());
    assert_eq!(send(&mut expression, 16), [cc(74, 66), cc(11, 125)]);
    assert!(expression.is_synced());
}

#[test]
fn small_motion_accumulates_and_axes_only_emit_their_own_changes() {
    let mut expression = Expression::new();
    send(&mut expression, 0);
    for _ in 0..32 {
        expression.motion(1, 0);
    }
    assert!(expression.is_synced());
    assert_eq!(send(&mut expression, 16), [cc(74, 65)]);
    for _ in 0..32 {
        expression.motion(0, 1);
    }
    assert_eq!(send(&mut expression, 32), [cc(11, 126)]);
    expression.motion(-32, -32);
    assert_eq!(send(&mut expression, 48), [cc(74, 64), cc(11, 127)]);
}

#[test]
fn symmetric_jitter_does_not_chatter_even_at_a_quantization_boundary() {
    let mut expression = Expression::new();
    send(&mut expression, 0);
    for now in 16..128 {
        expression.motion(1, -1);
        assert!(send(&mut expression, now).is_empty());
        expression.motion(-1, 1);
        assert!(send(&mut expression, now).is_empty());
    }
    expression.motion(20, 0);
    assert_eq!(send(&mut expression, 128), [cc(74, 65)]);
    for now in 144..256 {
        expression.motion(-1, 0);
        assert!(send(&mut expression, now).is_empty());
        expression.motion(1, 0);
        assert!(send(&mut expression, now).is_empty());
    }
    expression.motion(12, 0);
    assert!(send(&mut expression, 272).is_empty());
    expression.motion(32, 0);
    assert_eq!(send(&mut expression, 288), [cc(74, 66)]);
}

#[test]
fn large_motion_clamps_without_windup_or_signed_overflow() {
    let mut expression = Expression::new();
    send(&mut expression, 0);
    expression.motion(i16::MAX, i16::MIN);
    assert_eq!(send(&mut expression, 16), [cc(74, 127)]);
    expression.motion(20, -20);
    assert!(send(&mut expression, 32).is_empty());
    expression.motion(-32, 32);
    assert_eq!(send(&mut expression, 48), [cc(74, 126), cc(11, 126)]);
    expression.motion(i16::MIN, i16::MAX);
    assert_eq!(send(&mut expression, 64), [cc(74, 0), cc(11, 0)]);
    expression.motion(-20, 20);
    assert!(send(&mut expression, 80).is_empty());
    expression.motion(32, -32);
    assert_eq!(send(&mut expression, 96), [cc(74, 1), cc(11, 1)]);
}

#[test]
fn rate_limit_coalesces_motion_and_sends_at_the_exact_deadline() {
    let mut expression = Expression::new();
    send(&mut expression, 100);
    for now in 101..116 {
        expression.motion(32, 32);
        assert!(send(&mut expression, now).is_empty());
    }
    assert_eq!(send(&mut expression, 116), [cc(74, 79), cc(11, 112)]);
    assert!(send(&mut expression, 132).is_empty());
}

#[test]
fn a_failed_write_does_not_drop_or_acknowledge_the_pending_controls() {
    let mut expression = Expression::new();
    send(&mut expression, 0);
    expression.motion(32, 32);
    let failed = expression.pending(16).unwrap();
    assert_eq!(
        failed.events().collect::<Vec<_>>(),
        [cc(74, 65), cc(11, 126)]
    );
    // No commit: the USB endpoint did not confirm this write.
    expression.motion(64, 64);
    for now in 17..32 {
        assert!(send(&mut expression, now).is_empty());
    }
    assert_eq!(send(&mut expression, 32), [cc(74, 67), cc(11, 124)]);
    assert!(send(&mut expression, 48).is_empty());
}

#[test]
fn acknowledging_a_snapshot_keeps_later_motion_pending() {
    let mut expression = Expression::new();
    send(&mut expression, 0);
    expression.motion(32, 0);
    let in_flight = expression.pending(16).unwrap();
    expression.motion(32, 32);
    assert!(expression.commit(in_flight));
    assert!(expression.pending(31).is_none());
    assert_eq!(send(&mut expression, 32), [cc(74, 66), cc(11, 126)]);
}

#[test]
fn recovery_sends_the_latest_latched_pair_and_rejects_an_old_acknowledgment() {
    let mut expression = Expression::new();
    send(&mut expression, 0);
    expression.motion(32, 0);
    let in_flight = expression.pending(16).unwrap();
    expression.invalidate();
    assert!(!expression.is_synced());
    expression.motion(64, 64);
    assert!(!expression.commit(in_flight));
    assert_eq!(send(&mut expression, 17), [cc(74, 67), cc(11, 125)]);
    assert!(expression.is_synced());
    expression.invalidate();
    // A new session receives both values even immediately after the old send.
    assert_eq!(send(&mut expression, 18), [cc(74, 67), cc(11, 125)]);
    assert!(send(&mut expression, 1000).is_empty());
}

fn keys(
    instrument: &mut Instrument,
    positions: &[(usize, usize)],
    ready: bool,
) -> heapless::Vec<MidiEvent, { instrument::MAX_EVENTS }> {
    let mut state = MatrixState::new();
    for &(row, col) in positions {
        assert!(state.set(row, col, true));
    }
    let mut events = heapless::Vec::new();
    instrument.update(&state, ready, &mut |event| events.push(event).unwrap());
    events
}

fn wire(events: &[MidiEvent]) -> Vec<u8> {
    let mut packet = [0; output::PACKET_BYTES];
    let len = output::pack_events(events, &mut packet).unwrap();
    packet[..len].to_vec()
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

#[test]
fn expression_and_cc_backpressure_preserve_both_layouts_held_and_damper_notes() {
    // Independently transcribed physical positions: left D = C3, right K =
    // C4 in the mirrored map and Db5 in the parallel map, damper = (11,5).
    for (orientation, right_pitch) in [(Orientation::Mirrored, 60), (Orientation::Parallel, 73)] {
        let mut instrument = Instrument::new(orientation);
        let mut expression = Expression::new();
        send(&mut expression, 0);
        assert_eq!(
            keys(&mut instrument, &[(2, 4), (8, 2)], true).as_slice(),
            [on(48, 88), on(right_pitch, 88)]
        );
        assert!(keys(&mut instrument, &[(2, 4), (8, 2), (11, 5)], true).is_empty());
        assert!(keys(&mut instrument, &[(8, 2)], true).is_empty());
        let colours = instrument.frame();

        expression.motion(128, 96);
        let failed = expression.pending(16).unwrap();
        let controls: Vec<_> = failed.events().collect();
        assert_eq!(wire(&controls), [0x0b, 0xb0, 74, 68, 0x0b, 0xb0, 11, 124]);
        // A failed absolute-control write does not invalidate note ownership.
        assert!(keys(&mut instrument, &[(8, 2)], expression.is_synced()).is_empty());
        assert_eq!(instrument.frame(), colours);
        expression.motion(32, 32);
        assert!(expression.pending(31).is_none());
        let retried = send(&mut expression, 32);
        assert_eq!(wire(&retried), [0x0b, 0xb0, 74, 69, 0x0b, 0xb0, 11, 123]);
        assert!(keys(&mut instrument, &[(8, 2)], expression.is_synced()).is_empty());
        assert_eq!(instrument.frame(), colours);

        // The retained left note is still owned until damper is switched off;
        // the physically held right note survives that release independently.
        assert_eq!(
            keys(&mut instrument, &[(8, 2), (11, 5)], true).as_slice(),
            [off(48)]
        );
        assert_eq!(
            keys(&mut instrument, &[], true).as_slice(),
            [off(right_pitch)]
        );
    }
}

#[test]
fn expression_leaves_all_four_physical_thumb_roles_and_stop_intact() {
    for (orientation, right_pitch) in [(Orientation::Mirrored, 60), (Orientation::Parallel, 73)] {
        let mut instrument = Instrument::new(orientation);
        let mut expression = Expression::new();
        send(&mut expression, 0);
        expression.motion(-320, 320);
        assert_eq!(
            wire(&send(&mut expression, 16)),
            [0x0b, 0xb0, 74, 54, 0x0b, 0xb0, 11, 117]
        );

        assert_eq!(
            keys(&mut instrument, &[(8, 2)], true).as_slice(),
            [on(right_pitch, 88)]
        );
        // Down + accent affect only the new left E attack, not the held right K.
        assert_eq!(
            keys(&mut instrument, &[(8, 2), (5, 0), (11, 6), (1, 4)], true).as_slice(),
            [on(40, 120)]
        );
        assert_eq!(
            keys(&mut instrument, &[(8, 2), (5, 0), (11, 6)], true).as_slice(),
            [off(40)]
        );
        // Both octave thumbs cancel. Releasing accent restores normal velocity.
        assert_eq!(
            keys(&mut instrument, &[(8, 2), (5, 0), (5, 1), (1, 5)], true).as_slice(),
            [on(55, 88)]
        );
        assert!(keys(
            &mut instrument,
            &[(8, 2), (5, 0), (5, 1), (1, 5), (11, 5)],
            true
        )
        .is_empty());
        assert!(keys(&mut instrument, &[], true).is_empty());
        assert_eq!(
            keys(&mut instrument, &[(0, 1)], true).as_slice(),
            [off(55), off(right_pitch)]
        );
        // Stop clears damper, so this subsequent note releases normally.
        assert!(keys(&mut instrument, &[], true).is_empty());
        assert_eq!(
            keys(&mut instrument, &[(2, 4)], true).as_slice(),
            [on(48, 88)]
        );
        assert_eq!(keys(&mut instrument, &[], true).as_slice(), [off(48)]);
        assert!(expression.pending(1000).is_none());
    }
}

#[test]
fn recovery_syncs_latched_controls_before_rearming_fresh_literal_notes() {
    for (orientation, right_pitch) in [(Orientation::Mirrored, 60), (Orientation::Parallel, 73)] {
        let mut instrument = Instrument::new(orientation);
        let mut expression = Expression::new();
        send(&mut expression, 0);
        expression.motion(-320, 320);
        send(&mut expression, 16);
        let held = [(2, 4), (8, 2)];
        assert_eq!(
            keys(&mut instrument, &held, expression.is_synced()).as_slice(),
            [on(48, 88), on(right_pitch, 88)]
        );
        expression.invalidate();
        assert_eq!(
            keys(&mut instrument, &held, expression.is_synced()).as_slice(),
            [off(48), off(right_pitch)]
        );
        // A first CC write can fail without opening the initial note gate.
        let failed = expression.pending(17).unwrap();
        assert_eq!(
            wire(&failed.events().collect::<Vec<_>>()),
            [0x0b, 0xb0, 74, 54, 0x0b, 0xb0, 11, 117]
        );
        assert!(keys(&mut instrument, &[], expression.is_synced()).is_empty());
        assert!(keys(&mut instrument, &held, expression.is_synced()).is_empty());
        let resync = expression.pending(33).unwrap();
        assert_eq!(
            wire(&resync.events().collect::<Vec<_>>()),
            [0x0b, 0xb0, 74, 54, 0x0b, 0xb0, 11, 117]
        );
        assert!(expression.commit(resync));
        // Synchronisation does not resurrect keys held through recovery.
        assert!(keys(&mut instrument, &held, expression.is_synced()).is_empty());
        assert!(keys(&mut instrument, &[], expression.is_synced()).is_empty());
        assert_eq!(
            keys(&mut instrument, &held, expression.is_synced()).as_slice(),
            [on(48, 88), on(right_pitch, 88)]
        );
    }
}
