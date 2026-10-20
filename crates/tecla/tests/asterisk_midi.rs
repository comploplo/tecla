#![allow(
    clippy::tests_outside_test_module,
    reason = "integration tests live in tests/ by convention"
)]
#![allow(missing_docs, reason = "integration test")]
#![expect(clippy::unwrap_used, reason = "test assertions")]

// Exercise the actual firmware map and output helpers, rather than a test map.
#[path = "../../../keyboards/tecla/asterisk_v1_4_midi/output.rs"]
mod output;
#[path = "../../../keyboards/tecla/asterisk_v1_4_midi/keymaps/midi.rs"]
mod pads;

use tecla::devices::keyboard::tapping::TapHoldConfig;
use tecla::devices::midi::PadMidiOutput;
use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::report::{HidReport, MidiEvent};

type Processor = KeyboardProcessor<1, { pads::KEYS }, 1, { pads::KEYS }>;

fn processor() -> Processor {
    Processor::new(pads::keymap(), TapHoldConfig::default())
}

fn note_on(note: u8) -> MidiEvent {
    MidiEvent::NoteOn {
        channel: pads::CHANNEL,
        note,
        velocity: pads::VELOCITY,
    }
}

fn note_off(note: u8) -> MidiEvent {
    MidiEvent::NoteOff {
        channel: pads::CHANNEL,
        note,
    }
}

fn edge(processor: &mut Processor, col: u8, pressed: bool, now: u64) -> Vec<MidiEvent> {
    let pos = KeyPos::new(0, col);
    let event = if pressed {
        KeyEvent::press(pos, now)
    } else {
        KeyEvent::release(pos, now)
    };
    let mut events = Vec::new();
    processor.process_event(event, now, &mut |report| {
        assert!(
            matches!(report, HidReport::Midi(_)),
            "unexpected HID output"
        );
        if let HidReport::Midi(event) = report {
            events.push(event);
        }
    });
    events
}

fn cleanup(output: &mut PadMidiOutput<25>, session: u32) -> Vec<MidiEvent> {
    let mut events = Vec::new();
    while let Some(event) = output.next_cleanup() {
        events.push(event);
        assert!(output.cleanup_sent(session, session));
    }
    assert!(output.is_ready());
    assert_eq!(events, pads::NOTES.map(note_off));
    events
}

#[test]
fn every_pad_repeats_and_releases_its_exact_note_and_stop_emits_no_action() {
    let mut processor = processor();
    for col in 0..pads::STOP_COL {
        let note = pads::NOTES[usize::from(col)];
        for cycle in 0..3 {
            let now = u64::from(col) * 100 + cycle * 10;
            assert_eq!(edge(&mut processor, col, true, now), [note_on(note)]);
            assert_eq!(edge(&mut processor, col, false, now + 5), [note_off(note)]);
        }
    }
    assert!(edge(&mut processor, pads::STOP_COL, true, 3000).is_empty());
    assert!(edge(&mut processor, pads::STOP_COL, false, 3010).is_empty());
}

#[test]
fn all_25_notes_fit_one_scan_and_two_short_usb_transfers_in_order() {
    let mut processor = processor();
    for pressed in [true, false] {
        // Collect a full physical snapshot before packetizing its MIDI events.
        let mut batch: heapless::Vec<MidiEvent, { pads::KEYS }> = heapless::Vec::new();
        for col in 0..pads::STOP_COL {
            for event in edge(&mut processor, col, pressed, 100) {
                batch.push(event).unwrap();
            }
        }
        let expected = pads::NOTES.map(if pressed { note_on } else { note_off });
        assert_eq!(batch.as_slice(), expected);

        let mut bytes = Vec::new();
        let mut lengths = Vec::new();
        for chunk in batch.chunks(output::PACKET_EVENTS) {
            let mut packet = [0; output::PACKET_BYTES];
            let len = output::pack_events(chunk, &mut packet).unwrap();
            lengths.push(len);
            bytes.extend_from_slice(&packet[..len]);
        }
        assert_eq!(lengths, [60, 40]);
        assert_eq!(
            bytes,
            expected
                .into_iter()
                .flat_map(MidiEvent::to_usb_packet)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn oversized_packet_is_rejected_without_overwriting_the_buffer() {
    let mut packet = [0xAA; output::PACKET_BYTES];
    let events = [note_on(60); output::PACKET_EVENTS + 1];
    assert_eq!(output::pack_events(&events, &mut packet), None);
    assert_eq!(packet, [0xAA; output::PACKET_BYTES]);
}

#[test]
fn pending_write_cannot_commit_an_attack_after_usb_session_changes() {
    use core::cell::Cell;
    use core::future::{poll_fn, Future};
    use core::task::{Context, Poll, Waker};

    // Model the RP endpoint: first poll waits for its previous packet, and its
    // next poll would enqueue bytes. A reset must drop it before that poll.
    let session = Cell::new(1);
    let polls = Cell::new(0);
    let write = poll_fn(|_| {
        polls.set(polls.get() + 1);
        if polls.get() == 1 {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    });
    let mut guarded = core::pin::pin!(output::in_session(1, || session.get(), write));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(guarded.as_mut().poll(&mut cx).is_pending());
    // Both a disconnect and a full disconnect/reconnect invalidate the write.
    session.set(3);
    assert_eq!(guarded.as_mut().poll(&mut cx), Poll::Ready(None));
    assert_eq!(polls.get(), 1);

    let mut fresh = core::pin::pin!(output::in_session(3, || session.get(), async { 42 }));
    assert_eq!(fresh.as_mut().poll(&mut cx), Poll::Ready(Some(42)));
}

#[test]
fn offline_and_stop_cleanup_do_not_replay_attacks_or_lose_physical_releases() {
    let mut processor = processor();
    let mut output = PadMidiOutput::new(pads::CHANNEL, pads::NOTES).unwrap();

    // Offline input still reaches the processor, but its attack is discarded.
    assert_eq!(edge(&mut processor, 0, true, 0), [note_on(48)]);
    assert!(!output.is_ready());
    output.observe_session(1);
    cleanup(&mut output, 1);
    let mut held_reports = Vec::new();
    processor.tick(100, &mut |report| held_reports.push(report));
    assert!(held_reports.is_empty());
    assert_eq!(edge(&mut processor, 0, false, 101), [note_off(48)]);
    assert_eq!(edge(&mut processor, 0, true, 102), [note_on(48)]);

    // Stop clears even an already sounding note. A release during cleanup is
    // consumed immediately, so the next genuine press remains playable.
    assert!(edge(&mut processor, pads::STOP_COL, true, 103).is_empty());
    output.restart_cleanup();
    assert!(!output.is_ready());
    assert_eq!(edge(&mut processor, 0, false, 104), [note_off(48)]);
    cleanup(&mut output, 1);
    assert!(edge(&mut processor, pads::STOP_COL, false, 105).is_empty());
    assert_eq!(edge(&mut processor, 0, true, 106), [note_on(48)]);

    // A failed attack/release batch and a new USB session both require a full
    // sweep. Neither path stores old attacks for later delivery.
    output.restart_cleanup();
    assert!(!output.is_ready());
    cleanup(&mut output, 1);
    output.observe_session(2);
    assert_eq!(output.next_cleanup(), None);
    assert_eq!(edge(&mut processor, 0, false, 107), [note_off(48)]);
    output.observe_session(3);
    cleanup(&mut output, 3);
    assert_eq!(edge(&mut processor, 0, true, 108), [note_on(48)]);
}
