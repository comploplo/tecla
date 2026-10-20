//! Device-side telemetry transport.
//!
//! Defines the binary snapshot frame format and [`collect_frame`] for
//! gathering telemetry from all modalities. The host-side companion
//! (TUI, web dashboard) is a separate project.
//!
//! # Frame format
//!
//! ```text
//! ┌──────┬───────┬────────────────────────────────────────┐
//! │ 0x54 │ flags │ modality payloads (variable length)    │
//! └──────┴───────┴────────────────────────────────────────┘
//! ```
//!
//! - **Magic byte** `0x54` ('T' for telemetry)
//! - **Flags byte**: one bit per modality present in this frame
//! - Payloads appear in flag-bit order; each is a fixed-size struct

use heapless::Vec;

use super::accessibility::AccessibilitySnapshot;
use super::encoder::EncoderSnapshot;
use super::gamepad::GamepadSnapshot;
use super::midi::MidiSnapshot;
use super::pointing::PointingSnapshot;
use super::steno::StenoSnapshot;
use super::CoreSnapshot;

macro_rules! serialize_snapshot {
    ($buf:expr, $snap:expr, $($field:ident : $ty:tt),+ $(,)?) => {{
        let s = $snap;
        $(serialize_snapshot!(@push $buf, s.$field, $ty);)+
    }};
    (@push $buf:expr, $val:expr, u8)  => { let _ = $buf.push($val); };
    (@push $buf:expr, $val:expr, i8)  => { let _ = $buf.push($val as u8); };
    (@push $buf:expr, $val:expr, u16) => { push_u16($buf, $val); };
    (@push $buf:expr, $val:expr, u32) => { push_u32($buf, $val); };
    (@push $buf:expr, $val:expr, [i8; $n:expr]) => {
        let _ = $buf.extend_from_slice(&$val.map(|v| v as u8));
    };
}

macro_rules! deserialize_snapshot {
    ($buf:expr, $pos:expr, $T:ident, $($field:ident : $ty:tt),+ $(,)?) => {{
        let mut pos = $pos;
        let snap = $T {
            $($field: deserialize_snapshot!(@read $buf, pos, $ty)? ,)+
        };
        $pos = pos;
        snap
    }};
    (@read $buf:expr, $pos:ident, u8) => {{
        let v = *$buf.get($pos)?;
        $pos = $pos.checked_add(1)?;
        Some(v)
    }};
    (@read $buf:expr, $pos:ident, i8) => {{
        let v = *$buf.get($pos)? as i8;
        $pos = $pos.checked_add(1)?;
        Some(v)
    }};
    (@read $buf:expr, $pos:ident, u16) => {{
        let v = read_u16($buf, $pos)?;
        $pos = $pos.checked_add(2)?;
        Some(v)
    }};
    (@read $buf:expr, $pos:ident, u32) => {{
        let v = read_u32($buf, $pos)?;
        $pos = $pos.checked_add(4)?;
        Some(v)
    }};
    (@read $buf:expr, $pos:ident, [i8; $n:expr]) => {{
        let mut arr = [0i8; $n];
        let end = $pos.checked_add($n)?;
        let slice = $buf.get($pos..end)?;
        for (i, byte) in slice.iter().enumerate() {
            arr[i] = *byte as i8;
        }
        $pos = end;
        Some(arr)
    }};
}

const FRAME_MAGIC: u8 = 0x54;

const FLAG_CORE: u8 = 1 << 0;
const FLAG_STENO: u8 = 1 << 1;
const FLAG_MIDI: u8 = 1 << 2;
const FLAG_GAMEPAD: u8 = 1 << 3;
const FLAG_POINTING: u8 = 1 << 4;
const FLAG_ENCODER: u8 = 1 << 5;
const FLAG_ACCESSIBILITY: u8 = 1 << 6;

const MAX_FRAME_SIZE: usize = 128;

/// Bundled snapshots from all telemetry modalities.
#[derive(Clone, Copy, Default)]
pub struct TelemetryFrame {
    /// Core keystroke dynamics.
    pub core: Option<CoreSnapshot>,
    /// Steno chord accuracy.
    pub steno: Option<StenoSnapshot>,
    /// MIDI velocity and polyphony.
    pub midi: Option<MidiSnapshot>,
    /// Gamepad axis and button usage.
    pub gamepad: Option<GamepadSnapshot>,
    /// Pointing device metrics.
    pub pointing: Option<PointingSnapshot>,
    /// Encoder rotation metrics.
    pub encoder: Option<EncoderSnapshot>,
    /// Accessibility filter metrics.
    pub accessibility: Option<AccessibilitySnapshot>,
}

