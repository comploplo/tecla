use super::ModifierKey;

/// A standard HID keyboard keycode.
///
/// Discriminants match the USB HID keyboard usage page (0x07).
/// Modifier keys (0xE0–0xE7) are included; the report builder will split
/// them into the modifier byte automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum KeyCode {
    /// No key.
    None = 0x00,

    // Letters
    /// Letter A.
    A = 0x04,
    /// Letter B.
    B = 0x05,
    /// Letter C.
    C = 0x06,
    /// Letter D.
    D = 0x07,
    /// Letter E.
    E = 0x08,
    /// Letter F.
    F = 0x09,
    /// Letter G.
    G = 0x0A,
    /// Letter H.
    H = 0x0B,
    /// Letter I.
    I = 0x0C,
    /// Letter J.
    J = 0x0D,
    /// Letter K.
    K = 0x0E,
    /// Letter L.
    L = 0x0F,
    /// Letter M.
    M = 0x10,
    /// Letter N.
    N = 0x11,
    /// Letter O.
    O = 0x12,
    /// Letter P.
    P = 0x13,
    /// Letter Q.
    Q = 0x14,
    /// Letter R.
    R = 0x15,
    /// Letter S.
    S = 0x16,
    /// Letter T.
    T = 0x17,
    /// Letter U.
    U = 0x18,
    /// Letter V.
    V = 0x19,
    /// Letter W.
    W = 0x1A,
    /// Letter X.
    X = 0x1B,
    /// Letter Y.
    Y = 0x1C,
    /// Letter Z.
    Z = 0x1D,

    // Digits
    /// Digit 1.
    N1 = 0x1E,
    /// Digit 2.
    N2 = 0x1F,
    /// Digit 3.
    N3 = 0x20,
    /// Digit 4.
    N4 = 0x21,
    /// Digit 5.
    N5 = 0x22,
    /// Digit 6.
    N6 = 0x23,
    /// Digit 7.
    N7 = 0x24,
    /// Digit 8.
    N8 = 0x25,
    /// Digit 9.
    N9 = 0x26,
    /// Digit 0.
    N0 = 0x27,

    // Common keys
    /// Enter.
    Enter = 0x28,
    /// Escape.
    Escape = 0x29,
    /// Backspace.
    Backspace = 0x2A,
    /// Tab.
    Tab = 0x2B,
    /// Space.
    Space = 0x2C,
    /// Minus / hyphen.
    Minus = 0x2D,
    /// Equal.
    Equal = 0x2E,
    /// Left brace.
    LeftBrace = 0x2F,
    /// Right brace.
    RightBrace = 0x30,
    /// Backslash.
    Backslash = 0x31,
    /// Non-US hash / tilde.
    NonUsHash = 0x32,
    /// Semicolon.
    Semicolon = 0x33,
    /// Quote.
    Quote = 0x34,
    /// Grave accent.
    Grave = 0x35,
    /// Comma.
    Comma = 0x36,
    /// Period / dot.
    Dot = 0x37,
    /// Forward slash.
    Slash = 0x38,
    /// Caps Lock.
    CapsLock = 0x39,

    // Function keys
    /// F1.
    F1 = 0x3A,
    /// F2.
    F2 = 0x3B,
    /// F3.
    F3 = 0x3C,
    /// F4.
    F4 = 0x3D,
    /// F5.
    F5 = 0x3E,
    /// F6.
    F6 = 0x3F,
    /// F7.
    F7 = 0x40,
    /// F8.
    F8 = 0x41,
    /// F9.
    F9 = 0x42,
    /// F10.
    F10 = 0x43,
    /// F11.
    F11 = 0x44,
    /// F12.
    F12 = 0x45,

    // Navigation & editing
    /// Print Screen.
    PrintScreen = 0x46,
    /// Scroll Lock.
    ScrollLock = 0x47,
    /// Pause / Break.
    Pause = 0x48,
    /// Insert.
    Insert = 0x49,
    /// Home.
    Home = 0x4A,
    /// Page Up.
    PageUp = 0x4B,
    /// Delete.
    Delete = 0x4C,
    /// End.
    End = 0x4D,
    /// Page Down.
    PageDown = 0x4E,
    /// Right arrow.
    Right = 0x4F,
    /// Left arrow.
    Left = 0x50,
    /// Down arrow.
    Down = 0x51,
    /// Up arrow.
    Up = 0x52,

    // Numpad
    /// Num Lock.
    NumLock = 0x53,
    /// Keypad slash.
    KpSlash = 0x54,
    /// Keypad asterisk.
    KpAster = 0x55,
    /// Keypad minus.
    KpMinus = 0x56,
    /// Keypad plus.
    KpPlus = 0x57,
    /// Keypad Enter.
    KpEnter = 0x58,
    /// Keypad 1.
    Kp1 = 0x59,
    /// Keypad 2.
    Kp2 = 0x5A,
    /// Keypad 3.
    Kp3 = 0x5B,
    /// Keypad 4.
    Kp4 = 0x5C,
    /// Keypad 5.
    Kp5 = 0x5D,
    /// Keypad 6.
    Kp6 = 0x5E,
    /// Keypad 7.
    Kp7 = 0x5F,
    /// Keypad 8.
    Kp8 = 0x60,
    /// Keypad 9.
    Kp9 = 0x61,
    /// Keypad 0.
    Kp0 = 0x62,
    /// Keypad dot.
    KpDot = 0x63,

    /// Non-US backslash / pipe.
    NonUsBackslash = 0x64,
    /// Application (context menu).
    Application = 0x65,
    /// Power.
    Power = 0x66,
    /// Keypad equal.
    KpEqual = 0x67,

    /// F13.
    F13 = 0x68,
    /// F14.
    F14 = 0x69,
    /// F15.
    F15 = 0x6A,
    /// F16.
    F16 = 0x6B,
    /// F17.
    F17 = 0x6C,
    /// F18.
    F18 = 0x6D,
    /// F19.
    F19 = 0x6E,
    /// F20.
    F20 = 0x6F,
    /// F21.
    F21 = 0x70,
    /// F22.
    F22 = 0x71,
    /// F23.
    F23 = 0x72,
    /// F24.
    F24 = 0x73,

    /// Execute.
    Execute = 0x74,
    /// Help.
    Help = 0x75,
    /// Select.
    Select = 0x77,
    /// Stop.
    Stop = 0x78,
    /// Again.
    Again = 0x79,
    /// Undo.
    Undo = 0x7A,
    /// Cut.
    Cut = 0x7B,
    /// Copy.
    Copy = 0x7C,
    /// Paste.
    Paste = 0x7D,
    /// Find.
    Find = 0x7E,

    /// Keypad comma.
    KpComma = 0x85,

    /// International 1 (Ro).
    International1 = 0x87,
    /// International 2 (Katakana/Hiragana).
    International2 = 0x88,
    /// International 3 (Yen).
    International3 = 0x89,
    /// International 4 (Henkan).
    International4 = 0x8A,
    /// International 5 (Muhenkan).
    International5 = 0x8B,

    /// Language 1 (Hangul/English).
    Language1 = 0x90,
    /// Language 2 (Hanja).
    Language2 = 0x91,
    /// Language 3 (Katakana).
    Language3 = 0x92,
    /// Language 4 (Hiragana).
    Language4 = 0x93,
    /// Language 5 (Zenkaku/Hankaku).
    Language5 = 0x94,

    // Modifiers
    /// Left Control modifier key.
    LCtrl = 0xE0,
    /// Left Shift modifier key.
    LShift = 0xE1,
    /// Left Alt modifier key.
    LAlt = 0xE2,
    /// Left GUI modifier key.
    LGui = 0xE3,
    /// Right Control modifier key.
    RCtrl = 0xE4,
    /// Right Shift modifier key.
    RShift = 0xE5,
    /// Right Alt modifier key.
    RAlt = 0xE6,
    /// Right GUI modifier key.
    RGui = 0xE7,
}

