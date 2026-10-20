//! Via protocol — dynamic keymap remapping over raw USB HID.
//!
//! Decodes the 32-byte Via command packets for runtime keymap editing.
//! Feature-gated behind `via` (which implies `storage`).
//!
//! Transport is `transport::usb::via_usb_task` (feature `usb`) plus
//! [`service_via`](crate::task::service_via); the descriptor is
//! [`VIA_REPORT_DESCRIPTOR`](crate::pipeline::report::VIA_REPORT_DESCRIPTOR).
//!
//! # Scope
//!
//! A **subset** of Via: protocol version, keyboard values (uptime, firmware
//! version, layout options), layer count, per-key get/set, and keymap reset.
//! Not implemented: dynamic macros, the bulk keymap buffer commands, encoders,
//! custom/lighting values, or bootloader jump. Vial is a different protocol
//! and is not implemented at all.
//!
//! Command IDs are pinned to the vendored QMK header by
//! `tests/via_protocol_ids.rs` — they are Via's to define, not ours.

use crate::action::{Action, KeyCode, LayerOp, ModifierKey};
use crate::keyboard::KeyboardProcessor;
#[cfg(feature = "storage")]
use crate::storage::eeprom::EepromShim;

const VIA_PROTOCOL_VERSION: u16 = 0x000C;

// Command IDs from QMK's `quantum/via.h` (`enum via_command_id`). These are
// fixed by Via's host application — the numbering below is not ours to choose,
// and an earlier revision of this file had most of it wrong: 0x04 was treated
// as "get layer count" when Via defines it as "get keycode", so the host and
// the firmware disagreed about every keymap command.
const CMD_GET_PROTOCOL_VERSION: u8 = 0x01;
const CMD_GET_KEYBOARD_VALUE: u8 = 0x02;
const CMD_DYNAMIC_KEYMAP_GET_KEYCODE: u8 = 0x04;
const CMD_DYNAMIC_KEYMAP_SET_KEYCODE: u8 = 0x05;
const CMD_DYNAMIC_KEYMAP_RESET: u8 = 0x06;
const CMD_DYNAMIC_KEYMAP_GET_LAYER_COUNT: u8 = 0x11;
/// Via replaces the command id with this when it cannot handle a request.
const CMD_UNHANDLED: u8 = 0xFF;

// Sub-command IDs for CMD_GET_KEYBOARD_VALUE (`enum via_keyboard_value_id`).
const VALUE_UPTIME: u8 = 0x01;
const VALUE_LAYOUT_OPTIONS: u8 = 0x02;
const VALUE_FIRMWARE_VERSION: u8 = 0x04;

/// Reported to Via as the firmware revision.
const VIA_FIRMWARE_VERSION: u32 = 0x0000_0001;

// Not part of Via's enum: tecla-specific raw EEPROM access, kept in the
// range Via leaves unused.
const CMD_EEPROM_READ: u8 = 0x23;
const CMD_EEPROM_WRITE: u8 = 0x24;

/// Reads one chunk of a serialized [`TelemetryFrame`].
///
/// Also tecla-specific. The frame is larger than a 32-byte report, so the host
/// asks for chunk `n` until it has `total_len` bytes:
///
/// ```text
/// request : [0x25, chunk, ...]
/// response: [0x25, chunk, total_len, ...up to TELEMETRY_CHUNK_BYTES of frame]
/// ```
///
/// Reads are non-destructive so a multi-chunk read stays consistent; the host
/// clears deltas explicitly with [`CMD_TECLA_TELEMETRY_CLEAR`] once it has the
/// whole frame.
const CMD_TECLA_TELEMETRY: u8 = 0x25;

/// Clears every collector's delta counters, keeping cumulative totals.
const CMD_TECLA_TELEMETRY_CLEAR: u8 = 0x26;

/// Frame bytes per telemetry report: 32 minus the command, chunk, and length
/// bytes that precede the payload.
pub const TELEMETRY_CHUNK_BYTES: usize = 29;

