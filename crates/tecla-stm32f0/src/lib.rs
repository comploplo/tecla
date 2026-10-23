//! STM32F072 support for the NuPhy NOS75 wired keyboard.
#![no_std]

pub mod bootloader;
pub mod is31fl3733;
pub mod keyboard_control;
pub mod matrix;
mod scan;

pub use scan::{COLS, ROWS};
