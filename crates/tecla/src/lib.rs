//! Tecla — embedded Rust keyboard firmware core.
//!
//! Tecla is a `no_std`, async-first keyboard firmware library built on
//! [Embassy](https://embassy.dev) and the `embedded-hal` ecosystem. It
//! provides the full input pipeline from raw GPIO matrix scans through HID
//! report generation, with every stage independently testable on the host.
//!
//! # Architecture
//!
//! The pipeline has three stages, each in its own module:
//!
//! ```text
//! ┌──────────────┐     KeyEvent      ┌───────────────────┐     HidReport    ┌────────────┐
//! │  matrix.rs   │ ───────────────▶  │  keyboard.rs      │ ───────────────▶ │  hid.rs    │
//! │  debounce.rs │                   │  keymap.rs        │                  │  report.rs │
//! └──────────────┘                   │  tapping.rs  ...  │                  └────────────┘
//!                                    └───────────────────┘
//! ```
//!
//! - **[`matrix`]** — [`MatrixScanner`](matrix::MatrixScanner) /
//!   `AsyncMatrixScanner` traits; raw key state.
//! - **[`debounce`]** — [`Debouncer`](debounce::Debouncer) implementations
//!   that convert noisy GPIO transitions into clean [`KeyEvent`](event::KeyEvent)s.
//! - **[`keymap`]** — [`Layer`](keymap::Layer) and [`LayerStack`](keymap::LayerStack);
//!   multi-layer action resolution with transparent fallthrough.
//! - **[`tapping`]** — [`TapHoldRecognizer`](tapping::TapHoldRecognizer);
//!   mod-tap and layer-tap disambiguation.
//! - **[`recognizer`]** — The [`Recognizer`](recognizer::Recognizer) trait shared
//!   by all input-pattern modules (combos, leader, tap-dance, steno).
//! - **[`keyboard`]** — [`KeyboardProcessor`](keyboard::KeyboardProcessor);
//!   the synchronous event→report core, no Embassy dependency.
//! - **`task`** *(feature `embassy`)* — async Embassy tasks and channels
//!   that wire the pipeline into a three-task topology.
//! - **[`hid`]** — [`Reporter`](hid::Reporter) / `AsyncReporter`
//!   output boundary traits.
//! - **[`report`]** — [`KeyboardReport`](report::KeyboardReport) (6KRO),
//!   [`NkroKeyboardReport`](report::NkroKeyboardReport) (128-key bitmap),
//!   [`ConsumerReport`](report::ConsumerReport), [`HidReport`](report::HidReport).
//! - **[`action`]** — [`Action<Ext>`](action::Action), [`KeyCode`](action::KeyCode),
//!   [`ModifierKey`](action::ModifierKey), [`LayerOp`](action::LayerOp).
//! - **[`modifier`]** — [`ModifierState`](modifier::ModifierState); real, weak,
//!   and one-shot modifier stacks.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use tecla::prelude::*;
//! use tecla_macros::keymap;
//!
//! static KEYMAP: LayerStack<2, 48, ()> = keymap! {
//!     layer!(Base, A, B, C, ..., LT!(Fn, SPC)),
//!     layer!(Fn,  F1, F2, F3, ..., TRNS),
//! };
//!
//! #[keyboard_main]
//! async fn main(spawner: embassy_executor::Spawner) {
//!     let (event_ch, report_ch) = /* static channels */;
//!     spawner.spawn(matrix_task(scanner, debouncer, event_ch.sender())).unwrap();
//!     spawner.spawn(keyboard_task(event_ch.receiver(), processor, report_ch.sender())).unwrap();
//!     usb_task(report_ch.receiver(), reporter).await;
//! }
//! ```
//!
//! # Feature flags
//!
//! | Flag | Default | Effect |
//! |------|---------|--------|
//! | `std` | off | Enables `std`-dependent code paths (used in host tests). |
//! | `embassy` | off | Enables the `task` module and `AsyncReporter` / `AsyncMatrixScanner` re-exports. |
#![cfg_attr(not(feature = "std"), no_std)]

pub mod matrix;
pub mod pipeline;
pub mod storage;
pub mod telemetry;
pub mod transport;

pub mod devices;

pub mod accessibility;
pub mod action;
pub mod audio;
#[cfg(feature = "autocomplete")]
pub mod autocomplete;
#[cfg(feature = "autocorrect")]
pub mod autocorrect;
pub mod haptic;
pub mod keyboard;
pub mod rgb;

#[cfg(feature = "embassy")]
pub mod task;

pub mod prelude {
    //! Convenience re-exports — `use tecla::prelude::*` gives you everything a
    //! board crate needs.

    pub use crate::action::{
        Action, ConsumerCode, GamepadAction, KeyCode, KeyOverride, LayerOp, MidiAction,
        ModifierKey, MouseAction, TriLayerDef,
    };
    pub use crate::audio::{AudioCommand, AudioController, Tone};
    #[cfg(feature = "autocomplete")]
    pub use crate::autocomplete::AutocompleteEngine;
    #[cfg(feature = "autocorrect")]
    pub use crate::autocorrect::{AutocorrectEngine, Correction};
    pub use crate::devices::encoder::EncoderMapping;
    pub use crate::devices::keyboard::tapping::{
        TapHoldConfig, TapHoldDecision, TapHoldMode, TapHoldRecognizer,
    };
    pub use crate::haptic::{HapticCommand, HapticController, HapticEffect};
    pub use crate::keyboard::{KeyboardProcessor, RolloverMode};
    pub use crate::matrix::debounce::Debouncer;
    pub use crate::matrix::MatrixScanner;
    pub use crate::pipeline::event::{
        EncoderDirection, EncoderEvent, KeyEvent, KeyPos, PointingEvent,
    };
    pub use crate::pipeline::hid::{OutputMode, Reporter};
    pub use crate::pipeline::keymap::{Layer, LayerStack};
    pub use crate::pipeline::modifier::ModifierState;
    pub use crate::pipeline::power::{BatteryLevel, WakeReason};
    pub use crate::pipeline::recognizer::{Recognition, Recognizer};
    pub use crate::pipeline::report::HidReport;
    pub use crate::rgb::{Animation, Hsv, Rgb, RgbCommand, RgbController};
    pub use crate::transport::ble::{BleHostManager, BondInfo};

    pub use crate::matrix::split::{SplitConfig, SplitRole};
    pub use crate::transport::SplitTransport;

    #[cfg(feature = "embassy")]
    pub use crate::matrix::AsyncMatrixScanner;
    #[cfg(feature = "embassy")]
    pub use crate::pipeline::hid::{AsyncReporter, DualReporter};
    #[cfg(feature = "embassy")]
    pub use crate::task::{EventChannel, ReportChannel, TelemetryChannel};
    #[cfg(feature = "embassy")]
    pub use crate::transport::ble::BleCommandChannel;
    #[cfg(feature = "embassy")]
    pub use crate::transport::uart::UartTransport;
}