impl TelemetryFrame {
    /// Serialize the frame into a compact binary representation.
    pub fn serialize(&self, buf: &mut Vec<u8, MAX_FRAME_SIZE>) {
        buf.clear();
        let _ = buf.push(FRAME_MAGIC);

        let mut flags = 0u8;
        if self.core.is_some() {
            flags |= FLAG_CORE;
        }
        if self.steno.is_some() {
            flags |= FLAG_STENO;
        }
        if self.midi.is_some() {
            flags |= FLAG_MIDI;
        }
        if self.gamepad.is_some() {
            flags |= FLAG_GAMEPAD;
        }
        if self.pointing.is_some() {
            flags |= FLAG_POINTING;
        }
        if self.encoder.is_some() {
            flags |= FLAG_ENCODER;
        }
        if self.accessibility.is_some() {
            flags |= FLAG_ACCESSIBILITY;
        }
        let _ = buf.push(flags);

        if let Some(ref s) = self.core {
            serialize_snapshot!(buf, s,
                wpm: u16, rollover_ratio: u8, total_presses: u32,
                delta_presses: u16, avg_hold_ms: u16, avg_pp_ms: u16, error_rate: u8);
        }
        if let Some(ref s) = self.steno {
            serialize_snapshot!(buf, s,
                spm: u16, hit_rate: u8, retry_rate: u8, completion_rate: u8,
                undos: u32, delta_strokes: u16);
        }
        if let Some(ref s) = self.midi {
            serialize_snapshot!(buf, s,
                avg_velocity: u8, min_velocity: u8, max_velocity: u8,
                max_polyphony: u8, cc_count: u32, delta_notes: u16);
        }
        if let Some(ref s) = self.gamepad {
            serialize_snapshot!(buf, s,
                max_simultaneous: u8, axis_min: [i8; 6], axis_max: [i8; 6],
                total_presses: u32, delta_presses: u16);
        }
        if let Some(ref s) = self.pointing {
            serialize_snapshot!(buf, s,
                avg_velocity: u16, click_while_moving_rate: u8,
                total_clicks: u32, scroll_count: u32, delta_clicks: u16);
        }
        if let Some(ref s) = self.encoder {
            serialize_snapshot!(buf, s,
                total_cw: u32, total_ccw: u32, total_reversals: u16, delta_ticks: u16);
        }
        if let Some(ref s) = self.accessibility {
            serialize_snapshot!(buf, s,
                slow_reject_rate: u8, bounce_reject_rate: u8,
                sticky_uses: u32, delta_sticky_uses: u16, hold_time_trend: i8);
        }
    }

    /// Deserialize a frame from a binary buffer. Returns `None` on invalid data.
    pub fn deserialize(buf: &[u8]) -> Option<Self> {
        if buf.len() < 2 || buf[0] != FRAME_MAGIC {
            return None;
        }
        let flags = buf[1];
        let mut p = 2;
        let mut frame = TelemetryFrame::default();

        if flags & FLAG_CORE != 0 {
            frame.core = Some(deserialize_snapshot!(buf, p, CoreSnapshot,
                wpm: u16, rollover_ratio: u8, total_presses: u32,
                delta_presses: u16, avg_hold_ms: u16, avg_pp_ms: u16, error_rate: u8));
        }
        if flags & FLAG_STENO != 0 {
            frame.steno = Some(deserialize_snapshot!(buf, p, StenoSnapshot,
                spm: u16, hit_rate: u8, retry_rate: u8, completion_rate: u8,
                undos: u32, delta_strokes: u16));
        }
        if flags & FLAG_MIDI != 0 {
            frame.midi = Some(deserialize_snapshot!(buf, p, MidiSnapshot,
                avg_velocity: u8, min_velocity: u8, max_velocity: u8,
                max_polyphony: u8, cc_count: u32, delta_notes: u16));
        }
        if flags & FLAG_GAMEPAD != 0 {
            frame.gamepad = Some(deserialize_snapshot!(buf, p, GamepadSnapshot,
                max_simultaneous: u8, axis_min: [i8; 6], axis_max: [i8; 6],
                total_presses: u32, delta_presses: u16));
        }
        if flags & FLAG_POINTING != 0 {
            frame.pointing = Some(deserialize_snapshot!(buf, p, PointingSnapshot,
                avg_velocity: u16, click_while_moving_rate: u8,
                total_clicks: u32, scroll_count: u32, delta_clicks: u16));
        }
        if flags & FLAG_ENCODER != 0 {
            frame.encoder = Some(deserialize_snapshot!(buf, p, EncoderSnapshot,
                total_cw: u32, total_ccw: u32, total_reversals: u16, delta_ticks: u16));
        }
        if flags & FLAG_ACCESSIBILITY != 0 {
            frame.accessibility = Some(deserialize_snapshot!(buf, p, AccessibilitySnapshot,
                slow_reject_rate: u8, bounce_reject_rate: u8,
                sticky_uses: u32, delta_sticky_uses: u16, hold_time_trend: i8));
        }

        let _ = p;
        Some(frame)
    }
}