impl KeyCode {
    /// Returns the [`ModifierKey`] bitmask if this is a modifier, otherwise `None`.
    #[inline]
    #[must_use]
    pub const fn as_modifier(self) -> Option<ModifierKey> {
        let id = self.usage_id();
        if id < 0xE0 {
            return None;
        }
        Some(ModifierKey::from_bit_index(id - 0xE0))
    }

    /// Returns `true` if this keycode is a modifier key (usage 0xE0–0xE7).
    #[inline]
    #[must_use]
    pub const fn is_modifier(self) -> bool {
        self.usage_id() >= 0xE0
    }

    /// Returns the variant name as a static string.
    #[must_use]
    #[expect(clippy::too_many_lines, reason = "exhaustive match over all variants")]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
            Self::H => "H",
            Self::I => "I",
            Self::J => "J",
            Self::K => "K",
            Self::L => "L",
            Self::M => "M",
            Self::N => "N",
            Self::O => "O",
            Self::P => "P",
            Self::Q => "Q",
            Self::R => "R",
            Self::S => "S",
            Self::T => "T",
            Self::U => "U",
            Self::V => "V",
            Self::W => "W",
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
            Self::N1 => "N1",
            Self::N2 => "N2",
            Self::N3 => "N3",
            Self::N4 => "N4",
            Self::N5 => "N5",
            Self::N6 => "N6",
            Self::N7 => "N7",
            Self::N8 => "N8",
            Self::N9 => "N9",
            Self::N0 => "N0",
            Self::Enter => "Enter",
            Self::Escape => "Escape",
            Self::Backspace => "Backspace",
            Self::Tab => "Tab",
            Self::Space => "Space",
            Self::Minus => "Minus",
            Self::Equal => "Equal",
            Self::LeftBrace => "LeftBrace",
            Self::RightBrace => "RightBrace",
            Self::Backslash => "Backslash",
            Self::NonUsHash => "NonUsHash",
            Self::Semicolon => "Semicolon",
            Self::Quote => "Quote",
            Self::Grave => "Grave",
            Self::Comma => "Comma",
            Self::Dot => "Dot",
            Self::Slash => "Slash",
            Self::CapsLock => "CapsLock",
            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::F6 => "F6",
            Self::F7 => "F7",
            Self::F8 => "F8",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
            Self::PrintScreen => "PrintScreen",
            Self::ScrollLock => "ScrollLock",
            Self::Pause => "Pause",
            Self::Insert => "Insert",
            Self::Home => "Home",
            Self::PageUp => "PageUp",
            Self::Delete => "Delete",
            Self::End => "End",
            Self::PageDown => "PageDown",
            Self::Right => "Right",
            Self::Left => "Left",
            Self::Down => "Down",
            Self::Up => "Up",
            Self::NumLock => "NumLock",
            Self::KpSlash => "KpSlash",
            Self::KpAster => "KpAster",
            Self::KpMinus => "KpMinus",
            Self::KpPlus => "KpPlus",
            Self::KpEnter => "KpEnter",
            Self::Kp1 => "Kp1",
            Self::Kp2 => "Kp2",
            Self::Kp3 => "Kp3",
            Self::Kp4 => "Kp4",
            Self::Kp5 => "Kp5",
            Self::Kp6 => "Kp6",
            Self::Kp7 => "Kp7",
            Self::Kp8 => "Kp8",
            Self::Kp9 => "Kp9",
            Self::Kp0 => "Kp0",
            Self::KpDot => "KpDot",
            Self::NonUsBackslash => "NonUsBackslash",
            Self::Application => "Application",
            Self::Power => "Power",
            Self::KpEqual => "KpEqual",
            Self::F13 => "F13",
            Self::F14 => "F14",
            Self::F15 => "F15",
            Self::F16 => "F16",
            Self::F17 => "F17",
            Self::F18 => "F18",
            Self::F19 => "F19",
            Self::F20 => "F20",
            Self::F21 => "F21",
            Self::F22 => "F22",
            Self::F23 => "F23",
            Self::F24 => "F24",
            Self::Execute => "Execute",
            Self::Help => "Help",
            Self::Select => "Select",
            Self::Stop => "Stop",
            Self::Again => "Again",
            Self::Undo => "Undo",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::Find => "Find",
            Self::KpComma => "KpComma",
            Self::International1 => "International1",
            Self::International2 => "International2",
            Self::International3 => "International3",
            Self::International4 => "International4",
            Self::International5 => "International5",
            Self::Language1 => "Language1",
            Self::Language2 => "Language2",
            Self::Language3 => "Language3",
            Self::Language4 => "Language4",
            Self::Language5 => "Language5",
            Self::LCtrl => "LCtrl",
            Self::LShift => "LShift",
            Self::LAlt => "LAlt",
            Self::LGui => "LGui",
            Self::RCtrl => "RCtrl",
            Self::RShift => "RShift",
            Self::RAlt => "RAlt",
            Self::RGui => "RGui",
        }
    }

    /// Returns the raw HID usage ID.
    #[inline]
    #[must_use]
    #[expect(
        clippy::as_conversions,
        reason = "enum discriminant to u8 for HID usage ID"
    )]
    pub const fn usage_id(self) -> u8 {
        self as u8
    }

    /// Creates a [`KeyCode`] from a raw HID usage ID.
    ///
    /// Returns `None` for values that do not correspond to a defined variant.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive match over all HID usage IDs"
    )]
    pub const fn from_usage_id(id: u8) -> Option<Self> {
        match id {
            0x00 => Some(Self::None),
            0x04 => Some(Self::A),
            0x05 => Some(Self::B),
            0x06 => Some(Self::C),
            0x07 => Some(Self::D),
            0x08 => Some(Self::E),
            0x09 => Some(Self::F),
            0x0A => Some(Self::G),
            0x0B => Some(Self::H),
            0x0C => Some(Self::I),
            0x0D => Some(Self::J),
            0x0E => Some(Self::K),
            0x0F => Some(Self::L),
            0x10 => Some(Self::M),
            0x11 => Some(Self::N),
            0x12 => Some(Self::O),
            0x13 => Some(Self::P),
            0x14 => Some(Self::Q),
            0x15 => Some(Self::R),
            0x16 => Some(Self::S),
            0x17 => Some(Self::T),
            0x18 => Some(Self::U),
            0x19 => Some(Self::V),
            0x1A => Some(Self::W),
            0x1B => Some(Self::X),
            0x1C => Some(Self::Y),
            0x1D => Some(Self::Z),
            0x1E => Some(Self::N1),
            0x1F => Some(Self::N2),
            0x20 => Some(Self::N3),
            0x21 => Some(Self::N4),
            0x22 => Some(Self::N5),
            0x23 => Some(Self::N6),
            0x24 => Some(Self::N7),
            0x25 => Some(Self::N8),
            0x26 => Some(Self::N9),
            0x27 => Some(Self::N0),
            0x28 => Some(Self::Enter),
            0x29 => Some(Self::Escape),
            0x2A => Some(Self::Backspace),
            0x2B => Some(Self::Tab),
            0x2C => Some(Self::Space),
            0x2D => Some(Self::Minus),
            0x2E => Some(Self::Equal),
            0x2F => Some(Self::LeftBrace),
            0x30 => Some(Self::RightBrace),
            0x31 => Some(Self::Backslash),
            0x32 => Some(Self::NonUsHash),
            0x33 => Some(Self::Semicolon),
            0x34 => Some(Self::Quote),
            0x35 => Some(Self::Grave),
            0x36 => Some(Self::Comma),
            0x37 => Some(Self::Dot),
            0x38 => Some(Self::Slash),
            0x39 => Some(Self::CapsLock),
            0x3A => Some(Self::F1),
            0x3B => Some(Self::F2),
            0x3C => Some(Self::F3),
            0x3D => Some(Self::F4),
            0x3E => Some(Self::F5),
            0x3F => Some(Self::F6),
            0x40 => Some(Self::F7),
            0x41 => Some(Self::F8),
            0x42 => Some(Self::F9),
            0x43 => Some(Self::F10),
            0x44 => Some(Self::F11),
            0x45 => Some(Self::F12),
            0x46 => Some(Self::PrintScreen),
            0x47 => Some(Self::ScrollLock),
            0x48 => Some(Self::Pause),
            0x49 => Some(Self::Insert),
            0x4A => Some(Self::Home),
            0x4B => Some(Self::PageUp),
            0x4C => Some(Self::Delete),
            0x4D => Some(Self::End),
            0x4E => Some(Self::PageDown),
            0x4F => Some(Self::Right),
            0x50 => Some(Self::Left),
            0x51 => Some(Self::Down),
            0x52 => Some(Self::Up),
            0x53 => Some(Self::NumLock),
            0x54 => Some(Self::KpSlash),
            0x55 => Some(Self::KpAster),
            0x56 => Some(Self::KpMinus),
            0x57 => Some(Self::KpPlus),
            0x58 => Some(Self::KpEnter),
            0x59 => Some(Self::Kp1),
            0x5A => Some(Self::Kp2),
            0x5B => Some(Self::Kp3),
            0x5C => Some(Self::Kp4),
            0x5D => Some(Self::Kp5),
            0x5E => Some(Self::Kp6),
            0x5F => Some(Self::Kp7),
            0x60 => Some(Self::Kp8),
            0x61 => Some(Self::Kp9),
            0x62 => Some(Self::Kp0),
            0x63 => Some(Self::KpDot),
            0x64 => Some(Self::NonUsBackslash),
            0x65 => Some(Self::Application),
            0x66 => Some(Self::Power),
            0x67 => Some(Self::KpEqual),
            0x68 => Some(Self::F13),
            0x69 => Some(Self::F14),
            0x6A => Some(Self::F15),
            0x6B => Some(Self::F16),
            0x6C => Some(Self::F17),
            0x6D => Some(Self::F18),
            0x6E => Some(Self::F19),
            0x6F => Some(Self::F20),
            0x70 => Some(Self::F21),
            0x71 => Some(Self::F22),
            0x72 => Some(Self::F23),
            0x73 => Some(Self::F24),
            0x74 => Some(Self::Execute),
            0x75 => Some(Self::Help),
            0x77 => Some(Self::Select),
            0x78 => Some(Self::Stop),
            0x79 => Some(Self::Again),
            0x7A => Some(Self::Undo),
            0x7B => Some(Self::Cut),
            0x7C => Some(Self::Copy),
            0x7D => Some(Self::Paste),
            0x7E => Some(Self::Find),
            0x85 => Some(Self::KpComma),
            0x87 => Some(Self::International1),
            0x88 => Some(Self::International2),
            0x89 => Some(Self::International3),
            0x8A => Some(Self::International4),
            0x8B => Some(Self::International5),
            0x90 => Some(Self::Language1),
            0x91 => Some(Self::Language2),
            0x92 => Some(Self::Language3),
            0x93 => Some(Self::Language4),
            0x94 => Some(Self::Language5),
            0xE0 => Some(Self::LCtrl),
            0xE1 => Some(Self::LShift),
            0xE2 => Some(Self::LAlt),
            0xE3 => Some(Self::LGui),
            0xE4 => Some(Self::RCtrl),
            0xE5 => Some(Self::RShift),
            0xE6 => Some(Self::RAlt),
            0xE7 => Some(Self::RGui),
            _ => None,
        }
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "test code")]
mod tests {
    use super::*;

