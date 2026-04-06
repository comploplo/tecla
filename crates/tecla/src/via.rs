//! Via / Vial compatibility layer — dynamic keymap remapping over USB HID.
//!
//! Implements the Via protocol command set for runtime keymap editing.
//! Feature-gated behind `via` (which implies `storage`).

use crate::action::{Action, KeyCode, LayerOp, ModifierKey};
#[cfg(feature = "storage")]
use crate::eeprom::EepromShim;
use crate::keyboard::KeyboardProcessor;

const VIA_PROTOCOL_VERSION: u16 = 0x000C;

const CMD_GET_PROTOCOL_VERSION: u8 = 0x01;
const CMD_GET_UPTIME: u8 = 0x02;
const CMD_GET_LAYER_COUNT: u8 = 0x04;
const CMD_DYNAMIC_KEYMAP_GET_KEYCODE: u8 = 0x0C;
const CMD_DYNAMIC_KEYMAP_SET_KEYCODE: u8 = 0x0D;
const CMD_DYNAMIC_KEYMAP_RESET: u8 = 0x09;
const CMD_EEPROM_READ: u8 = 0x23;
const CMD_EEPROM_WRITE: u8 = 0x24;

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

        match request[0] {
            CMD_GET_PROTOCOL_VERSION => {
                response[1] = (VIA_PROTOCOL_VERSION >> 8) as u8;
                response[2] = VIA_PROTOCOL_VERSION as u8;
            }
            CMD_GET_UPTIME => {
                let ms = self.uptime_ms;
                response[1] = (ms >> 24) as u8;
                response[2] = (ms >> 16) as u8;
                response[3] = (ms >> 8) as u8;
                response[4] = ms as u8;
            }
            CMD_GET_LAYER_COUNT => {
                response[1] = LAYERS as u8;
            }
            CMD_DYNAMIC_KEYMAP_GET_KEYCODE => {
                let layer = usize::from(request[1]);
                let row = usize::from(request[2]);
                let col = usize::from(request[3]);
                let key = row * COLS + col;
                let via_kc = processor
                    .get_action(layer, key)
                    .map_or(0, |a| action_to_via_keycode(&a));
                response[1] = (via_kc >> 8) as u8;
                response[2] = via_kc as u8;
            }
            CMD_DYNAMIC_KEYMAP_SET_KEYCODE => {
                let layer = usize::from(request[1]);
                let row = usize::from(request[2]);
                let col = usize::from(request[3]);
                let via_kc = (u16::from(request[4]) << 8) | u16::from(request[5]);
                let key = row * COLS + col;
                let action = via_keycode_to_action(via_kc);
                processor.set_action(layer, key, action);
            }
            CMD_DYNAMIC_KEYMAP_RESET => {
                for (layer_idx, &layer_keys) in self.default_keymap.iter().enumerate() {
                    for (key_idx, &via_kc) in layer_keys.iter().enumerate() {
                        let action = via_keycode_to_action(via_kc);
                        processor.set_action(layer_idx, key_idx, action);
                    }
                }
            }
            _ => {}
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
    use crate::keymap::{Layer, LayerStack};
    use crate::tapping::TapHoldConfig;

    fn make_processor() -> KeyboardProcessor<2, 3, 2, 6> {
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
        req[0] = CMD_GET_UPTIME;
        via.process_command(&req, &mut resp, &mut proc);
        let ms = (u64::from(resp[1]) << 24)
            | (u64::from(resp[2]) << 16)
            | (u64::from(resp[3]) << 8)
            | u64::from(resp[4]);
        assert_eq!(ms, 0x1234_5678);
    }

    #[test]
    fn get_layer_count() {
        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut proc = make_processor();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = CMD_GET_LAYER_COUNT;
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
        let kc = (u16::from(resp[1]) << 8) | u16::from(resp[2]);
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
        let kc = (u16::from(resp[1]) << 8) | u16::from(resp[2]);
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
        use crate::eeprom::EepromShim;

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
        use crate::eeprom::EepromShim;

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
        use crate::eeprom::EepromShim;

        let via: ViaProtocol<3> = ViaProtocol::new(&[]);
        let mut eeprom = EepromShim::<64>::new();
        let mut req = [0u8; 32];
        let mut resp = [0u8; 32];
        req[0] = 0xFF;
        assert!(!via.process_eeprom_command(&req, &mut resp, &mut eeprom));
    }
}
