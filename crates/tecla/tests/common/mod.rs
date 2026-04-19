#![allow(
    dead_code,
    reason = "shared test helpers — not all used in every test file"
)]

use tecla::keyboard::KeyboardProcessor;
use tecla::pipeline::event::{KeyEvent, KeyPos};
use tecla::pipeline::report::HidReport;

pub fn pos(row: u8, col: u8) -> KeyPos {
    KeyPos::new(row, col)
}

pub fn press(pos: KeyPos, ms: u64) -> KeyEvent {
    KeyEvent::press(pos, ms)
}

pub fn release(pos: KeyPos, ms: u64) -> KeyEvent {
    KeyEvent::release(pos, ms)
}

pub fn collect<
    const R: usize,
    const C: usize,
    const L: usize,
    const K: usize,
    Ext: Copy + PartialEq + 'static,
>(
    proc: &mut KeyboardProcessor<R, C, L, K, Ext>,
    event: KeyEvent,
    now: u64,
) -> std::vec::Vec<HidReport> {
    let mut out = std::vec::Vec::new();
    proc.process_event(event, now, &mut |r| out.push(r));
    out
}
