//! Shared pipeline infrastructure.
//!
//! Types and traits used by all device modalities: events, the recognizer
//! trait, modifier state, the layer system, HID reporter traits, report
//! types, and power management.

pub mod event;
pub mod hid;
pub mod keymap;
pub mod modifier;
pub mod power;
pub mod recognizer;
pub mod report;