    #[test]
    fn keycode_a_usage_id_is_four() {
        assert_eq!(KeyCode::A.usage_id(), 0x04);
    }

    #[test]
    fn keycode_letters_span_twenty_six_consecutive_ids() {
        assert_eq!(KeyCode::Z.usage_id(), KeyCode::A.usage_id() + 25);
    }

    #[test]
    fn keycode_none_usage_id_is_zero() {
        assert_eq!(KeyCode::None.usage_id(), 0x00);
    }

    #[test]
    #[expect(clippy::too_many_lines, reason = "exhaustive match table")]
    fn from_usage_id_round_trips_all_valid_keycodes() {
        let all = [
            KeyCode::None,
            KeyCode::A,
            KeyCode::B,
            KeyCode::C,
            KeyCode::D,
            KeyCode::E,
            KeyCode::F,
            KeyCode::G,
            KeyCode::H,
            KeyCode::I,
            KeyCode::J,
            KeyCode::K,
            KeyCode::L,
            KeyCode::M,
            KeyCode::N,
            KeyCode::O,
            KeyCode::P,
            KeyCode::Q,
            KeyCode::R,
            KeyCode::S,
            KeyCode::T,
            KeyCode::U,
            KeyCode::V,
            KeyCode::W,
            KeyCode::X,
            KeyCode::Y,
            KeyCode::Z,
            KeyCode::N1,
            KeyCode::N2,
            KeyCode::N3,
            KeyCode::N4,
            KeyCode::N5,
            KeyCode::N6,
            KeyCode::N7,
            KeyCode::N8,
            KeyCode::N9,
            KeyCode::N0,
            KeyCode::Enter,
            KeyCode::Escape,
            KeyCode::Backspace,
            KeyCode::Tab,
            KeyCode::Space,
            KeyCode::Minus,
            KeyCode::Equal,
            KeyCode::LeftBrace,
            KeyCode::RightBrace,
            KeyCode::Backslash,
            KeyCode::NonUsHash,
            KeyCode::Semicolon,
            KeyCode::Quote,
            KeyCode::Grave,
            KeyCode::Comma,
            KeyCode::Dot,
            KeyCode::Slash,
            KeyCode::CapsLock,
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
            KeyCode::PrintScreen,
            KeyCode::ScrollLock,
            KeyCode::Pause,
            KeyCode::Insert,
            KeyCode::Home,
            KeyCode::PageUp,
            KeyCode::Delete,
            KeyCode::End,
            KeyCode::PageDown,
            KeyCode::Right,
            KeyCode::Left,
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::NumLock,
            KeyCode::KpSlash,
            KeyCode::KpAster,
            KeyCode::KpMinus,
            KeyCode::KpPlus,
            KeyCode::KpEnter,
            KeyCode::Kp1,
            KeyCode::Kp2,
            KeyCode::Kp3,
            KeyCode::Kp4,
            KeyCode::Kp5,
            KeyCode::Kp6,
            KeyCode::Kp7,
            KeyCode::Kp8,
            KeyCode::Kp9,
            KeyCode::Kp0,
            KeyCode::KpDot,
            KeyCode::NonUsBackslash,
            KeyCode::Application,
            KeyCode::Power,
            KeyCode::KpEqual,
            KeyCode::F13,
            KeyCode::F14,
            KeyCode::F15,
            KeyCode::F16,
            KeyCode::F17,
            KeyCode::F18,
            KeyCode::F19,
            KeyCode::F20,
            KeyCode::F21,
            KeyCode::F22,
            KeyCode::F23,
            KeyCode::F24,
            KeyCode::Execute,
            KeyCode::Help,
            KeyCode::Select,
            KeyCode::Stop,
            KeyCode::Again,
            KeyCode::Undo,
            KeyCode::Cut,
            KeyCode::Copy,
            KeyCode::Paste,
            KeyCode::Find,
            KeyCode::KpComma,
            KeyCode::International1,
            KeyCode::International2,
            KeyCode::International3,
            KeyCode::International4,
            KeyCode::International5,
            KeyCode::Language1,
            KeyCode::Language2,
            KeyCode::Language3,
            KeyCode::Language4,
            KeyCode::Language5,
            KeyCode::LCtrl,
            KeyCode::LShift,
            KeyCode::LAlt,
            KeyCode::LGui,
            KeyCode::RCtrl,
            KeyCode::RShift,
            KeyCode::RAlt,
            KeyCode::RGui,
        ];
        for kc in all {
            assert_eq!(
                KeyCode::from_usage_id(kc.usage_id()),
                Some(kc),
                "round-trip failed for {kc:?}"
            );
        }
    }