/// Via protocol handler.
///
/// Processes 32-byte raw HID command packets and mutates the keymap
/// via [`KeyboardProcessor::set_action`].
pub struct ViaProtocol<const COLS: usize> {
    uptime_ms: u64,
    default_keymap: &'static [&'static [u16]],
}

impl<const COLS: usize> ViaProtocol<COLS> {
    /// Creates a new Via protocol handler.
    ///
    /// `default_keymap` is a slice of layers, each a flat slice of Via keycodes
    /// in row-major order. Used by the reset command to restore defaults.
    #[must_use]
    pub fn new(default_keymap: &'static [&'static [u16]]) -> Self {
        Self {
            uptime_ms: 0,
            default_keymap,
        }
    }

    /// Processes a 32-byte Via command packet.
    ///
    /// Writes the response into `response`. Mutates `processor` for set/reset
    /// commands.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "wire protocol uses fixed-width fields"
    )]
    #[expect(
        clippy::as_conversions,
        reason = "wire protocol uses fixed-width fields"
    )]
    pub fn process_command<
        const ROWS: usize,
        const LAYERS: usize,
        const KEYS: usize,
        Ext: Copy + PartialEq + 'static,
    >(
        &self,
        request: &[u8; 32],
        response: &mut [u8; 32],
        processor: &mut KeyboardProcessor<ROWS, COLS, LAYERS, KEYS, Ext>,
    ) {
        *response = [0u8; 32];
        response[0] = request[0];

        // Via reuses the request buffer as the response, so every byte the
        // host sent is echoed back unless a handler overwrites it. Mirroring
        // that matters: the host reads layer/row/col out of the *reply* to
        // match it to the request it sent.
        *response = *request;

        match request[0] {
            CMD_GET_PROTOCOL_VERSION => {
                response[1] = (VIA_PROTOCOL_VERSION >> 8) as u8;
                response[2] = VIA_PROTOCOL_VERSION as u8;
            }
            CMD_GET_KEYBOARD_VALUE => match request[1] {
                VALUE_UPTIME => {
                    let ms = self.uptime_ms as u32;
                    response[2] = (ms >> 24) as u8;
                    response[3] = (ms >> 16) as u8;
                    response[4] = (ms >> 8) as u8;
                    response[5] = ms as u8;
                }
                VALUE_FIRMWARE_VERSION => {
                    let v = VIA_FIRMWARE_VERSION;
                    response[2] = (v >> 24) as u8;
                    response[3] = (v >> 16) as u8;
                    response[4] = (v >> 8) as u8;
                    response[5] = v as u8;
                }
                VALUE_LAYOUT_OPTIONS => {
                    // No layout options are defined; report all-zero rather
                    // than "unhandled", which Via treats as an error dialog.
                    response[2] = 0;
                    response[3] = 0;
                    response[4] = 0;
                    response[5] = 0;
                }
                _ => response[0] = CMD_UNHANDLED,
            },
            CMD_DYNAMIC_KEYMAP_GET_LAYER_COUNT => {
                response[1] = LAYERS as u8;
            }
            CMD_DYNAMIC_KEYMAP_GET_KEYCODE => {
                let layer = usize::from(request[1]);
                let row = usize::from(request[2]);
                let col = usize::from(request[3]);
                let via_kc = row
                    .checked_mul(COLS)
                    .and_then(|base| base.checked_add(col))
                    .and_then(|key| processor.get_action(layer, key))
                    .map_or(0, |a| action_to_via_keycode(&a));
                response[4] = (via_kc >> 8) as u8;
                response[5] = via_kc as u8;
            }
            CMD_DYNAMIC_KEYMAP_SET_KEYCODE => {
                let layer = usize::from(request[1]);
                let row = usize::from(request[2]);
                let col = usize::from(request[3]);
                let via_kc = (u16::from(request[4]) << 8) | u16::from(request[5]);
                if let Some(key) = row.checked_mul(COLS).and_then(|b| b.checked_add(col)) {
                    processor.set_action(layer, key, via_keycode_to_action(via_kc));
                }
            }
            CMD_DYNAMIC_KEYMAP_RESET => {
                for (layer_idx, &layer_keys) in self.default_keymap.iter().enumerate() {
                    for (key_idx, &via_kc) in layer_keys.iter().enumerate() {
                        let action = via_keycode_to_action(via_kc);
                        processor.set_action(layer_idx, key_idx, action);
                    }
                }
            }
            CMD_TECLA_TELEMETRY => {
                let Some(telemetry) = processor.telemetry() else {
                    // Telemetry not enabled on this build — say so rather than
                    // returning a zero-length frame the host would read as
                    // "collected nothing".
                    response[0] = CMD_UNHANDLED;
                    return;
                };
                let mut buf = heapless::Vec::new();
                telemetry.frame().serialize(&mut buf);

                let chunk = usize::from(request[1]);
                response[2] = buf.len() as u8;
                let start = chunk.saturating_mul(TELEMETRY_CHUNK_BYTES);
                // A chunk past the end is not an error: it is how a host that
                // guessed the chunk count discovers it is done.
                for (slot, &byte) in response[3..]
                    .iter_mut()
                    .zip(buf.iter().skip(start).take(TELEMETRY_CHUNK_BYTES))
                {
                    *slot = byte;
                }
            }
            CMD_TECLA_TELEMETRY_CLEAR => {
                if let Some(telemetry) = processor.telemetry_mut() {
                    telemetry.clear_deltas();
                } else {
                    response[0] = CMD_UNHANDLED;
                }
            }
            CMD_EEPROM_READ | CMD_EEPROM_WRITE => {
                // Handled by process_eeprom_command, which owns the storage.
            }
            _ => response[0] = CMD_UNHANDLED,
        }
    }

    /// Processes a Via EEPROM read or write command using the given shim.
    ///
    /// Returns `true` if the command was handled (0x23 or 0x24), `false` otherwise.
    /// Call this before or after [`process_command`](Self::process_command) for
    /// packets that may be EEPROM operations.
    #[cfg(feature = "storage")]
    pub fn process_eeprom_command<const SIZE: usize>(
        &self,
        request: &[u8; 32],
        response: &mut [u8; 32],
        eeprom: &mut EepromShim<SIZE>,
    ) -> bool {
        response[0] = request[0];
        match request[0] {
            CMD_EEPROM_READ => {
                let offset = (u16::from(request[1]) << 8) | u16::from(request[2]);
                let len = usize::from(request[3]).min(28);
                if let Some(dest) = response.get_mut(4..4 + len) {
                    if eeprom.read(offset, dest).is_err() {
                        response[0] = 0xFF;
                    }
                }
                true
            }
            CMD_EEPROM_WRITE => {
                let offset = (u16::from(request[1]) << 8) | u16::from(request[2]);
                let len = usize::from(request[3]).min(28);
                if let Some(src) = request.get(4..4 + len) {
                    if eeprom.write(offset, src).is_err() {
                        response[0] = 0xFF;
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Updates the uptime counter (call periodically from the main loop).
    pub fn set_uptime(&mut self, ms: u64) {
        self.uptime_ms = ms;
    }
}

/// Converts a Via 16-bit keycode to a tecla [`Action`].
#[must_use]
#[expect(
    clippy::as_conversions,
    reason = "extracting bytes from u16 wire keycode"
)]
pub fn via_keycode_to_action<Ext: Copy + PartialEq>(via_kc: u16) -> Action<Ext> {
    if via_kc == 0x0000 {
        return Action::NoOp;
    }

    let high = (via_kc >> 8) as u8;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "extracting low byte of u16"
    )]
    let low = via_kc as u8;

    match high {
        0x00 => match low {
            0xE0 => Action::Modifier(ModifierKey::LCTRL),
            0xE1 => Action::Modifier(ModifierKey::LSHIFT),
            0xE2 => Action::Modifier(ModifierKey::LALT),
            0xE3 => Action::Modifier(ModifierKey::LGUI),
            0xE4 => Action::Modifier(ModifierKey::RCTRL),
            0xE5 => Action::Modifier(ModifierKey::RSHIFT),
            0xE6 => Action::Modifier(ModifierKey::RALT),
            0xE7 => Action::Modifier(ModifierKey::RGUI),
            _ => KeyCode::from_usage_id(low).map_or(Action::NoOp, Action::Key),
        },
        0x40..=0x4F => {
            let layer = high & 0x0F;
            let kc = KeyCode::from_usage_id(low).unwrap_or(KeyCode::None);
            Action::LayerTap { layer, tap: kc }
        }
        0x51 => Action::Layer(LayerOp::Momentary(low)),
        0x52 => Action::Layer(LayerOp::Toggle(low)),
        0x53 => Action::Layer(LayerOp::SetDefault(low)),
        0x54 => Action::Layer(LayerOp::On(low)),
        0x55 => Action::Layer(LayerOp::Off(low)),
        _ => Action::NoOp,
    }
}

