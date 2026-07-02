keymap! {
    layer!(Base,
        // Left hand: outer -> inner. Column 0 is mostly a matrix spacer.
        // spacer              outer        pinky        ring         middle       index        inner
           NOOP,               ESC,         N1,          N2,          N3,          N4,          N5,
           NOOP,               CAPS,        Q,           W,           E,           R,           T,
           NOOP,               MT!(LCTL, BSPC), A,       S,           D,           F,           G,
           NOOP,               LSFT,        MT!(LALT, Z), X,          C,           V,           NOOP,
           NOOP,               NOOP,        NOOP,        NOOP,        B,           NOOP,        NOOP,
           LT!(Nav, ENT),      MT!(LGUI, TAB), NOOP,     NOOP,        NOOP,        NOOP,        NOOP,

        // Right hand: inner -> outer. Column 6 is mostly a matrix spacer.
        // inner               index        middle       ring         pinky        outer        spacer
           N6,                 N7,          N8,          N9,          N0,          MINS,        NOOP,
           Y,                  U,           I,           O,           P,           BSLS,        NOOP,
           H,                  J,           K,           L,           SCLN,        MT!(RSFT, QUOT), NOOP,
           NOOP,               M,           COMM,        DOT,         MT!(RALT, SLSH), RCTL,   NOOP,
           NOOP,               NOOP,        N,           NOOP,        NOOP,        NOOP,        NOOP,
           NOOP,               NOOP,        NOOP,        NOOP,        NOOP,        MT!(RSFT, BSPC), LT!(Media, SPC)
    ),
    layer!(Nav,
        // Left hand: outer -> inner.
        // spacer              outer        pinky        ring         middle       index        inner
           NOOP,               ESC,         F1,          F2,          F3,          F4,          F5,
           NOOP,               GRV,         N1,          N2,          N3,          N4,          N5,
           NOOP,               TRNS,        N6,          N7,          N8,          N9,          N0,
           NOOP,               TRNS,        TRNS,        LBRC,        RBRC,        LBRC,        NOOP,
           NOOP,               NOOP,        NOOP,        NOOP,        RBRC,        NOOP,        NOOP,
           TRNS,               TRNS,        NOOP,        NOOP,        NOOP,        NOOP,        NOOP,

        // Right hand: inner -> outer.
        // inner               index        middle       ring         pinky        outer        spacer
           F6,                 F7,          F8,          F9,          F10,         F11,         NOOP,
           N7,                 N8,          N9,          MINS,        SLSH,        F12,         NOOP,
           N4,                 N5,          N6,          EQL,         N8,          BSPC,        NOOP,
           NOOP,               N2,          N3,          DOT,         EQL,         ENT,         NOOP,
           NOOP,               NOOP,        N1,          NOOP,        NOOP,        NOOP,        NOOP,
           NOOP,               NOOP,        NOOP,        NOOP,        NOOP,        TRNS,        N0
    ),
    layer!(Media,
        // Left hand: outer -> inner.
        // spacer              outer        pinky        ring         middle       index        inner
           NOOP,               NOOP,        NOOP,        NOOP,        NOOP,        NOOP,        NOOP,
           NOOP,               TRNS,        TRNS,        NOOP,        NOOP,        NOOP,        TRNS,
           NOOP,               TRNS,        NOOP,        NOOP,        NOOP,        NOOP,        TRNS,
           NOOP,               TRNS,        TRNS,        TRNS,        TRNS,        TRNS,        NOOP,
           NOOP,               NOOP,        NOOP,        NOOP,        TRNS,        NOOP,        NOOP,
           TRNS,               TRNS,        NOOP,        NOOP,        NOOP,        NOOP,        NOOP,

        // Right hand: inner -> outer.
        // inner               index        middle       ring         pinky        outer        spacer
           TRNS,               TRNS,        TRNS,        TRNS,        TRNS,        NOOP,        NOOP,
           PGUP,               HOME,        UP,          END,         TRNS,        TRNS,        NOOP,
           PGDN,               LEFT,        DOWN,        RGHT,        TRNS,        TRNS,        NOOP,
           NOOP,               TRNS,        TRNS,        TRNS,        TRNS,        TRNS,        NOOP,
           NOOP,               NOOP,        TRNS,        NOOP,        NOOP,        NOOP,        NOOP,
           NOOP,               NOOP,        NOOP,        NOOP,        NOOP,        TRNS,        TRNS
    ),
}