    #[test]
    fn from_usage_id_rejects_invalid_bytes() {
        assert_eq!(KeyCode::from_usage_id(0x01), None);
        assert_eq!(KeyCode::from_usage_id(0x02), None);
        assert_eq!(KeyCode::from_usage_id(0x03), None);
        assert_eq!(KeyCode::from_usage_id(0x76), None);
        assert_eq!(KeyCode::from_usage_id(0x99), None);
        assert_eq!(KeyCode::from_usage_id(0xDF), None);
        assert_eq!(KeyCode::from_usage_id(0xE8), None);
        assert_eq!(KeyCode::from_usage_id(0xFF), None);
    }

    #[test]
    fn modifier_keycodes_are_detected_as_modifiers() {
        assert!(KeyCode::LCtrl.is_modifier());
        assert!(KeyCode::LShift.is_modifier());
        assert!(KeyCode::RAlt.is_modifier());
        assert!(KeyCode::RGui.is_modifier());
    }

    #[test]
    fn non_modifier_keycodes_are_not_modifiers() {
        assert!(!KeyCode::A.is_modifier());
        assert!(!KeyCode::Space.is_modifier());
        assert!(!KeyCode::F12.is_modifier());
    }

    #[test]
    fn all_modifier_keycodes_map_to_correct_bitmasks() {
        let pairs = [
            (KeyCode::LCtrl, ModifierKey::LCTRL),
            (KeyCode::LShift, ModifierKey::LSHIFT),
            (KeyCode::LAlt, ModifierKey::LALT),
            (KeyCode::LGui, ModifierKey::LGUI),
            (KeyCode::RCtrl, ModifierKey::RCTRL),
            (KeyCode::RShift, ModifierKey::RSHIFT),
            (KeyCode::RAlt, ModifierKey::RALT),
            (KeyCode::RGui, ModifierKey::RGUI),
        ];
        for (kc, expected) in pairs {
            assert_eq!(kc.as_modifier(), Some(expected));
        }
    }

