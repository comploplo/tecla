//! Bounded, event-driven observations for the local Harmony monitor.

use core::fmt::{self, Write};

use tecla::music::harmony::Chord;
use tecla::music::realization::Preferences;
use tecla::pipeline::report::MidiEvent;
use tecla::rgb::Rgb;

use super::harmony_instrument::Snapshot;
use super::instrument::diagnostics::DiagnosticStatus;

/// Serialize one coherent instrument snapshot; MIDI events are attempted writes.
/// `tx_fault` distinguishes a failed batch from successful note delivery.
pub fn encode(
    snapshot: Snapshot,
    status: DiagnosticStatus,
    events: &[MidiEvent],
    leds: &[Rgb; 52],
    out: &mut heapless::String<2560>,
) -> fmt::Result {
    out.clear();
    write!(out, "{{\"v\":2,\"seq\":{},\"event_ms\":{},\"time_ms\":{},\"cause\":{},\"physical\":{},\"accepted\":{},", snapshot.sequence, snapshot.time_ms, status.now_ms, snapshot.cause, snapshot.physical, snapshot.accepted)?;
    write!(out, "\"transport\":{{\"ready\":{},\"midi_ready\":{},\"right_ok\":{},\"right_age_ms\":{},\"session\":{},\"recovered\":{},\"tx_fault\":{},\"dropped\":{},\"update_us\":{}}},", status.ready, status.midi_ready, status.right_ok, status.right_age_ms, status.usb_session, status.recovered, status.tx_fault, status.dropped, status.update_us)?;
    out.push_str("\"captured\":").map_err(|_| fmt::Error)?;
    notes(out, &snapshot.captured)?;
    out.push_str(",\"support\":").map_err(|_| fmt::Error)?;
    notes(out, &snapshot.support)?;
    write!(out, ",\"limited\":{}", snapshot.diagnostics.limited)?;
    out.push_str(",\"requested\":").map_err(|_| fmt::Error)?;
    preferences(out, snapshot.requested)?;
    out.push_str(",\"applied\":").map_err(|_| fmt::Error)?;
    preferences(out, snapshot.diagnostics.applied)?;
    out.push_str(",\"reference\":").map_err(|_| fmt::Error)?;
    chord(out, snapshot.diagnostics.reference)?;
    out.push_str(",\"chord\":").map_err(|_| fmt::Error)?;
    chord(out, snapshot.diagnostics.current.chord)?;
    out.push_str(",\"destination\":").map_err(|_| fmt::Error)?;
    chord(out, snapshot.diagnostics.current.destination)?;
    let suspended = snapshot
        .diagnostics
        .current
        .suspended
        .iter()
        .enumerate()
        .fold(0u8, |mask, (voice, held)| mask | (u8::from(*held) << voice));
    write!(
        out,
        ",\"suspended\":{suspended},\"root_implied\":{},\"resolved\":",
        snapshot.diagnostics.current.root_implied
    )?;
    notes(out, &snapshot.diagnostics.resolved)?;
    out.push_str(",\"tendencies\":").map_err(|_| fmt::Error)?;
    notes(out, &snapshot.diagnostics.tendencies)?;
    out.push_str(",\"events\":[").map_err(|_| fmt::Error)?;
    for (index, event) in events.iter().enumerate() {
        if index > 0 {
            out.push(',').map_err(|_| fmt::Error)?;
        }
        let [cin, status, note, velocity] = event.to_usb_packet();
        write!(out, "[{cin},{status},{note},{velocity}]")?;
    }
    out.push_str("],\"leds\":\"").map_err(|_| fmt::Error)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for colour in leds {
        for value in [colour.r, colour.g, colour.b] {
            out.push(char::from(HEX[usize::from(value >> 4)]))
                .map_err(|_| fmt::Error)?;
            out.push(char::from(HEX[usize::from(value & 15)]))
                .map_err(|_| fmt::Error)?;
        }
    }
    out.push_str("\"}\n").map_err(|_| fmt::Error)
}

fn optional(out: &mut heapless::String<2560>, note: Option<u8>) -> fmt::Result {
    match note {
        Some(note) => write!(out, "{note}"),
        None => out.push_str("null").map_err(|_| fmt::Error),
    }
}