/// Converts a tecla [`Action`] to a Via 16-bit keycode.
#[must_use]
#[expect(clippy::as_conversions, reason = "converting to wire format u16")]
pub fn action_to_via_keycode<Ext: Copy + PartialEq>(action: &Action<Ext>) -> u16 {
    match action {
        Action::NoOp => 0x0000,
        Action::Key(kc) => u16::from(kc.usage_id()),
        Action::Modifier(m) => {
            let bits = m.bits();
            let idx = bits.trailing_zeros();
            #[expect(clippy::cast_possible_truncation, reason = "idx < 8 guaranteed")]
            if idx < 8 {
                u16::from(0xE0u8 + idx as u8)
            } else {
                0
            }
        }
        Action::Layer(LayerOp::Momentary(n)) => 0x5100 | u16::from(*n),
        Action::Layer(LayerOp::Toggle(n)) => 0x5200 | u16::from(*n),
        Action::Layer(LayerOp::SetDefault(n)) => 0x5300 | u16::from(*n),
        Action::Layer(LayerOp::On(n)) => 0x5400 | u16::from(*n),
        Action::Layer(LayerOp::Off(n)) => 0x5500 | u16::from(*n),
        Action::LayerTap { layer, tap } => {
            0x4000 | (u16::from(*layer) << 8) | u16::from(tap.usage_id())
        }
        Action::Transparent => 0x0001,
        _ => 0x0000,
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::unused_result_ok, reason = "test code")]
mod tests {
    use super::*;
    use crate::devices::keyboard::tapping::TapHoldConfig;
    use crate::pipeline::keymap::{Layer, LayerStack};

