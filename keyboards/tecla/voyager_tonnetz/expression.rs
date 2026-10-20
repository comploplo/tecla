//! Latched MIDI expression from the Navigator's unaccelerated sensor motion.

use super::instrument::CHANNEL;
use tecla::pipeline::report::MidiEvent;

const COUNTS_PER_STEP: i32 = 32;
const HYSTERESIS: i32 = 4;
const MAX_POSITION: i32 = 127 * COUNTS_PER_STEP;
const SEND_INTERVAL_MS: u64 = 16;
const CONTROLLERS: [u8; 2] = [74, 11];

/// Rejects sensor intervals that began while motion was muted.
pub struct MotionGate {
    armed_since_ms: Option<u64>,
}

impl MotionGate {
    /// Starts muted.
    pub const fn new() -> Self {
        Self {
            armed_since_ms: None,
        }
    }
    /// Starts a fresh boundary on each transition from muted to enabled.
    pub fn update(&mut self, enabled: bool, now_ms: u64) {
        if !enabled {
            self.armed_since_ms = None;
        } else if self.armed_since_ms.is_none() {
            self.armed_since_ms = Some(now_ms);
        }
    }
    /// Accepts only samples whose entire accumulation interval was enabled.
    pub fn accepts(&self, since_ms: u64) -> bool {
        self.armed_since_ms.is_some_and(|armed| since_ms >= armed)
    }
}

/// A delivery snapshot. Acknowledge it only after its entire USB write succeeds.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    values: [u8; 2],
    changed: [bool; 2],
    generation: u32,
}

impl Frame {
    /// Changed brightness/expression controls, sharing the notes' MIDI channel 1.
    pub fn events(self) -> impl Iterator<Item = MidiEvent> {
        (0..2).filter_map(move |axis| {
            self.changed[axis].then_some(MidiEvent::Cc {
                channel: CHANNEL,
                controller: CONTROLLERS[axis],
                value: self.values[axis],
            })
        })
    }
}

/// Two bounded, latched controls, with coalesced delivery and fractional motion.
pub struct Expression {
    positions: [i32; 2],
    values: [u8; 2],
    delivered: [Option<u8>; 2],
    last_attempt_ms: Option<u64>,
    generation: u32,
}

impl Expression {
    /// Starts with neutral brightness and full expression, ready to synchronise.
    pub const fn new() -> Self {
        Self {
            positions: [64 * COUNTS_PER_STEP, MAX_POSITION],
            values: [64, 127],
            delivered: [None; 2],
            last_attempt_ms: None,
            generation: 0,
        }
    }

    /// Integrates raw counts: right brightens; up increases expression.
    ///
    /// Uses 32 raw counts per MIDI step, preserving fractional movement.
    /// Hysteresis prevents tiny reversals at a step boundary from chattering.
    pub fn motion(&mut self, dx: i16, dy: i16) {
        for (axis, delta) in [i32::from(dx), -i32::from(dy)].into_iter().enumerate() {
            self.positions[axis] = (self.positions[axis] + delta).clamp(0, MAX_POSITION);
            let departure = self.positions[axis] - i32::from(self.values[axis]) * COUNTS_PER_STEP;
            if departure.abs() >= COUNTS_PER_STEP / 2 + HYSTERESIS {
                self.values[axis] =
                    ((self.positions[axis] + COUNTS_PER_STEP / 2) / COUNTS_PER_STEP) as u8;
            }
        }
    }

    /// Returns at most two changed CCs, at most once per 16 ms attempted send.
    ///
    /// Preparing a frame starts the rate window but does not acknowledge values.
    /// Motion received while a write is pending is coalesced into the next frame.
    pub fn pending(&mut self, now_ms: u64) -> Option<Frame> {
        if self
            .last_attempt_ms
            .is_some_and(|at| now_ms.saturating_sub(at) < SEND_INTERVAL_MS)
        {
            return None;
        }
        let changed = core::array::from_fn(|axis| self.delivered[axis] != Some(self.values[axis]));
        if !changed.into_iter().any(|value| value) {
            return None;
        }
        self.last_attempt_ms = Some(now_ms);
        Some(Frame {
            values: self.values,
            changed,
            generation: self.generation,
        })
    }

    /// Records a successful complete write, provided recovery did not invalidate it.
    pub fn commit(&mut self, frame: Frame) -> bool {
        if frame.generation != self.generation {
            return false;
        }
        for (axis, changed) in frame.changed.into_iter().enumerate() {
            if changed {
                self.delivered[axis] = Some(frame.values[axis]);
            }
        }
        true
    }

    /// Whether this USB session has received both controls at least once.
    ///
    /// Ordinary later motion does not unset this initial synchronisation gate.
    pub const fn is_synced(&self) -> bool {
        self.delivered[0].is_some() && self.delivered[1].is_some()
    }

    /// Keeps the latched controls, but resends both after USB recovery.
    pub fn invalidate(&mut self) {
        self.delivered = [None; 2];
        self.last_attempt_ms = None;
        self.generation = self.generation.wrapping_add(1);
    }
}