fn notes(out: &mut heapless::String<2560>, values: &[Option<u8>]) -> fmt::Result {
    out.push('[').map_err(|_| fmt::Error)?;
    for (index, note) in values.iter().enumerate() {
        if index > 0 {
            out.push(',').map_err(|_| fmt::Error)?;
        }
        optional(out, *note)?;
    }
    out.push(']').map_err(|_| fmt::Error)
}

fn preferences(out: &mut heapless::String<2560>, prefs: Preferences) -> fmt::Result {
    write!(out, "{{\"colour\":\"{:?}\",\"foundation\":\"{:?}\",\"motion\":\"{:?}\",\"reharm\":\"{:?}\",\"direction\":\"{:?}\",\"delay\":{},\"pedal\":{}}}", prefs.colour, prefs.foundation, prefs.motion, prefs.reharm, prefs.direction, prefs.delay, prefs.pedal)
}

fn chord(out: &mut heapless::String<2560>, value: Option<Chord>) -> fmt::Result {
    match value {
        Some(value) => write!(
            out,
            "{{\"root\":{},\"shape\":\"{:?}\",\"tones\":{}}}",
            value.root,
            value.shape,
            value.tones().bits()
        ),
        None => out.push_str("null").map_err(|_| fmt::Error),
    }
}

#[cfg(test)]
mod tests {
    use super::super::harmony_instrument::HarmonyInstrument;
    use super::*;
    use tecla::music::harmony::Shape;
    use tecla::music::realization::{Colour, Direction, Foundation, Motion, Reharm};

    #[test]
    fn largest_restrike_with_long_intents_and_transport_counters_fits() {
        let mut snapshot = HarmonyInstrument::new().snapshot();
        snapshot.sequence = u32::MAX;
        snapshot.time_ms = u64::MAX;
        snapshot.cause = u16::MAX;
        snapshot.physical = (1u64 << 52) - 1;
        snapshot.accepted = snapshot.physical;
        snapshot.captured = [Some(127); 24];
        snapshot.captured[0] = None;
        snapshot.support = [Some(127); 6];
        snapshot.requested = Preferences {
            colour: Colour::SecondsFourths,
            foundation: Foundation::Seventh,
            motion: Motion::Outward,
            reharm: Reharm::AppliedDominant,
            direction: Direction::Substitute,
            delay: false,
            pedal: false,
        };
        snapshot.diagnostics.applied = snapshot.requested;
        snapshot.diagnostics.limited = 15;
        snapshot.diagnostics.reference = Chord::new(11, Shape::HalfDiminished7);
        snapshot.diagnostics.current.chord = snapshot.diagnostics.reference;
        snapshot.diagnostics.current.destination = snapshot.diagnostics.reference;
        snapshot.diagnostics.current.suspended = [true; 6];
        snapshot.diagnostics.resolved = [Some(127); 6];
        snapshot.diagnostics.tendencies = [Some(127); 6];
        let status = DiagnosticStatus {
            now_ms: u64::MAX,
            ready: false,
            right_age_ms: u64::MAX,
            right_ok: false,
            usb_session: u32::MAX,
            midi_ready: false,
            recovered: false,
            tx_fault: false,
            dropped: u32::MAX,
            update_us: u32::MAX,
        };
        // At most 23 literal notes plus six supporting voices can be restruck.
        let mut events = heapless::Vec::<MidiEvent, 58>::new();
        for _ in 0..29 {
            events
                .push(MidiEvent::NoteOff {
                    channel: 0,
                    note: 127,
                })
                .unwrap();
        }
        for _ in 0..29 {
            events
                .push(MidiEvent::NoteOn {
                    channel: 0,
                    note: 127,
                    velocity: 88,
                })
                .unwrap();
        }
        let mut out = heapless::String::new();
        encode(snapshot, status, &events, &[Rgb::BLACK; 52], &mut out).unwrap();
        let frame: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(frame["v"], 2);
        assert_eq!(frame["limited"], 15);
        assert!(frame.get("answer").is_none());
        assert_eq!(frame["requested"]["colour"], "SecondsFourths");
        assert_eq!(frame["requested"]["reharm"], "AppliedDominant");
        assert_eq!(frame["requested"]["direction"], "Substitute");
        assert_eq!(frame["events"].as_array().unwrap().len(), events.len());
        for (actual, expected) in frame["events"].as_array().unwrap().iter().zip(events) {
            assert_eq!(*actual, serde_json::json!(expected.to_usb_packet()));
        }
        assert_eq!(frame["leds"].as_str().unwrap().len(), 312);
        assert!(out.ends_with('\n'));
    }
}