/// Collect a [`TelemetryFrame`] from the processor telemetry and reset deltas.
/// `encoder` is supplied by the caller because rotary encoders are scanned in
/// their own task, which owns the [`EncoderTelemetry`](super::encoder::EncoderTelemetry)
/// collector; pass `None` on boards without encoders.
pub fn collect_frame<const KEYS: usize>(
    t: &mut crate::keyboard::ProcessorTelemetry<KEYS>,
    encoder: Option<EncoderSnapshot>,
) -> TelemetryFrame {
    use super::TelemetryExt;
    TelemetryFrame {
        core: Some(t.core.collect()),
        steno: Some(t.steno.collect()),
        midi: Some(t.midi.collect()),
        gamepad: Some(t.gamepad.collect()),
        pointing: Some(t.pointing.collect()),
        encoder,
        accessibility: Some(t.accessibility.collect()),
    }
}

/// Async writer for sending serialized telemetry frames off-device.
///
/// Board crates implement this for their transport (CDC ACM, BLE, etc.).
#[cfg(feature = "embassy")]
#[expect(
    async_fn_in_trait,
    reason = "single-threaded Embassy executor; Send bounds not required"
)]
pub trait AsyncTelemetryWriter {
    /// Write a serialized telemetry frame. The buffer contains a complete
    /// frame as produced by [`TelemetryFrame::serialize`].
    async fn write(&mut self, buf: &[u8]) -> Result<(), ()>;
}