    #[test]
    #[expect(clippy::too_many_lines, reason = "exhaustive match table")]
    fn name_round_trips_through_usage_id() {
        let all = [
            KeyCode::None,
            KeyCode::A,
            KeyCode::B,
            KeyCode::C,
            KeyCode::D,
            KeyCode::E,
            KeyCode::F,
            KeyCode::G,
            KeyCode::H,
            KeyCode::I,
            KeyCode::J,
            KeyCode::K,
            KeyCode::L,
            KeyCode::M,
            KeyCode::N,
            KeyCode::O,
            KeyCode::P,
            KeyCode::Q,
            KeyCode::R,
            KeyCode::S,
            KeyCode::T,
            KeyCode::U,
            KeyCode::V,
            KeyCode::W,
            KeyCode::X,
            KeyCode::Y,
            KeyCode::Z,
            KeyCode::N1,
            KeyCode::N2,
            KeyCode::N3,
            KeyCode::N4,
            KeyCode::N5,
            KeyCode::N6,
            KeyCode::N7,
            KeyCode::N8,
            KeyCode::N9,
            KeyCode::N0,
            KeyCode::Enter,
            KeyCode::Escape,
            KeyCode::Backspace,
            KeyCode::Tab,
            KeyCode::Space,
            KeyCode::Minus,
            KeyCode::Equal,
            KeyCode::LeftBrace,
            KeyCode::RightBrace,
            KeyCode::Backslash,
            KeyCode::NonUsHash,
            KeyCode::Semicolon,
            KeyCode::Quote,
            KeyCode::Grave,
            KeyCode::Comma,
            KeyCode::Dot,
            KeyCode::Slash,
            KeyCode::CapsLock,
            KeyCode::F1,
            KeyCode::F2,
            KeyCode::F3,
            KeyCode::F4,
            KeyCode::F5,
            KeyCode::F6,
            KeyCode::F7,
            KeyCode::F8,
            KeyCode::F9,
            KeyCode::F10,
            KeyCode::F11,
            KeyCode::F12,
            KeyCode::PrintScreen,
            KeyCode::ScrollLock,
            KeyCode::Pause,
            KeyCode::Insert,
            KeyCode::Home,
            KeyCode::PageUp,
            KeyCode::Delete,
            KeyCode::End,
            KeyCode::PageDown,
            KeyCode::Right,
            KeyCode::Left,
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::NumLock,
            KeyCode::KpSlash,
            KeyCode::KpAster,
            KeyCode::KpMinus,
            KeyCode::KpPlus,
            KeyCode::KpEnter,
            KeyCode::Kp1,
            KeyCode::Kp2,
            KeyCode::Kp3,
            KeyCode::Kp4,
            KeyCode::Kp5,
            KeyCode::Kp6,
            KeyCode::Kp7,
            KeyCode::Kp8,
            KeyCode::Kp9,
            KeyCode::Kp0,
            KeyCode::KpDot,
            KeyCode::NonUsBackslash,
            KeyCode::Application,
            KeyCode::Power,
            KeyCode::KpEqual,
            KeyCode::F13,
            KeyCode::F14,
            KeyCode::F15,
            KeyCode::F16,
            KeyCode::F17,
            KeyCode::F18,
            KeyCode::F19,
            KeyCode::F20,
            KeyCode::F21,
            KeyCode::F22,
            KeyCode::F23,
            KeyCode::F24,
            KeyCode::Execute,
            KeyCode::Help,
            KeyCode::Select,
            KeyCode::Stop,
            KeyCode::Again,
            KeyCode::Undo,
            KeyCode::Cut,
            KeyCode::Copy,
            KeyCode::Paste,
            KeyCode::Find,
            KeyCode::KpComma,
            KeyCode::International1,
            KeyCode::International2,
            KeyCode::International3,
            KeyCode::International4,
            KeyCode::International5,
            KeyCode::Language1,
            KeyCode::Language2,
            KeyCode::Language3,
            KeyCode::Language4,
            KeyCode::Language5,
            KeyCode::LCtrl,
            KeyCode::LShift,
            KeyCode::LAlt,
            KeyCode::LGui,
            KeyCode::RCtrl,
            KeyCode::RShift,
            KeyCode::RAlt,
            KeyCode::RGui,
        ];
        for kc in all {
            let round_tripped = KeyCode::from_usage_id(kc.usage_id())
                .expect("from_usage_id should return Some for a valid keycode");
            assert_eq!(
                round_tripped.name(),
                kc.name(),
                "name round-trip failed for {kc:?}"
            );
        }
    }

    #[test]
    fn non_modifier_keycode_as_modifier_is_none() {
        assert_eq!(KeyCode::A.as_modifier(), None);
        assert_eq!(KeyCode::Space.as_modifier(), None);
    }
}
