//! Voyager chromatic four-finger audition profile.
#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "Embassy main and interrupt binding")]
#![expect(clippy::unwrap_used, reason = "infallible firmware task spawning")]

#[path = "../../../keyboards/tecla/voyager_tonnetz/expression.rs"]
mod expression;
#[path = "../../../keyboards/tecla/voyager_tonnetz/firmware.rs"]
mod firmware;
#[path = "../../../keyboards/tecla/voyager_tonnetz/instrument.rs"]
mod instrument;
#[path = "../../../keyboards/tecla/voyager_tonnetz/scan_guard.rs"]
mod scan_guard;

#[tecla_macros::keyboard_main]
async fn main(spawner: embassy_executor::Spawner) {
    firmware::run(
        spawner,
        instrument::Orientation::FourFinger,
        "Voyager Four Finger",
        "TECLA-FOUR-FINGER",
    )
    .await;
}