fn push_u16(buf: &mut Vec<u8, MAX_FRAME_SIZE>, v: u16) {
    let _ = buf.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(buf: &mut Vec<u8, MAX_FRAME_SIZE>, v: u32) {
    let _ = buf.extend_from_slice(&v.to_le_bytes());
}

fn read_u16(buf: &[u8], pos: usize) -> Option<u16> {
    let end = pos.checked_add(2)?;
    let bytes: [u8; 2] = buf.get(pos..end)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

fn read_u32(buf: &[u8], pos: usize) -> Option<u32> {
    let end = pos.checked_add(4)?;
    let bytes: [u8; 4] = buf.get(pos..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn empty_frame_round_trips() {
        let frame = TelemetryFrame::default();
        let mut buf = Vec::new();
        frame.serialize(&mut buf);
        assert_eq!(buf[0], FRAME_MAGIC);
        assert_eq!(buf[1], 0);
        assert_eq!(buf.len(), 2);
        let decoded = TelemetryFrame::deserialize(&buf).unwrap();
        assert!(decoded.core.is_none());
    }

    #[test]
    fn core_only_round_trips() {
        let frame = TelemetryFrame {
            core: Some(CoreSnapshot {
                wpm: 60,
                rollover_ratio: 25,
                total_presses: 1000,
                delta_presses: 50,
                avg_hold_ms: 80,
                avg_pp_ms: 120,
                error_rate: 3,
            }),
            ..Default::default()
        };
        let mut buf = Vec::new();
        frame.serialize(&mut buf);
        let decoded = TelemetryFrame::deserialize(&buf).unwrap();
        let c = decoded.core.unwrap();
        assert_eq!(c.wpm, 60);
        assert_eq!(c.total_presses, 1000);
        assert_eq!(c.error_rate, 3);
        assert!(decoded.steno.is_none());
    }

    #[test]
    fn pointing_round_trips() {
        let frame = TelemetryFrame {
            pointing: Some(PointingSnapshot {
                avg_velocity: 42,
                click_while_moving_rate: 80,
                total_clicks: 555,
                scroll_count: 33,
                delta_clicks: 7,
            }),
            ..Default::default()
        };
        let mut buf = Vec::new();
        frame.serialize(&mut buf);
        let decoded = TelemetryFrame::deserialize(&buf).unwrap();
        let p = decoded.pointing.unwrap();
        assert_eq!(p.avg_velocity, 42);
        assert_eq!(p.total_clicks, 555);
        assert_eq!(p.delta_clicks, 7);
    }

    #[test]
    fn encoder_round_trips() {
        let frame = TelemetryFrame {
            encoder: Some(EncoderSnapshot {
                total_cw: 150,
                total_ccw: 120,
                total_reversals: 8,
                delta_ticks: 12,
            }),
            ..Default::default()
        };
        let mut buf = Vec::new();
        frame.serialize(&mut buf);
        let decoded = TelemetryFrame::deserialize(&buf).unwrap();
        let e = decoded.encoder.unwrap();
        assert_eq!(e.total_cw, 150);
        assert_eq!(e.total_ccw, 120);
        assert_eq!(e.total_reversals, 8);
        assert_eq!(e.delta_ticks, 12);
    }

    #[test]
    fn all_modalities_round_trip() {
        let frame = TelemetryFrame {
            core: Some(CoreSnapshot {
                wpm: 45,
                rollover_ratio: 10,
                total_presses: 500,
                delta_presses: 20,
                avg_hold_ms: 90,
                avg_pp_ms: 110,
                error_rate: 2,
            }),
            steno: Some(StenoSnapshot {
                spm: 30,
                hit_rate: 200,
                retry_rate: 5,
                completion_rate: 150,
                undos: 3,
                delta_strokes: 10,
            }),
            midi: Some(MidiSnapshot {
                avg_velocity: 80,
                min_velocity: 20,
                max_velocity: 127,
                max_polyphony: 4,
                cc_count: 50,
                delta_notes: 15,
            }),
            gamepad: Some(GamepadSnapshot {
                max_simultaneous: 3,
                axis_min: [-100, -50, 0, 0, 0, 0],
                axis_max: [100, 50, 0, 0, 0, 0],
                total_presses: 300,
                delta_presses: 12,
            }),
            pointing: Some(PointingSnapshot {
                avg_velocity: 15,
                click_while_moving_rate: 80,
                total_clicks: 100,
                scroll_count: 25,
                delta_clicks: 5,
            }),
            encoder: Some(EncoderSnapshot {
                total_cw: 150,
                total_ccw: 120,
                total_reversals: 8,
                delta_ticks: 12,
            }),
            accessibility: Some(AccessibilitySnapshot {
                slow_reject_rate: 25,
                bounce_reject_rate: 10,
                sticky_uses: 42,
                delta_sticky_uses: 7,
                hold_time_trend: -3,
            }),
        };
        let mut buf = Vec::new();
        frame.serialize(&mut buf);
        let decoded = TelemetryFrame::deserialize(&buf).unwrap();

        assert_eq!(decoded.core.unwrap().wpm, 45);
        assert_eq!(decoded.steno.unwrap().spm, 30);
        assert_eq!(decoded.midi.unwrap().max_polyphony, 4);
        assert_eq!(decoded.gamepad.unwrap().axis_min[0], -100);
        assert_eq!(decoded.pointing.unwrap().total_clicks, 100);
        assert_eq!(decoded.encoder.unwrap().total_cw, 150);
        let acc = decoded.accessibility.unwrap();
        assert_eq!(acc.hold_time_trend, -3);
        assert_eq!(acc.delta_sticky_uses, 7);
    }

    #[test]
    fn invalid_magic_returns_none() {
        assert!(TelemetryFrame::deserialize(&[0xFF, 0x00]).is_none());
    }

    #[test]
    fn truncated_payload_returns_none() {
        assert!(TelemetryFrame::deserialize(&[FRAME_MAGIC, FLAG_CORE, 0x00]).is_none());
    }

    #[test]
    fn malformed_payload_returns_none() {
        let buf = [FRAME_MAGIC, FLAG_CORE, 0x01, 0x02, 0x03];
        assert!(TelemetryFrame::deserialize(&buf).is_none());
    }

    #[test]
    fn collect_frame_resets_deltas() {
        let mut t = crate::keyboard::ProcessorTelemetry::<6>::new();
        t.core
            .record_press(crate::pipeline::event::KeyPos::new(0, 0), 100, 3);
        t.pointing.record_click(false);
        use crate::telemetry::TelemetryExt;
        let mut enc = crate::telemetry::encoder::EncoderTelemetry::<2>::new();
        enc.record_cw(0);
        let frame = collect_frame(&mut t, Some(enc.collect()));
        assert_eq!(frame.core.unwrap().delta_presses, 1);
        assert_eq!(frame.pointing.unwrap().delta_clicks, 1);
        assert_eq!(frame.encoder.unwrap().total_cw, 1);
        let frame2 = collect_frame(&mut t, None);
        assert!(frame2.encoder.is_none());
        assert_eq!(frame2.core.unwrap().delta_presses, 0);
        assert_eq!(frame2.pointing.unwrap().delta_clicks, 0);
    }
}
