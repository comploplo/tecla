//! Tecla — embedded Rust keyboard firmware core.
//!
//! Tecla is a `no_std`, async-first keyboard firmware library built on
//! [Embassy](https://embassy.dev) and the `embedded-hal` ecosystem. It
//! provides the full input pipeline from raw GPIO matrix scans through HID
//! report generation, with every stage independently testable on the host.
//!
//! # Architecture
//!
//! ```text
//!  matrix::           keyboard::                 pipeline::
//!  ┌───────────────┐  ┌──────────────────────┐   ┌──────────────┐
//!  │ scanner.rs    │  │ processor.rs         │   │ report.rs    │
//!  │ debounce.rs   │──▶ (+ devices::*)       │──▶│ hid.rs       │
//!  │ split.rs      │  │ steno_playback.rs    │   │              │
//!  └───────────────┘  └──────────────────────┘   └──────────────┘
//!         KeyEvent                                    HidReport
//! ```
//!
//! - **[`matrix`]** — [`MatrixScanner`](matrix::MatrixScanner) and
//!   [`Debouncer`](matrix::debounce::Debouncer): raw key state in,
//!   clean [`KeyEvent`](pipeline::event::KeyEvent)s out. Also
//!   [`split`](matrix::split) for multi-half boards and
//!   [`analog`](matrix::analog) for Hall-effect / continuous-travel scanning.
//! - **[`pipeline`]** — the stage-agnostic types: [`event`](pipeline::event),
//!   [`keymap`](pipeline::keymap) ([`Layer`](pipeline::keymap::Layer),
//!   [`LayerStack`](pipeline::keymap::LayerStack)),
//!   [`modifier`](pipeline::modifier),
//!   [`recognizer`](pipeline::recognizer),
//!   [`report`](pipeline::report) ([`KeyboardReport`](pipeline::report::KeyboardReport),
//!   [`NkroKeyboardReport`](pipeline::report::NkroKeyboardReport),
//!   [`HidReport`](pipeline::report::HidReport)),
//!   [`hid`](pipeline::hid) ([`Reporter`](pipeline::hid::Reporter)) and
//!   [`power`](pipeline::power).
//! - **[`keyboard`]** — [`KeyboardProcessor`](keyboard::KeyboardProcessor);
//!   the synchronous event→report core, no Embassy dependency.
//! - **[`devices`]** — one module per input modality
//!   ([`keyboard`](devices::keyboard) tap-hold and combos,
//!   [`steno`](devices::steno), [`midi`](devices::midi),
//!   [`mouse`](devices::mouse), [`gamepad`](devices::gamepad),
//!   [`encoder`](devices::encoder)).
//! - **[`action`]** — [`Action<Ext>`](action::Action), [`KeyCode`](action::KeyCode),
//!   [`ModifierKey`](action::ModifierKey), [`LayerOp`](action::LayerOp).
//! - **[`task`]** *(feature `embassy`)* — async Embassy tasks and channels
//!   that wire the pipeline into a multi-task topology.
//! - **[`telemetry`]**, **[`storage`]**, **[`transport`]**, **[`rgb`]**,
//!   **[`audio`]**, **[`haptic`]**, **[`accessibility`]** — optional
//!   subsystems a board crate opts into.
//!
//! # Quick start
//!
//! The keymap and the processor are pure logic — no executor, no hardware —
//! so the core of a board crate looks like this and runs anywhere:
//!
//! ```rust
//! use tecla::prelude::*;
//! use tecla_macros::{keymap, layer, LT};
//!
//! // 2 layers × 4 keys. `TRNS` falls through to the layer below.
//! let keymap: LayerStack<2, 4, ()> = keymap! {
//!     layer!(Base, A, B, C, LT!(1, SPC)),
//!     layer!(Fn,   F1, F2, F3, TRNS),
//! };
//!
//! let mut processor: KeyboardProcessor<1, 4, 2, 4> =
//!     KeyboardProcessor::new(keymap, TapHoldConfig::default());
//!
//! let mut reports = Vec::new();
//! processor.process_event(KeyEvent::press(KeyPos::new(0, 0), 0), 0, &mut |r| {
//!     reports.push(r);
//! });
//! assert_eq!(reports.len(), 1);
//! ```
//!
//! A board crate then supplies a [`MatrixScanner`](matrix::MatrixScanner) and a
//! [`Reporter`](pipeline::hid::Reporter) for its hardware and, with the
//! `embassy` feature, hands both to [`task::run`] — see the `examples/`
//! directory of any `tecla-*` board crate for a complete firmware binary.
//!
//! # Feature flags
//!
//! | Flag | Default | Effect |
//! |------|---------|--------|
//! | `autocorrect` | **on** | Trie-based typo correction (`AutocorrectEngine`). |
//! | `autocomplete` | **on** | Trie-based prefix completion (`AutocompleteEngine`). |
//! | `std` | off | Enables `std`-dependent code paths (used in host tests). |
//! | `testing` | off | Alias for `std`, used by the coverage and integration-test targets. |
//! | `embassy` | off | Enables the [`task`] module and the async scanner / reporter traits. |
//! | `storage` | off | Persistent keymap and setting storage on internal flash. |
//! | `via` | off | Via protocol command decoding (implies `storage`). |
#![cfg_attr(not(feature = "std"), no_std)]

pub mod matrix;
pub mod music;
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
        Action, ConsumerCode, DksKeys, GamepadAction, KeyCode, KeyOverride, LayerOp, MidiAction,
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

    pub use crate::matrix::split::{SplitConfig, TileId};
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
