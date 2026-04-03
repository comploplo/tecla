# tecla

A keyboard firmware core for embedded Rust.

`no_std` by default. Async-first via Embassy. MCU-agnostic via `embedded-hal`.

**Targets:** RP2040, STM32, nRF52840.

---

## What it is

Tecla is a clean-room reimplementation of keyboard firmware in Rust. It provides a fully tested synchronous core — matrix scanning, debounce, layer resolution, tap-hold, combos, tap-dance, leader sequences, and steno chord recognition — wired to an Embassy async runtime for real hardware.

## Status

| Phase | What | Status |
|---|---|---|
| 1 | Core types — `KeyPos`, `KeyEvent`, `KeyCode`, `Action<Ext>` | ✅ |
| 2 | Matrix scanning — `MatrixState`, `MatrixScanner` | ✅ |
| 3 | Debounce — `SymDefer`, `AsymEagerDefer` | ✅ |
| 4 | Layer system — `LayerStack`, press-time cache | ✅ |
| 5 | Tap-hold — `TapHoldRecognizer` (Default, HOKP, PermissiveHold) | ✅ |
| 6 | Extended recognizers — combo, tap-dance, leader, steno | ✅ |
| 7 | Async orchestrator — `tecla::run()`, Embassy task wiring | 🔜 |
| 8 | Extended output — MIDI, gamepad | 🔜 |
| 9 | Board support — RP2040, STM32, nRF52840 | 🔜 |
| 10 | QMK config parsing — `info.json`, `keymap.c` | 🔜 |
| 11 | Flash QMK keyboards — `tecla flash --keyboard <id>` | 🔜 |

## Design

The core is pure synchronous logic. All timing is injected as `now_ms: u64` — no global clock, fully deterministic, trivially testable on the host.

Every input behaviour (tap-hold, combos, tap-dance, leader sequences, steno) implements a single `Recognizer` trait. The orchestrator runs all active recognizers on the same event stream.

```rust
pub trait Recognizer {
    type Output;
    fn observe(&mut self, event: KeyEvent, now_ms: u64) -> Recognition<Self::Output>;
    fn tick(&mut self, now_ms: u64) -> Recognition<Self::Output>;
    fn next_deadline_ms(&self) -> Option<u64>;
    fn reset(&mut self);
}
```

## Acknowledgements

Inspired by [QMK Firmware](https://github.com/qmk/qmk_firmware) — the keyboard firmware that started it all.

Built on [Embassy](https://github.com/embassy-rs/embassy) — async embedded Rust done right.

Hat tip to [rmk](https://github.com/HaoboGu/rmk) — a parallel effort in the same space worth knowing about.
