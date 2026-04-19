//! ZMK key name to tecla key name mapping.

use std::collections::HashMap;
use std::sync::LazyLock;

static ZMK_TO_TECLA: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Letters
    for c in b'A'..=b'Z' {
        let s: &'static str = match c {
            b'A' => "A", b'B' => "B", b'C' => "C", b'D' => "D", b'E' => "E",
            b'F' => "F", b'G' => "G", b'H' => "H", b'I' => "I", b'J' => "J",
            b'K' => "K", b'L' => "L", b'M' => "M", b'N' => "N", b'O' => "O",
            b'P' => "P", b'Q' => "Q", b'R' => "R", b'S' => "S", b'T' => "T",
            b'U' => "U", b'V' => "V", b'W' => "W", b'X' => "X", b'Y' => "Y",
            b'Z' => "Z", _ => unreachable!(),
        };
        m.insert(s, s);
    }

    // Numbers (ZMK uses N1..N9, N0 and NUMBER_1..NUMBER_9, NUMBER_0)
    m.insert("N0", "N0");
    m.insert("N1", "N1");
    m.insert("N2", "N2");
    m.insert("N3", "N3");
    m.insert("N4", "N4");
    m.insert("N5", "N5");
    m.insert("N6", "N6");
    m.insert("N7", "N7");
    m.insert("N8", "N8");
    m.insert("N9", "N9");
    m.insert("NUMBER_0", "N0");
    m.insert("NUMBER_1", "N1");
    m.insert("NUMBER_2", "N2");
    m.insert("NUMBER_3", "N3");
    m.insert("NUMBER_4", "N4");
    m.insert("NUMBER_5", "N5");
    m.insert("NUMBER_6", "N6");
    m.insert("NUMBER_7", "N7");
    m.insert("NUMBER_8", "N8");
    m.insert("NUMBER_9", "N9");
    m.insert("NUM_1", "N1");
    m.insert("NUM_2", "N2");
    m.insert("NUM_3", "N3");
    m.insert("NUM_4", "N4");
    m.insert("NUM_5", "N5");
    m.insert("NUM_6", "N6");
    m.insert("NUM_7", "N7");
    m.insert("NUM_8", "N8");
    m.insert("NUM_9", "N9");

    // Whitespace / editing
    m.insert("SPACE", "SPC");
    m.insert("TAB", "TAB");
    m.insert("ENTER", "ENT");
    m.insert("RET", "ENT");
    m.insert("RETURN", "ENT");
    m.insert("RETURN2", "ENT");
    m.insert("ESCAPE", "ESC");
    m.insert("ESC", "ESC");
    m.insert("BACKSPACE", "BSPC");
    m.insert("BSPC", "BSPC");
    m.insert("BKSP", "BSPC");
    m.insert("DELETE", "DEL");
    m.insert("DEL", "DEL");
    m.insert("INSERT", "INS");
    m.insert("INS", "INS");

    // Punctuation — primary names
    m.insert("MINUS", "MINS");
    m.insert("EQUAL", "EQL");
    m.insert("LEFT_BRACKET", "LBRC");
    m.insert("LBKT", "LBRC");
    m.insert("LBRC", "LBRC");
    m.insert("RIGHT_BRACKET", "RBRC");
    m.insert("RBKT", "RBRC");
    m.insert("RBRC", "RBRC");
    m.insert("BACKSLASH", "BSLS");
    m.insert("BSLH", "BSLS");
    m.insert("NON_US_HASH", "NUHS");
    m.insert("NUHS", "NUHS");
    m.insert("SEMICOLON", "SCLN");
    m.insert("SEMI", "SCLN");
    m.insert("APOSTROPHE", "QUOT");
    m.insert("APOS", "QUOT");
    m.insert("SQT", "QUOT");
    m.insert("QUOT", "QUOT");
    m.insert("GRAVE", "GRV");
    m.insert("COMMA", "COMM");
    m.insert("PERIOD", "DOT");
    m.insert("DOT", "DOT");
    m.insert("SLASH", "SLSH");
    m.insert("FSLH", "SLSH");
    m.insert("NON_US_BACKSLASH", "NUBS");
    m.insert("NON_US_BSLH", "NUBS");

    // Shifted punctuation (ZMK names for shifted symbols)
    m.insert("EXCLAMATION", "EXLM");
    m.insert("EXCL", "EXLM");
    m.insert("AT_SIGN", "AT");
    m.insert("AT", "AT");
    m.insert("HASH", "HASH");
    m.insert("POUND", "HASH");
    m.insert("DOLLAR", "DLR");
    m.insert("DLLR", "DLR");
    m.insert("PERCENT", "PERC");
    m.insert("PRCNT", "PERC");
    m.insert("CARET", "CIRC");
    m.insert("AMPERSAND", "AMPR");
    m.insert("AMPS", "AMPR");
    m.insert("ASTERISK", "ASTR");
    m.insert("ASTRK", "ASTR");
    m.insert("STAR", "ASTR");
    m.insert("LEFT_PARENTHESIS", "LPRN");
    m.insert("LPAR", "LPRN");
    m.insert("RIGHT_PARENTHESIS", "RPRN");
    m.insert("RPAR", "RPRN");
    m.insert("UNDERSCORE", "UNDS");
    m.insert("UNDER", "UNDS");
    m.insert("PLUS", "PLUS");
    m.insert("LEFT_BRACE", "LCBR");
    m.insert("RIGHT_BRACE", "RCBR");
    m.insert("PIPE", "PIPE");
    m.insert("PIPE2", "PIPE");
    m.insert("COLON", "COLN");
    m.insert("DOUBLE_QUOTES", "DQUO");
    m.insert("DQT", "DQUO");
    m.insert("TILDE", "TILD");
    m.insert("TILDE2", "TILD");
    m.insert("LESS_THAN", "LT");
    m.insert("GREATER_THAN", "GT");
    m.insert("GT", "GT");
    m.insert("QUESTION", "QUES");
    m.insert("QMARK", "QUES");

    // Navigation
    m.insert("HOME", "HOME");
    m.insert("END", "END");
    m.insert("PAGE_UP", "PGUP");
    m.insert("PG_UP", "PGUP");
    m.insert("PAGE_DOWN", "PGDN");
    m.insert("PG_DN", "PGDN");
    m.insert("UP", "UP");
    m.insert("UP_ARROW", "UP");
    m.insert("UARW", "UP");
    m.insert("DOWN", "DOWN");
    m.insert("DOWN_ARROW", "DOWN");
    m.insert("DARW", "DOWN");
    m.insert("LEFT", "LEFT");
    m.insert("LEFT_ARROW", "LEFT");
    m.insert("LARW", "LEFT");
    m.insert("RIGHT", "RGHT");
    m.insert("RIGHT_ARROW", "RGHT");
    m.insert("RARW", "RGHT");

    // Function keys
    m.insert("F1", "F1");
    m.insert("F2", "F2");
    m.insert("F3", "F3");
    m.insert("F4", "F4");
    m.insert("F5", "F5");
    m.insert("F6", "F6");
    m.insert("F7", "F7");
    m.insert("F8", "F8");
    m.insert("F9", "F9");
    m.insert("F10", "F10");
    m.insert("F11", "F11");
    m.insert("F12", "F12");
    m.insert("F13", "F13");
    m.insert("F14", "F14");
    m.insert("F15", "F15");
    m.insert("F16", "F16");

    // Modifiers (used as standalone keys with &kp)
    m.insert("LEFT_CONTROL", "LCTL");
    m.insert("LCTRL", "LCTL");
    m.insert("LEFT_SHIFT", "LSFT");
    m.insert("LSHFT", "LSFT");
    m.insert("LSHIFT", "LSFT");
    m.insert("LEFT_ALT", "LALT");
    m.insert("LALT", "LALT");
    m.insert("LEFT_GUI", "LGUI");
    m.insert("LGUI", "LGUI");
    m.insert("LCMD", "LGUI");
    m.insert("LMETA", "LGUI");
    m.insert("LWIN", "LGUI");
    m.insert("RIGHT_CONTROL", "RCTL");
    m.insert("RCTRL", "RCTL");
    m.insert("RCTL", "RCTL");
    m.insert("RIGHT_SHIFT", "RSFT");
    m.insert("RSHFT", "RSFT");
    m.insert("LEFT_GUI", "LGUI");
    m.insert("RIGHT_ALT", "RALT");
    m.insert("RALT", "RALT");
    m.insert("RIGHT_GUI", "RGUI");
    m.insert("RGUI", "RGUI");
    m.insert("RCMD", "RGUI");
    m.insert("RMETA", "RGUI");

    // Lock keys
    m.insert("CAPS", "CAPS");
    m.insert("CAPSLOCK", "CAPS");
    m.insert("CLCK", "CAPS");
    m.insert("CAPS_LOCK", "CAPS");
    m.insert("SCROLL_LOCK", "SLCK");
    m.insert("SLCK", "SLCK");
    m.insert("KP_NUMLOCK", "NLCK");
    m.insert("KP_NUM", "NLCK");
    m.insert("NUM_LOCK", "NLCK");

    // Print screen / pause
    m.insert("PRINTSCREEN", "PSCR");
    m.insert("PSCRN", "PSCR");
    m.insert("PAUSE_BREAK", "PAUS");

    // Keypad
    m.insert("KP_SLASH", "PSLS");
    m.insert("KP_DIVIDE", "PSLS");
    m.insert("KP_ASTERISK", "PAST");
    m.insert("KP_MULTIPLY", "PAST");
    m.insert("KP_MINUS", "PMNS");
    m.insert("KP_PLUS", "PPLS");
    m.insert("KP_ENTER", "PENT");
    m.insert("KP_DOT", "PDOT");
    m.insert("KP_EQUAL", "PEQL");
    m.insert("KP_N0", "P0");
    m.insert("KP_N1", "P1");
    m.insert("KP_N2", "P2");
    m.insert("KP_N3", "P3");
    m.insert("KP_N4", "P4");
    m.insert("KP_N5", "P5");
    m.insert("KP_N6", "P6");
    m.insert("KP_N7", "P7");
    m.insert("KP_N8", "P8");
    m.insert("KP_N9", "P9");
    m.insert("KP_NUMBER_0", "P0");
    m.insert("KP_NUMBER_1", "P1");
    m.insert("KP_NUMBER_2", "P2");
    m.insert("KP_NUMBER_3", "P3");
    m.insert("KP_NUMBER_4", "P4");
    m.insert("KP_NUMBER_5", "P5");
    m.insert("KP_NUMBER_6", "P6");
    m.insert("KP_NUMBER_7", "P7");
    m.insert("KP_NUMBER_8", "P8");
    m.insert("KP_NUMBER_9", "P9");

    // Application / menu
    m.insert("K_APP", "APP");
    m.insert("K_CMENU", "APP");
    m.insert("APPLICATION", "APP");

    // International
    m.insert("INTERNATIONAL_1", "INT1");
    m.insert("INTERNATIONAL_2", "INT2");
    m.insert("INTERNATIONAL_3", "INT3");
    m.insert("INTERNATIONAL_4", "INT4");
    m.insert("INTERNATIONAL_5", "INT5");
    m.insert("LANG1", "LANG1");
    m.insert("LANG2", "LANG2");
    m.insert("LANGUAGE_1", "LANG1");
    m.insert("LANGUAGE_2", "LANG2");
    m.insert("LANGUAGE_3", "LANG3");
    m.insert("LANGUAGE_4", "LANG4");
    m.insert("LANGUAGE_5", "LANG5");
    m.insert("LT", "LT");

    // Consumer keys
    m.insert("C_MUTE", "CONSUMER(MUTE)");
    m.insert("K_MUTE", "CONSUMER(MUTE)");
    m.insert("C_VOL_UP", "CONSUMER(VOL_UP)");
    m.insert("K_VOL_UP", "CONSUMER(VOL_UP)");
    m.insert("C_VOL_DN", "CONSUMER(VOL_DN)");
    m.insert("K_VOL_DN", "CONSUMER(VOL_DN)");
    m.insert("C_NEXT", "CONSUMER(NEXT)");
    m.insert("C_PREV", "CONSUMER(PREV)");
    m.insert("C_PREVIOUS", "CONSUMER(PREV)");
    m.insert("C_PLAY_PAUSE", "CONSUMER(PLAY_PAUSE)");
    m.insert("C_PP", "CONSUMER(PLAY_PAUSE)");
    m.insert("C_PLAY", "CONSUMER(PLAY)");
    m.insert("K_PP", "CONSUMER(PLAY_PAUSE)");
    m.insert("C_STOP", "CONSUMER(STOP)");
    m.insert("C_BRI_UP", "CONSUMER(BRI_UP)");
    m.insert("C_BRI_DN", "CONSUMER(BRI_DN)");
    m.insert("C_BRIGHTNESS_UP", "CONSUMER(BRI_UP)");
    m.insert("C_BRIGHTNESS_DOWN", "CONSUMER(BRI_DN)");
    m.insert("C_PWR", "CONSUMER(POWER)");
    m.insert("C_POWER", "CONSUMER(POWER)");

    // Editing keys
    m.insert("K_UNDO", "UNDO");
    m.insert("K_CUT", "CUT");
    m.insert("K_COPY", "COPY");
    m.insert("K_PASTE", "PASTE");
    m.insert("K_LOCK", "LOCK");

    // Power
    m.insert("POWER", "POWER");

    m
});

