//! Bounded USB-MIDI packet packing and session-safe asynchronous writes.

use crate::pipeline::report::{MidiEvent, MIDI_PACKET_SIZE};
use core::future::{poll_fn, Future};
use core::task::Poll;

/// Events per USB transfer, keeping every packet shorter than its 64-byte endpoint.
pub const PACKET_EVENTS: usize = 15;
/// Scratch buffer size for one short USB-MIDI transfer.
pub const PACKET_BYTES: usize = PACKET_EVENTS * MIDI_PACKET_SIZE;

/// Packs events without changing their order or the shared MIDI wire encoding.
pub fn pack_events(events: &[MidiEvent], buffer: &mut [u8; PACKET_BYTES]) -> Option<usize> {
    let len = events.len().checked_mul(MIDI_PACKET_SIZE)?;
    let bytes = buffer.get_mut(..len)?;
    for (packet, event) in bytes
        .as_chunks_mut::<MIDI_PACKET_SIZE>()
        .0
        .iter_mut()
        .zip(events)
    {
        packet.copy_from_slice(&event.to_usb_packet());
    }
    Some(len)
}

/// Drops a pending write before it can be polled in a different USB session.
pub async fn in_session<F: Future>(
    session: u32,
    current_session: impl Fn() -> u32,
    write: F,
) -> Option<F::Output> {
    let mut write = core::pin::pin!(write);
    poll_fn(|cx| {
        if session & 1 == 0 || current_session() != session {
            Poll::Ready(None)
        } else {
            write.as_mut().poll(cx).map(Some)
        }
    })
    .await
}
