//! Proc macros for tecla keyboard firmware.
//!
//! This crate exposes six macros for declaring keymaps at compile time with
//! no runtime allocation and full compile-time validation.
//!
//! | Macro | Purpose |
//! |-------|---------|
//! | [`keymap!`] | Build a complete [`LayerStack`] from named layer declarations. |
//! | [`steno!`] | Build a complete [`LayerStack`] from steno bit-index declarations. |
//! | [`layer!`] | Build a single [`Layer`] from a key identifier list. |
//! | [`MT!`] | Mod-tap shorthand: hold = modifier, tap = key. |
//! | [`LT!`] | Layer-tap shorthand: hold = momentary layer, tap = key. |
//! | [`keyboard_main`] | Entry-point attribute; wraps `async fn` with `#[embassy_executor::main]`. |
//!
//! [`LayerStack`]: tecla::keymap::LayerStack
//! [`Layer`]: tecla::keymap::Layer
//!
//! # Key identifiers
//!
//! Both `layer!` and the inner positions of `MT!`/`LT!` use a QMK-compatible
//! short-name vocabulary. The full table:
//!
//! **Letters:** `A`–`Z`
//!
//! **Digits:** `N1`–`N9`, `N0`
//!
//! **Function keys:** `F1`–`F12`
//!
//! **Common keys:**
//!
//! | Identifier(s) | Key |
//! |---------------|-----|
//! | `SPC`, `SPACE` | Space |
//! | `ENT`, `ENTER` | Enter |
//! | `ESC` | Escape |
//! | `TAB` | Tab |
//! | `BSPC`, `BSP`, `BKSP` | Backspace |
//! | `DEL` | Delete |
//! | `INS` | Insert |
//! | `HOME` | Home |
//! | `END` | End |
//! | `PGUP` | Page Up |
//! | `PGDN` | Page Down |
//! | `UP` / `DOWN` / `LEFT` / `RGHT` / `RIGHT` | Arrow keys |
//! | `CAPS` | Caps Lock |
//! | `PSCR` | Print Screen |
//! | `PAUS`, `BRKE` | Pause/Break |
//! | `SLCK` | Scroll Lock |
//! | `NLCK` | Num Lock |
//!
//! **Modifier keys (as regular key actions):**
//!
//! | Identifier(s) | Key |
//! |---------------|-----|
//! | `LCTL`, `LCTRL` | Left Control |
//! | `RCTL`, `RCTRL` | Right Control |
//! | `LSFT`, `LSHIFT` | Left Shift |
//! | `RSFT`, `RSHIFT` | Right Shift |
//! | `LALT` | Left Alt |
//! | `RALT` | Right Alt |
//! | `LGUI`, `LWIN`, `LCMD` | Left GUI |
//! | `RGUI`, `RWIN`, `RCMD` | Right GUI |
//!
//! **Punctuation:**
//!
//! | Identifier | Key |
//! |------------|-----|
//! | `MINS`, `MINUS` | `-` |
//! | `EQL`, `EQUAL` | `=` |
//! | `LBRC` | `[` |
//! | `RBRC` | `]` |
//! | `BSLS` | `\` |
//! | `SCLN` | `;` |
//! | `QUOT` | `'` |
//! | `GRV` | `` ` `` |
//! | `COMM` | `,` |
//! | `DOT` | `.` |
//! | `SLSH` | `/` |
//!
//! **Numpad:** `KP0`–`KP9`, `KPSLSH`, `KPAST`, `KPMNS`, `KPPLS`, `KPENT`, `KPDOT`
//!
//! **Special layer identifiers** (only valid in `layer!`/`keymap!`, not as tap keys):
//!
//! | Identifier | Meaning |
//! |------------|---------|
//! | `TRNS`, `TRANSPARENT` | Fall through to the next active layer below. |
//! | `NOOP`, `NoOp`, `XXXXXXX` | Explicitly block fallthrough — produce no key. |
//!
//! # Layer names in `LT!`
//!
//! Inside `keymap!`, the first argument to `LT!` may be either a numeric
//! layer index **or a layer name** declared in the same `keymap!` block.
//! Names are resolved at macro-expansion time with no runtime cost.
//!
//! ```rust,ignore
//! keymap! {
//!     layer!(Base, A, B, LT!(Fn, SPC)),   // "Fn" → layer index 1
//!     layer!(Fn,  F1, F2, TRNS),
//! }
//! ```
//!
//! Forward references are supported — `Base` may reference `Fn` even though
//! `Fn` is declared later in the list. Numeric indices (`LT!(1, SPC)`) also
//! continue to work unchanged.
//!
//! The following are **compile errors** with span-accurate diagnostics:
//! - A name that doesn't match any layer in the same `keymap!`.
//! - Duplicate layer names within one `keymap!`.
//! - A layer name used in standalone `layer!` (no name map available — use a
//!   numeric index instead).
//! - Layers with mismatched key counts.
//!
//! # Modifier identifiers in `MT!`
//!
//! The first argument to `MT!` must be one of the eight modifier names:
//! `LCTL`/`LCTRL`, `RCTL`/`RCTRL`, `LSFT`/`LSHIFT`, `RSFT`/`RSHIFT`,
//! `LALT`, `RALT`, `LGUI`/`LWIN`/`LCMD`, `RGUI`/`RWIN`/`RCMD`.
#![expect(
    non_snake_case,
    reason = "MT and LT are proc macro names matching their macro syntax"
)]

