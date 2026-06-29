# Tecla Native Boards

This directory holds Tecla-native board definitions.

Each subdirectory is a stable, human-readable profile name that maps to a
Tecla-native board definition.

Typical contents:

- `board.toml` — board metadata, target, flash method, and profile name
- `examples/` — board wiring, MCU setup, and runtime glue
- `keymaps/` — the editable keymap / steno layout for that board
- `combos/` — optional combo definitions for that board

Current profiles:

- `planck-rp2040`
- `split-rp2040`
- `steno_uni_v4`
- `planck-nrf52840`
- `planck-nrf52840-ble`
- `planck-stm32f4`
- `voyager`
- `voyager_usbtest` — USB/HID bring-up diagnostic for the Voyager
- `voyager_matrix_test` — raw matrix diagnostic for the Voyager

These files are intentionally separate from the vendored QMK tree under
`keyboards/`.
