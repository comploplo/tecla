//! Action types facade.
//!
//! Implementation lives in `types`; this module keeps the public surface
//! shallow and re-exports the action types.

mod consumer;
mod keycode;
mod types;

pub use consumer::ConsumerCode;
pub use keycode::KeyCode;
pub use types::{
    Action, DksKeys, GamepadAction, KeyOverride, LayerOp, MidiAction, ModifierKey, MouseAction,
    TriLayerDef,
};