/// Maps a ZMK key name (e.g. `"TAB"`, `"LSHFT"`) to a tecla key name.
pub fn zmk_key_to_tecla(name: &str) -> Option<&'static str> {
    ZMK_TO_TECLA.get(name).copied()
}

/// Maps a ZMK modifier name (e.g. `"LSHFT"`, `"LC"`) to a tecla modifier.
pub fn zmk_mod_to_tecla(name: &str) -> Option<&'static str> {
    match name {
        "LCTRL" | "LEFT_CONTROL" | "LC" => Some("LCTL"),
        "LSHFT" | "LSHIFT" | "LEFT_SHIFT" | "LS" => Some("LSFT"),
        "LALT" | "LEFT_ALT" | "LA" => Some("LALT"),
        "LGUI" | "LEFT_GUI" | "LCMD" | "LMETA" | "LWIN" | "LG" => Some("LGUI"),
        "RCTRL" | "RIGHT_CONTROL" | "RC" => Some("RCTL"),
        "RSHFT" | "RIGHT_SHIFT" | "RS" => Some("RSFT"),
        "RALT" | "RIGHT_ALT" | "RA" => Some("RALT"),
        "RGUI" | "RIGHT_GUI" | "RCMD" | "RMETA" | "RWIN" | "RG" => Some("RGUI"),
        "RSHIFT" => Some("RSFT"),
        _ => None,
    }
}