use proc_macro::TokenStream;
use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
use quote::quote;
use std::collections::HashMap;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Error, Ident, LitInt, Token,
};

fn ident_to_action(name: &str, span: Span) -> Result<TokenStream2, Error> {
    if let Some(ts) = special_token(name) {
        return Ok(ts);
    }
    let kc = ident_to_keycode(name, span)?;
    Ok(quote! { ::tecla::action::Action::<()>::Key(#kc) })
}

fn special_token(name: &str) -> Option<TokenStream2> {
    match name {
        "TRNS" | "TRANSPARENT" => Some(quote! { ::tecla::action::Action::<()>::Transparent }),
        "NOOP" | "NoOp" | "XXXXXXX" => Some(quote! { ::tecla::action::Action::<()>::NoOp }),
        "OUT_USB" => Some(
            quote! { ::tecla::action::Action::<()>::OutputSelect(::tecla::hid::OutputMode::Usb) },
        ),
        "OUT_BLE" => Some(
            quote! { ::tecla::action::Action::<()>::OutputSelect(::tecla::hid::OutputMode::Ble) },
        ),
        _ => None,
    }
}

fn ident_to_modifier(name: &str, span: Span) -> Result<TokenStream2, Error> {
    let mk = |variant: &str| -> TokenStream2 {
        let v = Ident::new(variant, span);
        quote! { ::tecla::action::ModifierKey::#v }
    };
    let result = match name {
        "LCTL" | "LCTRL" => mk("LCTRL"),
        "RCTL" | "RCTRL" => mk("RCTRL"),
        "LSFT" | "LSHIFT" => mk("LSHIFT"),
        "RSFT" | "RSHIFT" => mk("RSHIFT"),
        "LALT" => mk("LALT"),
        "RALT" => mk("RALT"),
        "LGUI" | "LWIN" | "LCMD" => mk("LGUI"),
        "RGUI" | "RWIN" | "RCMD" => mk("RGUI"),
        _ => {
            return Err(Error::new(
                span,
                format!("unknown modifier identifier `{name}`"),
            ));
        }
    };
    Ok(result)
}

fn is_modifier_prefix(name: &str) -> bool {
    matches!(
        name,
        "LSFT" | "RSFT" | "LCTL" | "RCTL" | "LALT" | "RALT" | "LGUI" | "RGUI"
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "exhaustive match over all QMK key name aliases"
)]
fn keycode_variant(name: &str) -> Option<&'static str> {
    match name {
        "A" => Some("A"),
        "B" => Some("B"),
        "C" => Some("C"),
        "D" => Some("D"),
        "E" => Some("E"),
        "F" => Some("F"),
        "G" => Some("G"),
        "H" => Some("H"),
        "I" => Some("I"),
        "J" => Some("J"),
        "K" => Some("K"),
        "L" => Some("L"),
        "M" => Some("M"),
        "N" => Some("N"),
        "O" => Some("O"),
        "P" => Some("P"),
        "Q" => Some("Q"),
        "R" => Some("R"),
        "S" => Some("S"),
        "T" => Some("T"),
        "U" => Some("U"),
        "V" => Some("V"),
        "W" => Some("W"),
        "X" => Some("X"),
        "Y" => Some("Y"),
        "Z" => Some("Z"),
        "N1" => Some("N1"),
        "N2" => Some("N2"),
        "N3" => Some("N3"),
        "N4" => Some("N4"),
        "N5" => Some("N5"),
        "N6" => Some("N6"),
        "N7" => Some("N7"),
        "N8" => Some("N8"),
        "N9" => Some("N9"),
        "N0" => Some("N0"),
        "Enter" => Some("Enter"),
        "Escape" => Some("Escape"),
        "Backspace" => Some("Backspace"),
        "Tab" => Some("Tab"),
        "Space" => Some("Space"),
        "Minus" => Some("Minus"),
        "Equal" => Some("Equal"),
        "LeftBrace" => Some("LeftBrace"),
        "RightBrace" => Some("RightBrace"),
        "Backslash" => Some("Backslash"),
        "NonUsHash" => Some("NonUsHash"),
        "Semicolon" => Some("Semicolon"),
        "Quote" => Some("Quote"),
        "Grave" => Some("Grave"),
        "Comma" => Some("Comma"),
        "Dot" => Some("Dot"),
        "Slash" => Some("Slash"),
        "CapsLock" => Some("CapsLock"),
        "F1" => Some("F1"),
        "F2" => Some("F2"),
        "F3" => Some("F3"),
        "F4" => Some("F4"),
        "F5" => Some("F5"),
        "F6" => Some("F6"),
        "F7" => Some("F7"),
        "F8" => Some("F8"),
        "F9" => Some("F9"),
        "F10" => Some("F10"),
        "F11" => Some("F11"),
        "F12" => Some("F12"),
        "F13" => Some("F13"),
        "F14" => Some("F14"),
        "F15" => Some("F15"),
        "F16" => Some("F16"),
        "F17" => Some("F17"),
        "F18" => Some("F18"),
        "F19" => Some("F19"),
        "F20" => Some("F20"),
        "F21" => Some("F21"),
        "F22" => Some("F22"),
        "F23" => Some("F23"),
        "F24" => Some("F24"),
        "PrintScreen" => Some("PrintScreen"),
        "ScrollLock" => Some("ScrollLock"),
        "Pause" => Some("Pause"),
        "Insert" => Some("Insert"),
        "Home" => Some("Home"),
        "PageUp" => Some("PageUp"),
        "Delete" => Some("Delete"),
        "End" => Some("End"),
        "PageDown" => Some("PageDown"),
        "Right" => Some("Right"),
        "Left" => Some("Left"),
        "Down" => Some("Down"),
        "Up" => Some("Up"),
        "NumLock" => Some("NumLock"),
        "KpSlash" => Some("KpSlash"),
        "KpAster" => Some("KpAster"),
        "KpMinus" => Some("KpMinus"),
        "KpPlus" => Some("KpPlus"),
        "KpEnter" => Some("KpEnter"),
        "Kp1" => Some("Kp1"),
        "Kp2" => Some("Kp2"),
        "Kp3" => Some("Kp3"),
        "Kp4" => Some("Kp4"),
        "Kp5" => Some("Kp5"),
        "Kp6" => Some("Kp6"),
        "Kp7" => Some("Kp7"),
        "Kp8" => Some("Kp8"),
        "Kp9" => Some("Kp9"),
        "Kp0" => Some("Kp0"),
        "KpDot" => Some("KpDot"),
        "NonUsBackslash" => Some("NonUsBackslash"),
        "Application" => Some("Application"),
        "Power" => Some("Power"),
        "KpEqual" => Some("KpEqual"),
        "Execute" => Some("Execute"),
        "Help" => Some("Help"),
        "Select" => Some("Select"),
        "Stop" => Some("Stop"),
        "Again" => Some("Again"),
        "Undo" => Some("Undo"),
        "Cut" => Some("Cut"),
        "Copy" => Some("Copy"),
        "Paste" => Some("Paste"),
        "Find" => Some("Find"),
        "KpComma" => Some("KpComma"),
        "International1" => Some("International1"),
        "International2" => Some("International2"),
        "International3" => Some("International3"),
        "International4" => Some("International4"),
        "International5" => Some("International5"),
        "Language1" => Some("Language1"),
        "Language2" => Some("Language2"),
        "Language3" => Some("Language3"),
        "Language4" => Some("Language4"),
        "Language5" => Some("Language5"),
        "LCtrl" => Some("LCtrl"),
        "LShift" => Some("LShift"),
        "LAlt" => Some("LAlt"),
        "LGui" => Some("LGui"),
        "RCtrl" => Some("RCtrl"),
        "RShift" => Some("RShift"),
        "RAlt" => Some("RAlt"),
        "RGui" => Some("RGui"),
        _ => None,
    }
}

fn alias_to_variant(name: &str) -> Option<&'static str> {
    match name {
        "SPC" | "SPACE" => Some("Space"),
        "ENT" | "ENTER" => Some("Enter"),
        "ESC" => Some("Escape"),
        "BSPC" | "BSP" | "BKSP" => Some("Backspace"),
        "MINS" | "MINUS" => Some("Minus"),
        "EQL" | "EQUAL" => Some("Equal"),
        "LBRC" => Some("LeftBrace"),
        "RBRC" => Some("RightBrace"),
        "BSLS" => Some("Backslash"),
        "SCLN" => Some("Semicolon"),
        "QUOT" => Some("Quote"),
        "GRV" => Some("Grave"),
        "COMM" => Some("Comma"),
        "SLSH" => Some("Slash"),
        "CAPS" => Some("CapsLock"),
        "PSCR" => Some("PrintScreen"),
        "SLCK" => Some("ScrollLock"),
        "PAUS" | "BRKE" => Some("Pause"),
        "INS" => Some("Insert"),
        "PGUP" => Some("PageUp"),
        "DEL" => Some("Delete"),
        "PGDN" => Some("PageDown"),
        "RGHT" | "RIGHT" => Some("Right"),
        "NLCK" => Some("NumLock"),
        "NUBS" => Some("NonUsBackslash"),
        "NUHS" => Some("NonUsHash"),
        "APP" => Some("Application"),
        "LCTL" | "LCTRL" => Some("LCtrl"),
        "RCTL" | "RCTRL" => Some("RCtrl"),
        "LSFT" | "LSHIFT" => Some("LShift"),
        "RSFT" | "RSHIFT" => Some("RShift"),
        "LALT" => Some("LAlt"),
        "RALT" => Some("RAlt"),
        "LGUI" | "LWIN" | "LCMD" => Some("LGui"),
        "RGUI" | "RWIN" | "RCMD" => Some("RGui"),
        "TAB" => Some("Tab"),
        "DOT" => Some("Dot"),
        "HOME" => Some("Home"),
        "END" => Some("End"),
        "UP" => Some("Up"),
        "DOWN" => Some("Down"),
        "LEFT" => Some("Left"),
        "KP0" => Some("Kp0"),
        "KP1" => Some("Kp1"),
        "KP2" => Some("Kp2"),
        "KP3" => Some("Kp3"),
        "KP4" => Some("Kp4"),
        "KP5" => Some("Kp5"),
        "KP6" => Some("Kp6"),
        "KP7" => Some("Kp7"),
        "KP8" => Some("Kp8"),
        "KP9" => Some("Kp9"),
        "KPSLSH" => Some("KpSlash"),
        "KPAST" => Some("KpAster"),
        "KPMNS" => Some("KpMinus"),
        "KPPLS" => Some("KpPlus"),
        "KPENT" => Some("KpEnter"),
        "KPDOT" => Some("KpDot"),
        "KPEQL" | "PEQL" => Some("KpEqual"),
        "KPCOMM" | "PCMM" => Some("KpComma"),
        "INT1" => Some("International1"),
        "INT2" => Some("International2"),
        "INT3" => Some("International3"),
        "INT4" => Some("International4"),
        "INT5" => Some("International5"),
        "LNG1" => Some("Language1"),
        "LNG2" => Some("Language2"),
        "LNG3" => Some("Language3"),
        "LNG4" => Some("Language4"),
        "LNG5" => Some("Language5"),
        _ => None,
    }
}

fn ident_to_keycode(name: &str, span: Span) -> Result<TokenStream2, Error> {
    let variant = keycode_variant(name)
        .or_else(|| alias_to_variant(name))
        .ok_or_else(|| Error::new(span, format!("unknown key identifier `{name}`")))?;
    let v = Ident::new(variant, span);
    Ok(quote! { ::tecla::action::KeyCode::#v })
}

enum LayerRef {
    Index(LitInt),
    Name(Ident),
}

enum KeyExpr {
    Bare(Ident),
    Direct(TokenStream2),
    ModTap { mod_ident: Ident, key_ident: Ident },
    LayerTap { layer: LayerRef, key_ident: Ident },
}

impl KeyExpr {
    fn to_tokens(&self, name_map: &HashMap<String, u8>) -> Result<TokenStream2, Error> {
        match self {
            Self::Bare(ident) => ident_to_action(&ident.to_string(), ident.span()),
            Self::Direct(ts) => Ok(ts.clone()),
            Self::ModTap {
                mod_ident,
                key_ident,
            } => {
                let hold = ident_to_modifier(&mod_ident.to_string(), mod_ident.span())?;
                let tap = ident_to_keycode(&key_ident.to_string(), key_ident.span())?;
                Ok(quote! {
                    ::tecla::action::Action::<()>::ModTap { hold: #hold, tap: #tap }
                })
            }
            Self::LayerTap { layer, key_ident } => {
                let layer_idx: u8 = match layer {
                    LayerRef::Index(lit) => {
                        let v: u64 = lit.base10_parse().map_err(|e| {
                            Error::new(lit.span(), format!("invalid layer index: {e}"))
                        })?;
                        u8::try_from(v).map_err(|e| {
                            Error::new(
                                lit.span(),
                                format!("layer index out of u8 range (0\u{2013}255): {e}"),
                            )
                        })?
                    }
                    LayerRef::Name(name_ident) => {
                        let name = name_ident.to_string();
                        if name_map.is_empty() {
                            return Err(Error::new(
                                name_ident.span(),
                                format!(
                                    "layer name `{name}` can only be used inside keymap!; \
                                     use a numeric index in standalone layer! or LT!"
                                ),
                            ));
                        }
                        *name_map.get(&name).ok_or_else(|| {
                            Error::new(name_ident.span(), format!("unknown layer name `{name}`"))
                        })?
                    }
                };
                let layer_lit = Literal::u8_suffixed(layer_idx);
                let tap = ident_to_keycode(&key_ident.to_string(), key_ident.span())?;
                Ok(quote! {
                    ::tecla::action::Action::<()>::LayerTap { layer: #layer_lit, tap: #tap }
                })
            }
        }
    }
}

struct LayerInput {
    name: Ident,
    keys: Vec<KeyExpr>,
}

struct StenoLayerInput {
    name: Ident,
    bits: Vec<LitInt>,
}

fn parse_call_expr(name: &str, span: Span, content: ParseStream<'_>) -> Result<KeyExpr, Error> {
    match name {
        "MO" | "TG" | "TO" | "DF" => {
            let n: LitInt = content.parse()?;
            let val: u8 = n
                .base10_parse()
                .map_err(|e| Error::new(n.span(), format!("invalid layer index: {e}")))?;
            let lit = Literal::u8_suffixed(val);
            #[expect(
                clippy::unreachable,
                reason = "outer match arm constrains name to MO|TG|TO|DF"
            )]
            let op_variant = match name {
                "MO" => "Momentary",
                "TG" => "Toggle",
                "TO" => "On",
                "DF" => "SetDefault",
                _ => unreachable!(),
            };
            let op = Ident::new(op_variant, span);
            Ok(KeyExpr::Direct(quote! {
                ::tecla::action::Action::<()>::Layer(::tecla::action::LayerOp::#op(#lit))
            }))
        }
        "TT" => {
            let n: LitInt = content.parse()?;
            let val: u8 = n
                .base10_parse()
                .map_err(|e| Error::new(n.span(), format!("invalid layer index: {e}")))?;
            let lit = Literal::u8_suffixed(val);
            Ok(KeyExpr::Direct(quote! {
                ::tecla::action::Action::<()>::TapToggle(#lit)
            }))
        }
        "OSL" => {
            let n: LitInt = content.parse()?;
            let val: u8 = n
                .base10_parse()
                .map_err(|e| Error::new(n.span(), format!("invalid layer index: {e}")))?;
            let lit = Literal::u8_suffixed(val);
            Ok(KeyExpr::Direct(quote! {
                ::tecla::action::Action::<()>::OneShotLayer(#lit)
            }))
        }
        "OSM" => {
            let mod_ident: Ident = content.parse()?;
            let modifier = ident_to_modifier(&mod_ident.to_string(), mod_ident.span())?;
            Ok(KeyExpr::Direct(quote! {
                ::tecla::action::Action::<()>::OneShotMod(#modifier)
            }))
        }
        "STENO" => {
            let n: LitInt = content.parse()?;
            let val: u16 = n
                .base10_parse()
                .map_err(|e| Error::new(n.span(), format!("invalid steno bit index: {e}")))?;
            if val > 63 {
                return Err(Error::new(
                    n.span(),
                    format!("steno bit index must be in the range 0..=63, got {val}"),
                ));
            }
            let lit = Literal::u8_suffixed(val as u8);
            Ok(KeyExpr::Direct(quote! {
                ::tecla::action::Action::<()>::Steno(#lit)
            }))
        }
        "HID" => {
            let n: LitInt = content.parse()?;
            let val: u8 = n
                .base10_parse()
                .map_err(|e| Error::new(n.span(), format!("invalid HID usage ID: {e}")))?;
            let lit = Literal::u8_suffixed(val);
            Ok(KeyExpr::Direct(quote! {
                {
                    const _: () = {
                        if ::tecla::action::KeyCode::from_usage_id(#lit).is_none() {
                            ::core::panic!("invalid HID usage ID");
                        }
                    };
                    ::tecla::action::Action::<()>::Key(
                        match ::tecla::action::KeyCode::from_usage_id(#lit) {
                            ::core::option::Option::Some(kc) => kc,
                            ::core::option::Option::None => unreachable!(),
                        }
                    )
                }
            }))
        }
        _ if is_modifier_prefix(name) => {
            let key_ident: Ident = content.parse()?;
            let modifier = ident_to_modifier(name, span)?;
            let key = ident_to_keycode(&key_ident.to_string(), key_ident.span())?;
            Ok(KeyExpr::Direct(quote! {
                ::tecla::action::Action::<()>::ModifiedKey {
                    modifier: #modifier,
                    key: #key,
                }
            }))
        }
        _ => Err(Error::new(
            span,
            format!("unknown call expression `{name}`"),
        )),
    }
}

fn parse_key_expr(input: ParseStream<'_>) -> Result<KeyExpr, Error> {
    let ident: Ident = input.parse()?;
    let name = ident.to_string();

    if (matches!(
        name.as_str(),
        "MO" | "TG" | "TO" | "DF" | "TT" | "OSL" | "OSM" | "STENO" | "HID"
    ) || is_modifier_prefix(&name))
        && input.peek(syn::token::Paren)
    {
        let content;
        syn::parenthesized!(content in input);
        return parse_call_expr(&name, ident.span(), &content);
    }

    if (name == "MT" || name == "LT") && input.peek(syn::token::Not) {
        let _bang: Token![!] = input.parse()?;
        let content;
        syn::parenthesized!(content in input);

        if name == "MT" {
            let mod_ident: Ident = content.parse()?;
            let _comma: Token![,] = content.parse()?;
            let key_ident: Ident = content.parse()?;
            if !content.is_empty() {
                return Err(content.error("MT! takes exactly two arguments"));
            }
            return Ok(KeyExpr::ModTap {
                mod_ident,
                key_ident,
            });
        }

        let layer = if content.peek(LitInt) {
            LayerRef::Index(content.parse::<LitInt>()?)
        } else {
            LayerRef::Name(content.parse::<Ident>()?)
        };
        let _comma: Token![,] = content.parse()?;
        let key_ident: Ident = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("LT! takes exactly two arguments"));
        }
        return Ok(KeyExpr::LayerTap { layer, key_ident });
    }

    Ok(KeyExpr::Bare(ident))
}

impl Parse for LayerInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let _comma: Token![,] = input.parse()?;

        let mut keys = Vec::new();
        while !input.is_empty() {
            keys.push(parse_key_expr(input)?);
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        Ok(Self { name, keys })
    }
}

impl Parse for StenoLayerInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let _comma: Token![,] = input.parse()?;

        let mut bits = Vec::new();
        while !input.is_empty() {
            bits.push(input.parse()?);
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        Ok(Self { name, bits })
    }
}

fn emit_layer(
    layer_input: &LayerInput,
    name_map: &HashMap<String, u8>,
) -> Result<TokenStream2, Error> {
    let mut action_tokens = Vec::new();
    for key in &layer_input.keys {
        action_tokens.push(key.to_tokens(name_map)?);
    }
    Ok(quote! {
        ::tecla::pipeline::keymap::Layer::new(&[
            #(#action_tokens),*
        ])
    })
}

fn emit_steno_layer(layer_input: &StenoLayerInput) -> Result<TokenStream2, Error> {
    let mut action_tokens = Vec::new();
    for bit in &layer_input.bits {
        let val: u16 = bit
            .base10_parse()
            .map_err(|e| Error::new(bit.span(), format!("invalid steno bit index: {e}")))?;
        if val > 63 {
            return Err(Error::new(
                bit.span(),
                format!("steno bit index must be in the range 0..=63, got {val}"),
            ));
        }
        let lit = Literal::u8_suffixed(val as u8);
        action_tokens.push(quote! {
            ::tecla::action::Action::<()>::Steno(#lit)
        });
    }
    Ok(quote! {
        ::tecla::pipeline::keymap::Layer::new(&[
            #(#action_tokens),*
        ])
    })
}

struct KeymapInput {
    layers: Vec<LayerInput>,
}

impl Parse for KeymapInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut layers = Vec::new();
        while !input.is_empty() {
            let mac_name: Ident = input.parse()?;
            if mac_name != "layer" {
                return Err(Error::new(
                    mac_name.span(),
                    "expected `layer!(...)` invocation",
                ));
            }
            let _bang: Token![!] = input.parse()?;
            let content;
            syn::parenthesized!(content in input);
            let layer_input: LayerInput = content.parse()?;
            layers.push(layer_input);

            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }
        Ok(Self { layers })
    }
}

struct StenoInput {
    layers: Vec<StenoLayerInput>,
}

impl Parse for StenoInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut layers = Vec::new();
        while !input.is_empty() {
            let mac_name: Ident = input.parse()?;
            if mac_name != "layer" {
                return Err(Error::new(
                    mac_name.span(),
                    "expected `layer!(...)` invocation",
                ));
            }
            let _bang: Token![!] = input.parse()?;
            let content;
            syn::parenthesized!(content in input);
            let layer_input: StenoLayerInput = content.parse()?;
            layers.push(layer_input);

            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }
        Ok(Self { layers })
    }
}

/// Build a [`LayerStack`] from a list of named layer declarations.
///
/// Each argument must be a `layer!(Name, KEY, ...)` invocation. All layers
/// must have the same number of keys — a mismatch is a compile error pointing
/// at the offending layer name.
///
/// Layer names declared here can be used as the first argument to `LT!`
/// anywhere in the same `keymap!`, including forward references.
/// Numeric indices still work too. Duplicate layer names are a compile error.
///
/// [`LayerStack`]: tecla::keymap::LayerStack
///
/// # Example
///
/// ```ignore
/// let stack = keymap! {
///     layer!(Base,  A, B, LT!(Fn, SPC), MT!(LSFT, ENT)),
///     layer!(Fn,   F1, F2, TRNS,        TRNS),
/// };
/// ```
#[proc_macro]
pub fn keymap(input: TokenStream) -> TokenStream {
    let keymap_input = match syn::parse::<KeymapInput>(input) {
        Ok(k) => k,
        Err(e) => return e.to_compile_error().into(),
    };

    let num_layers = keymap_input.layers.len();
    let Some(first_layer) = keymap_input.layers.first() else {
        return Error::new(Span::call_site(), "keymap! requires at least one layer")
            .to_compile_error()
            .into();
    };

    let first_key_count = first_layer.keys.len();
    for layer in keymap_input.layers.iter().skip(1) {
        if layer.keys.len() != first_key_count {
            return Error::new(
                layer.name.span(),
                format!(
                    "layer `{}` has {} keys but layer `{}` has {}; \
                     all layers must have the same key count",
                    layer.name,
                    layer.keys.len(),
                    first_layer.name,
                    first_key_count
                ),
            )
            .to_compile_error()
            .into();
        }
    }

    let mut name_map: HashMap<String, u8> = HashMap::new();
    for (i, layer) in keymap_input.layers.iter().enumerate() {
        let name = layer.name.to_string();
        if let Some(prev_idx) = name_map.get(&name) {
            return Error::new(
                layer.name.span(),
                format!("duplicate layer name `{name}`; already defined as layer {prev_idx}"),
            )
            .to_compile_error()
            .into();
        }
        let Ok(idx) = u8::try_from(i) else {
            return Error::new(layer.name.span(), "keymap! supports at most 255 layers")
                .to_compile_error()
                .into();
        };
        name_map.insert(name, idx);
    }

    let num_keys = first_key_count;

    let mut layer_pushes = Vec::new();
    for layer_input in &keymap_input.layers {
        let layer_ts = match emit_layer(layer_input, &name_map) {
            Ok(ts) => ts,
            Err(e) => return e.to_compile_error().into(),
        };
        layer_pushes.push(quote! {
            _layers.push(#layer_ts).ok();
        });
    }

    quote! {
        {
            let mut _layers: ::heapless::Vec<
                ::tecla::pipeline::keymap::Layer<()>,
                #num_layers
            > = ::heapless::Vec::new();
            #(#layer_pushes)*
            ::tecla::pipeline::keymap::LayerStack::<#num_layers, #num_keys, ()>::new(_layers)
        }
    }
    .into()
}

/// Build a complete [`LayerStack`] for steno keyboards from bit-index layers.
///
/// The syntax mirrors [`keymap!`], but each layer uses numeric bit positions
/// instead of key identifiers. Each number becomes [`Action::Steno`] at the
/// matching key position.
///
/// # Example
///
/// ```ignore
/// let stack: LayerStack<1, 12, ()> = steno! {
///     layer!(Base, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11),
/// };
/// ```
#[proc_macro]
pub fn steno(input: TokenStream) -> TokenStream {
    let steno_input = match syn::parse::<StenoInput>(input) {
        Ok(k) => k,
        Err(e) => return e.to_compile_error().into(),
    };

    let num_layers = steno_input.layers.len();
    let Some(first_layer) = steno_input.layers.first() else {
        return Error::new(Span::call_site(), "steno! requires at least one layer")
            .to_compile_error()
            .into();
    };

    let first_key_count = first_layer.bits.len();
    for layer in steno_input.layers.iter().skip(1) {
        if layer.bits.len() != first_key_count {
            return Error::new(
                layer.name.span(),
                format!(
                    "layer `{}` has {} keys but layer `{}` has {}; \
                     all layers must have the same key count",
                    layer.name,
                    layer.bits.len(),
                    first_layer.name,
                    first_key_count
                ),
            )
            .to_compile_error()
            .into();
        }
    }

    let num_keys = first_key_count;

    let mut layer_pushes = Vec::new();
    for layer_input in &steno_input.layers {
        let layer_ts = match emit_steno_layer(layer_input) {
            Ok(ts) => ts,
            Err(e) => return e.to_compile_error().into(),
        };
        layer_pushes.push(quote! {
            _layers.push(#layer_ts).ok();
        });
    }

    quote! {
        {
            let mut _layers: ::heapless::Vec<
                ::tecla::pipeline::keymap::Layer<()>,
                #num_layers
            > = ::heapless::Vec::new();
            #(#layer_pushes)*
            ::tecla::pipeline::keymap::LayerStack::<#num_layers, #num_keys, ()>::new(_layers)
        }
    }
    .into()
}

/// Build a single [`Layer`] from a list of key identifiers.
///
/// The first argument is the layer name (documentation only — discarded at
/// compile time). The remaining arguments are key identifiers from the
/// vocabulary in the crate docs, plus `MT!(MOD, KEY)` and `LT!(N, KEY)`
/// inline expressions.
///
/// Layer names **cannot** be used in `LT!` here — there is no `keymap!`
/// context to resolve them against. Use a numeric index instead, or use
/// `keymap!` if you want named references.
///
/// [`Layer`]: tecla::keymap::Layer
///
/// # Example
///
/// ```ignore
/// let l: Layer<()> = layer!(Base, A, B, MT!(LSFT, ENT), LT!(1, SPC));
/// ```
#[proc_macro]
pub fn layer(input: TokenStream) -> TokenStream {
    let layer_input = match syn::parse::<LayerInput>(input) {
        Ok(l) => l,
        Err(e) => return e.to_compile_error().into(),
    };
    match emit_layer(&layer_input, &HashMap::new()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Mod-tap shorthand: hold = modifier, tap = key.
///
/// Expands to [`Action::ModTap`]. The first argument must be a modifier
/// identifier (`LSFT`, `LCTL`, `LALT`, `LGUI`, and their `R*` / long-form
/// equivalents). The second must be a key identifier from the standard table.
///
/// Can be used standalone or inline inside `layer!` / `keymap!`.
///
/// [`Action::ModTap`]: tecla::action::Action::ModTap
///
/// # Example
///
/// ```ignore
/// let a: Action<()> = MT!(LSFT, A);   // hold left-shift, tap A
/// let b: Action<()> = MT!(RCTL, ESC); // hold right-ctrl, tap Escape
/// ```
#[proc_macro]
pub fn MT(input: TokenStream) -> TokenStream {
    let parser = |input: ParseStream<'_>| -> syn::Result<(Ident, Ident)> {
        let args = Punctuated::<Ident, Token![,]>::parse_separated_nonempty(input)?;
        let mut iter = args.into_iter();
        let mod_ident = iter.next().ok_or_else(|| {
            Error::new(
                Span::call_site(),
                "MT! requires two arguments: MT!(MOD, KEY)",
            )
        })?;
        let key_ident = iter.next().ok_or_else(|| {
            Error::new(
                Span::call_site(),
                "MT! requires two arguments: MT!(MOD, KEY)",
            )
        })?;
        if iter.next().is_some() {
            return Err(Error::new(
                Span::call_site(),
                "MT! takes exactly two arguments",
            ));
        }
        Ok((mod_ident, key_ident))
    };

    let (mod_ident, key_ident) = match syn::parse::Parser::parse(parser, input) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let hold = match ident_to_modifier(&mod_ident.to_string(), mod_ident.span()) {
        Ok(ts) => ts,
        Err(e) => return e.to_compile_error().into(),
    };
    let tap = match ident_to_keycode(&key_ident.to_string(), key_ident.span()) {
        Ok(ts) => ts,
        Err(e) => return e.to_compile_error().into(),
    };

    quote! {
        ::tecla::action::Action::<()>::ModTap { hold: #hold, tap: #tap }
    }
    .into()
}

/// Layer-tap shorthand: hold = momentary layer, tap = key.
///
/// Expands to [`Action::LayerTap`]. The first argument is a layer index
/// (numeric literal, 0–255). The second is a key identifier from the
/// standard table.
///
/// When used **inside `keymap!`**, the first argument may also be a layer
/// name declared in the same `keymap!` block (including forward references).
/// Outside of `keymap!` only numeric indices are accepted.
///
/// [`Action::LayerTap`]: tecla::action::Action::LayerTap
///
/// # Example
///
/// ```ignore
/// let a: Action<()> = LT!(1, SPC);  // hold layer 1, tap space
///
/// keymap! {
///     layer!(Base, A, B, LT!(Fn, SPC)),  // name reference
///     layer!(Fn,  F1, F2, TRNS),
/// }
/// ```
#[proc_macro]
pub fn LT(input: TokenStream) -> TokenStream {
    let parser = |input: ParseStream<'_>| -> syn::Result<(LitInt, Ident)> {
        let layer: LitInt = input.parse()?;
        let _comma: Token![,] = input.parse()?;
        let key_ident: Ident = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("LT! takes exactly two arguments"));
        }
        Ok((layer, key_ident))
    };

    let (layer, key_ident) = match syn::parse::Parser::parse(parser, input) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let layer_val: u64 = match layer.base10_parse() {
        Ok(v) => v,
        Err(e) => {
            return Error::new(layer.span(), format!("invalid layer index: {e}"))
                .to_compile_error()
                .into()
        }
    };
    let Ok(layer_u8) = u8::try_from(layer_val) else {
        return Error::new(layer.span(), "layer index out of u8 range (0\u{2013}255)")
            .to_compile_error()
            .into();
    };
    let layer_lit = Literal::u8_suffixed(layer_u8);

    let tap = match ident_to_keycode(&key_ident.to_string(), key_ident.span()) {
        Ok(ts) => ts,
        Err(e) => return e.to_compile_error().into(),
    };

    quote! {
        ::tecla::action::Action::<()>::LayerTap { layer: #layer_lit, tap: #tap }
    }
    .into()
}

/// Entry-point attribute for keyboard firmware.
///
/// Wraps the annotated `async fn` with `#[embassy_executor::main]`. Board
/// crates that use this attribute do not need to add `embassy-executor` to
/// their own dependencies for the entry-point attribute — `tecla-macros` re-
/// expands it from its own dep tree.
///
/// The annotated function must accept an `embassy_executor::Spawner` parameter
/// and return `()`.
///
/// # Example
///
/// ```ignore
/// #[keyboard_main]
/// async fn main(spawner: embassy_executor::Spawner) {
///     spawner.spawn(matrix_task(/* … */)).unwrap();
///     spawner.spawn(keyboard_task(/* … */)).unwrap();
///     usb_task(/* … */).await;
/// }
/// ```
#[proc_macro_attribute]
pub fn keyboard_main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item: TokenStream2 = item.into();
    quote! {
        #[::embassy_executor::main]
        #item
    }
    .into()
}
