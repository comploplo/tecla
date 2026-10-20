//! Literal left Tonnetz with categorical harmonic gestures and pitch-state LEDs.
#![no_std]
#![no_main]
#![allow(unsafe_code, reason = "Embassy main and interrupt binding")]
#![expect(clippy::unwrap_used, reason = "infallible firmware task spawning")]

#[path = "../../../keyboards/tecla/voyager_tonnetz/expression.rs"]
mod expression;
#[path = "../../../keyboards/tecla/voyager_tonnetz/firmware.rs"]
mod firmware;
#[path = "../../../keyboards/tecla/voyager_harmony/instrument.rs"]
mod harmony_instrument;
#[path = "../../../keyboards/tecla/voyager_tonnetz/instrument.rs"]
#[allow(
    dead_code,
    reason = "shared lattice map also contains the manual Tonnetz instrument"
)]
mod instrument;
#[path = "../../../keyboards/tecla/voyager_tonnetz/scan_guard.rs"]
mod scan_guard;
#[path = "../../../keyboards/tecla/voyager_harmony/telemetry.rs"]
mod telemetry;

impl firmware::Performance for harmony_instrument::HarmonyInstrument {
    fn diagnostics_enabled(&self) -> bool {
        true
    }

    fn diagnostic_frame(
        &self,
        status: firmware::DiagnosticStatus,
        events: &[tecla::pipeline::report::MidiEvent],
        out: &mut heapless::String<2560>,
    ) -> bool {
        telemetry::encode(self.snapshot(), status, events, &self.frame(), out).is_ok()
    }

    fn update_at(
        &mut self,
        state: &tecla::matrix::MatrixState<12, 7>,
        now_ms: u64,
        ready: bool,
        emit: &mut dyn FnMut(tecla::pipeline::report::MidiEvent),
    ) {
        harmony_instrument::HarmonyInstrument::update_at(self, state, now_ms, ready, emit);
    }

    fn frame(&self) -> [tecla::rgb::Rgb; instrument::LEDS] {
        harmony_instrument::HarmonyInstrument::frame(self)
    }
}

#[tecla_macros::keyboard_main]
async fn main(spawner: embassy_executor::Spawner) {
    firmware::run_profile(
        spawner,
        harmony_instrument::HarmonyInstrument::new(),
        "Voyager Harmony",
        "TECLA-HARMONY",
        false,
    )
    .await;
}
