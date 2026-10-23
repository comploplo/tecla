//! Voyager chromatic accordion audition profile.
#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "Embassy main and interrupt binding")]
#![expect(clippy::unwrap_used, reason = "infallible firmware task spawning")]

#[path = "../profiles/voyager_tonnetz/expression.rs"]
mod expression;
#[path = "../profiles/voyager_tonnetz/firmware.rs"]
mod firmware;
#[path = "../profiles/voyager_tonnetz/instrument.rs"]
mod instrument;
#[path = "../profiles/voyager_tonnetz/scan_guard.rs"]
mod scan_guard;

#[tecla_macros::keyboard_main]
async fn main(spawner: embassy_executor::Spawner) {
    firmware::run(
        spawner,
        instrument::Orientation::Accordion,
        "Voyager Accordion",
        "TECLA-ACCORDION",
    )
    .await;
}
