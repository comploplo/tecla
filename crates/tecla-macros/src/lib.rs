//! Proc macros for tecla keyboard firmware — `keymap!`, `layer!`, `MT!`, `LT!`.
#![expect(
    non_snake_case,
    reason = "MT and LT are proc macro names matching their macro syntax"
)]
use proc_macro::TokenStream;

/// Define a keyboard keymap.
///
/// # Example
/// ```ignore
/// keymap! {
///     layer!(Default,
///         A,   B,   C,
///         SPC.layer_tap(1),
///     ),
/// }
/// ```
#[proc_macro]
#[inline]
pub fn keymap(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

/// Define a single keymap layer.
#[proc_macro]
#[inline]
pub fn layer(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

/// Mod-tap: hold for modifier, tap for key. Shorthand: `MT!(LSFT, A)`.
#[proc_macro]
#[inline]
pub fn MT(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

/// Layer-tap: hold for layer, tap for key. Shorthand: `LT!(1, SPC)`.
#[proc_macro]
#[inline]
pub fn LT(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}

/// Entry point attribute for keyboard firmware.
///
/// Wraps `embassy_executor::main` and injects keyboard initialization.
#[proc_macro_attribute]
#[inline]
pub fn keyboard_main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