    pub(crate) fn make_processor() -> KeyboardProcessor<2, 3, 2, 6> {
        let mut layers: heapless::Vec<Layer<()>, 2> = heapless::Vec::new();
        layers
            .push(Layer::new(&[
                Action::Key(KeyCode::A),
                Action::Key(KeyCode::B),
                Action::Key(KeyCode::C),
                Action::Key(KeyCode::D),
                Action::Key(KeyCode::E),
                Action::Key(KeyCode::F),
            ]))
            .ok();
        layers.push(Layer::new(&[Action::Transparent; 6])).ok();
        let stack: LayerStack<2, 6> = LayerStack::new(layers);
        KeyboardProcessor::new(stack, TapHoldConfig::default())
    }

    #[test]
    fn get_protocol_version() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_GET_PROTOCOL_VERSION;
        via.process_command(&req, &mut resp, &mut proc);
        assert_eq!(resp[0], CMD_GET_PROTOCOL_VERSION);
        let version = (u16::from(resp[1]) << 8) | u16::from(resp[2]);
        assert_eq!(version, VIA_PROTOCOL_VERSION);
    }

    #[test]
    fn get_uptime() {
        let mut via: ViaProtocol<3> = ViaProtocol::new(&[]);
        via.set_uptime(0x1234_5678);
        let mut proc = make_processor();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        // Uptime is a sub-command of get_keyboard_value, with the value in
        // bytes 2..6 (QMK via.c: command_data[1..5]).
        req[0] = CMD_GET_KEYBOARD_VALUE;
        req[1] = VALUE_UPTIME;
        via.process_command(&req, &mut resp, &mut proc);
        let ms = (u64::from(resp[2]) << 24)
            | (u64::from(resp[3]) << 16)
            | (u64::from(resp[4]) << 8)
            | u64::from(resp[5]);
        assert_eq!(ms, 0x1234_5678);
    }

    #[test]
    fn get_layer_count() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_DYNAMIC_KEYMAP_GET_LAYER_COUNT;
        via.process_command(&req, &mut resp, &mut proc);
        assert_eq!(resp[1], 2);
    }

    #[test]
    fn get_keycode() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_DYNAMIC_KEYMAP_GET_KEYCODE;
        req[1] = 0; // layer
        req[2] = 0; // row
        req[3] = 0; // col
        via.process_command(&req, &mut resp, &mut proc);
        let kc = (u16::from(resp[4]) << 8) | u16::from(resp[5]);
        assert_eq!(kc, u16::from(KeyCode::A.usage_id()));
    }

    #[test]
    fn set_keycode() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_DYNAMIC_KEYMAP_SET_KEYCODE;
        req[1] = 0; // layer
        req[2] = 0; // row
        req[3] = 0; // col
        req[4] = 0;
        req[5] = KeyCode::Z.usage_id();
        via.process_command(&req, &mut resp, &mut proc);

        assert_eq!(proc.get_action(0, 0), Some(Action::Key(KeyCode::Z)));
    }

    #[test]
    fn reset_keymap() {
        static LAYER0: [u16; 6] = [0x14, 0x14, 0x14, 0x14, 0x14, 0x14]; // KC_Q
        static DEFAULT: [&[u16]; 1] = [&LAYER0];
        let via: ViaProtocol<3> = ViaProtocol::new(&DEFAULT);
        let mut proc = make_processor();

        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_DYNAMIC_KEYMAP_RESET;
        via.process_command(&req, &mut resp, &mut proc);

        assert_eq!(proc.get_action(0, 0), Some(Action::Key(KeyCode::Q)));
    }

    #[test]
    fn unknown_command_is_noop() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = 0xFF;
        via.process_command(&req, &mut resp, &mut proc);
        assert_eq!(resp[0], 0xFF);
        assert_eq!(resp[1], 0);
    }

    #[test]
    fn get_keycode_out_of_bounds() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_DYNAMIC_KEYMAP_GET_KEYCODE;
        req[1] = 99; // invalid layer
        req[2] = 0;
        req[3] = 0;
        via.process_command(&req, &mut resp, &mut proc);
        let kc = (u16::from(resp[4]) << 8) | u16::from(resp[5]);
        assert_eq!(kc, 0);
    }

    #[test]
    fn via_keycode_roundtrip_basic() {
        let cases: [Action<()>; 12] = [
            Action::NoOp,
            Action::Key(KeyCode::A),
            Action::Key(KeyCode::Z),
            Action::Key(KeyCode::N1),
            Action::Modifier(ModifierKey::LSHIFT),
            Action::Modifier(ModifierKey::RALT),
            Action::Layer(LayerOp::Momentary(1)),
            Action::Layer(LayerOp::Toggle(2)),
            Action::Layer(LayerOp::SetDefault(0)),
            Action::Layer(LayerOp::On(3)),
            Action::Layer(LayerOp::Off(1)),
            Action::LayerTap {
                layer: 1,
                tap: KeyCode::Space,
            },
        ];
        for action in &cases {
            let via_kc = action_to_via_keycode(action);
            let roundtripped: Action<()> = via_keycode_to_action(via_kc);
            assert_eq!(
                *action, roundtripped,
                "roundtrip failed for {action:?} (via_kc=0x{via_kc:04X})"
            );
        }
    }

    #[test]
    fn via_keycode_transparent() {
        let action: Action<()> = Action::Transparent;
        let kc = action_to_via_keycode(&action);
        assert_eq!(kc, 0x0001);
    }

    #[test]
    fn via_keycode_unknown_returns_noop() {
        let action: Action<()> = via_keycode_to_action(0xFFFF);
        assert_eq!(action, Action::NoOp);
    }

    #[test]
    fn via_keycode_noop_zero() {
        let action: Action<()> = via_keycode_to_action(0x0000);
        assert_eq!(action, Action::NoOp);
    }

    #[test]
    fn action_to_via_ext_returns_zero() {
        let action: Action<()> = Action::CapsWord;
        assert_eq!(action_to_via_keycode(&action), 0);
    }

    #[test]
    fn via_modifier_keycodes() {
        let kc: Action<()> = via_keycode_to_action(0x00E0);
        assert_eq!(kc, Action::Modifier(ModifierKey::LCTRL));
        let kc: Action<()> = via_keycode_to_action(0x00E7);
        assert_eq!(kc, Action::Modifier(ModifierKey::RGUI));
    }

    #[test]
    fn action_to_via_modifier_none_returns_zero() {
        let action: Action<()> = Action::Modifier(ModifierKey::NONE);
        assert_eq!(action_to_via_keycode(&action), 0);
    }

    #[test]
    fn eeprom_read_command() {
        use crate::storage::eeprom::EepromShim;

        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut eeprom = EepromShim::<64>::new();
        eeprom.write(0, &[0xAA, 0xBB, 0xCC]).unwrap();

        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_EEPROM_READ;
        req[1] = 0;
        req[2] = 0;
        req[3] = 3;
        assert!(via.process_eeprom_command(&req, &mut resp, &mut eeprom));
        assert_eq!(&resp[4..7], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn eeprom_write_command() {
        use crate::storage::eeprom::EepromShim;

        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut eeprom = EepromShim::<64>::new();

        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_EEPROM_WRITE;
        req[1] = 0;
        req[2] = 2;
        req[3] = 2;
        req[4] = 0xDE;
        req[5] = 0xAD;
        assert!(via.process_eeprom_command(&req, &mut resp, &mut eeprom));

        assert_eq!(eeprom.read_byte(2).unwrap(), 0xDE);
        assert_eq!(eeprom.read_byte(3).unwrap(), 0xAD);
    }

    #[test]
    fn eeprom_unhandled_command() {
        use crate::storage::eeprom::EepromShim;

        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut eeprom = EepromShim::<64>::new();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = 0xFF;
        assert!(!via.process_eeprom_command(&req, &mut resp, &mut eeprom));
    }
}

/// End-to-end tests for the Via transport loop.
///
/// The decoder had 18 unit tests but no transport: nothing carried a packet
/// from a host to it and back. These drive the same request/response exchange
/// the raw-HID endpoint performs, so the wiring is covered on the host rather
/// than only on hardware.
#[cfg(test)]
mod transport_tests {
    use super::tests::*;
    use super::*;
    use crate::pipeline::event::{KeyEvent, KeyPos};
    use crate::pipeline::report::VIA_REPORT_SIZE;

    /// One host exchange: 32 bytes in, 32 bytes out.
    fn exchange<const ROWS: usize, const LAYERS: usize, const KEYS: usize>(
        via: &ViaProtocol<3>,
        processor: &mut KeyboardProcessor<ROWS, 3, LAYERS, KEYS>,
        request: &[u8],
    ) -> [u8; VIA_REPORT_SIZE] {
        let mut req = [0u8; VIA_REPORT_SIZE];
        req[..request.len()].copy_from_slice(request);
        let mut resp = [0u8; VIA_REPORT_SIZE];
        via.process_command(&req, &mut resp, processor);
        resp
    }

    #[test]
    fn telemetry_reassembles_across_chunks_and_round_trips() {
        use crate::telemetry::transport::TelemetryFrame;

        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor().with_telemetry();

        // Give the collectors something to report, so a frame of zeros cannot
        // pass this test by accident.
        let mut emit = |_r| {};
        proc.process_event(KeyEvent::press(KeyPos::new(0, 0), 0), 0, &mut emit);
        proc.process_event(KeyEvent::release(KeyPos::new(0, 0), 40), 40, &mut emit);

        let first = exchange(&via, &mut proc, &[CMD_TECLA_TELEMETRY, 0]);
        let total = usize::from(first[2]);
        assert!(total > 0, "frame should not be empty");

        let mut wire: heapless::Vec<u8, 128> = heapless::Vec::new();
        let chunks = total.div_ceil(TELEMETRY_CHUNK_BYTES);
        for chunk in 0..chunks {
            let resp = exchange(&via, &mut proc, &[CMD_TECLA_TELEMETRY, chunk as u8]);
            assert_eq!(usize::from(resp[2]), total, "length must not drift");
            let remaining = total - wire.len();
            let take = remaining.min(TELEMETRY_CHUNK_BYTES);
            wire.extend_from_slice(&resp[3..3 + take]).unwrap();
        }
        assert_eq!(wire.len(), total);

        let decoded = TelemetryFrame::deserialize(&wire).expect("reassembled frame must decode");
        let core = decoded.core.expect("core snapshot present");
        assert_eq!(core.total_presses, 1);
        assert_eq!(core.delta_presses, 1);
    }

    #[test]
    fn telemetry_reads_do_not_clear_deltas_but_the_clear_command_does() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor().with_telemetry();
        let mut emit = |_r| {};
        proc.process_event(KeyEvent::press(KeyPos::new(0, 0), 0), 0, &mut emit);

        let delta = |p: &KeyboardProcessor<2, 3, 2, 6>| {
            p.telemetry().unwrap().core.snapshot().delta_presses
        };

        // Reading twice must not consume the deltas — a multi-chunk read would
        // otherwise return a different frame partway through.
        let _ = exchange(&via, &mut proc, &[CMD_TECLA_TELEMETRY, 0]);
        let _ = exchange(&via, &mut proc, &[CMD_TECLA_TELEMETRY, 0]);
        assert_eq!(delta(&proc), 1, "reads must be non-destructive");

        let _ = exchange(&via, &mut proc, &[CMD_TECLA_TELEMETRY_CLEAR]);
        assert_eq!(delta(&proc), 0, "clear command must reset deltas");
        assert_eq!(
            proc.telemetry().unwrap().core.snapshot().total_presses,
            1,
            "cumulative totals survive a clear"
        );
    }

    #[test]
    fn telemetry_commands_report_unhandled_when_telemetry_is_off() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor(); // no .with_telemetry()
        let resp = exchange(&via, &mut proc, &[CMD_TECLA_TELEMETRY, 0]);
        assert_eq!(
            resp[0], CMD_UNHANDLED,
            "must not look like a valid empty frame"
        );
        let resp = exchange(&via, &mut proc, &[CMD_TECLA_TELEMETRY_CLEAR]);
        assert_eq!(resp[0], CMD_UNHANDLED);
    }

    #[test]
    fn a_set_keycode_command_actually_remaps_the_key() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();

        // Base layer, row 0, col 1 currently resolves to B.
        let before = exchange(&via, &mut proc, &[CMD_DYNAMIC_KEYMAP_GET_KEYCODE, 0, 0, 1]);
        let before_kc = (u16::from(before[4]) << 8) | u16::from(before[5]);
        assert_eq!(
            via_keycode_to_action::<()>(before_kc),
            Action::Key(KeyCode::B)
        );

        // Remap it to Z (Via keycode == HID usage id for basic keys).
        let z = u16::from(KeyCode::Z.usage_id());
        exchange(
            &via,
            &mut proc,
            &[
                CMD_DYNAMIC_KEYMAP_SET_KEYCODE,
                0,
                0,
                1,
                (z >> 8) as u8,
                z as u8,
            ],
        );

        // Read it back over the wire...
        let after = exchange(&via, &mut proc, &[CMD_DYNAMIC_KEYMAP_GET_KEYCODE, 0, 0, 1]);
        let after_kc = (u16::from(after[4]) << 8) | u16::from(after[5]);
        assert_eq!(
            via_keycode_to_action::<()>(after_kc),
            Action::Key(KeyCode::Z),
            "the readback should reflect the new mapping"
        );

        // ...and confirm the *processor* really changed, not just the readback.
        let mut reports = std::vec::Vec::new();
        proc.process_event(
            crate::pipeline::event::KeyEvent::press(crate::pipeline::event::KeyPos::new(0, 1), 0),
            0,
            &mut |r| reports.push(r),
        );
        let typed = reports.iter().any(|r| match r {
            crate::pipeline::report::HidReport::Keyboard(kr) => {
                kr.keycodes.contains(&KeyCode::Z.usage_id())
            }
            _ => false,
        });
        assert!(typed, "pressing the remapped key should now emit Z");
    }

    #[test]
    fn every_response_echoes_the_command_id_in_byte_zero() {
        // Via's host matches replies to requests by this byte; getting it
        // wrong makes the app hang waiting for a response it already has.
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        for (cmd, sub) in [
            (CMD_GET_PROTOCOL_VERSION, 0),
            (CMD_GET_KEYBOARD_VALUE, VALUE_UPTIME),
            (CMD_GET_KEYBOARD_VALUE, VALUE_FIRMWARE_VERSION),
            (CMD_GET_KEYBOARD_VALUE, VALUE_LAYOUT_OPTIONS),
            (CMD_DYNAMIC_KEYMAP_GET_LAYER_COUNT, 0),
            (CMD_DYNAMIC_KEYMAP_GET_KEYCODE, 0),
            (CMD_DYNAMIC_KEYMAP_SET_KEYCODE, 0),
        ] {
            let resp = exchange(&via, &mut proc, &[cmd, sub, 0, 0]);
            assert_eq!(resp[0], cmd, "command 0x{cmd:02X} did not echo its id");
        }
    }

    #[test]
    fn an_unknown_command_is_answered_with_via_s_unhandled_marker() {
        // A Via host that sends a command this firmware does not implement must
        // still get 32 bytes back, or the exchange deadlocks — and byte 0 must
        // be 0xFF (id_unhandled), which is how the host detects it.
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let resp = exchange(&via, &mut proc, &[0xEE, 1, 2, 3]);
        assert_eq!(resp.len(), VIA_REPORT_SIZE);
        assert_eq!(resp[0], CMD_UNHANDLED);

        // An unknown *sub*-command of get_keyboard_value likewise.
        let resp = exchange(&via, &mut proc, &[CMD_GET_KEYBOARD_VALUE, 0x7E]);
        assert_eq!(resp[0], CMD_UNHANDLED);
    }

    #[test]
    fn out_of_range_positions_do_not_panic_or_corrupt_the_keymap() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let z = u16::from(KeyCode::Z.usage_id());
        // Layer, row, and column all past the end of a 2x3 / 2-layer keymap.
        for (layer, row, col) in [(9u8, 0u8, 0u8), (0, 9, 0), (0, 0, 9), (255, 255, 255)] {
            exchange(
                &via,
                &mut proc,
                &[
                    CMD_DYNAMIC_KEYMAP_SET_KEYCODE,
                    layer,
                    row,
                    col,
                    (z >> 8) as u8,
                    z as u8,
                ],
            );
        }
        // A valid key is untouched.
        let resp = exchange(&via, &mut proc, &[CMD_DYNAMIC_KEYMAP_GET_KEYCODE, 0, 0, 0]);
        let kc = (u16::from(resp[4]) << 8) | u16::from(resp[5]);
        assert_eq!(via_keycode_to_action::<()>(kc), Action::Key(KeyCode::A));
    }

    #[test]
    fn uptime_is_reported_big_endian_as_via_expects() {
        let mut via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        via.set_uptime(0x0102_0304);
        let resp = exchange(&via, &mut proc, &[CMD_GET_KEYBOARD_VALUE, VALUE_UPTIME]);
        assert_eq!(&resp[2..6], &[0x01, 0x02, 0x03, 0x04]);
    }
}
