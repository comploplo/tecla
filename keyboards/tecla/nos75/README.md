# NuPhy NOS75

Experimental native Tecla port for the wired NOS75. The initial typing firmware
has been flashed by the user and observed enumerating as Tecla NOS75. The RGB
implementation builds and is checked against the production dump, but physical
lighting and LED current remain untested. ROM recovery still needs a hardware
check.

From the repository root:

```sh
just flash --tecla-config nos75 --dry-run  # build + size check; no device access
just flash --tecla-config nos75            # build + wait for ROM DFU + flash
```

Install `dfu-util` and `arm-none-eabi-objcopy` (or `llvm-objcopy`) for the DFU route.
The Rust target `thumbv6m-none-eabi` is included in `rust-toolchain.toml`.
Enter DFU before flashing: unplug USB, hold physical Esc, reconnect, then release
Esc. Stock firmware's Esc bootmagic **clears its saved settings** on entry; save
VIA settings before leaving stock. Tecla's Esc startup path does not erase them.
The coordinator accepts exactly one matching DFU device. With multiple devices,
select one using `--device serial:<serial>` or `--device path:<usb-path>`;
it forwards the exact selection to `dfu-util`.

Edit [keymap.rs](keymap.rs), then build/flash again. This is a conventional
F1–F12 base with Caps Lock mapped to Left Control, a momentary Fn layer, and
6-key rollover plus modifiers. It works with USB hosts including macOS.
The left modifiers use macOS order: Control, Option, Command, then Space.

| Hold Fn with | Action |
| --- | --- |
| F1 / F2 | Brightness down / up |
| F7 / F8 / F9 | Previous / play-pause / next |
| F10 / F11 / F12 | Mute / volume down / up |
| Delete / Home | Insert / End |
| W | Type the average WPM for active typing since startup |

Fn+W types the active-typing average without Enter or a reset. It uses a small
standalone collector, without enabling the larger processor telemetry
collectors. See [typing statistics](../README.md#typing-statistics) for the
shared definition, exclusions, and host-layout convention.

Lighting starts with a slow rainbow across all 81 key LEDs and eight side LEDs.
Cycle through **solid, rainbow, breathing, reactive fade, typing-speed rainbow,
heatmap, and ripple**. Reactive and heatmap modes start dark and respond to typing.

| Hold Fn with | Lighting control |
| --- | --- |
| Left / Right | Previous / next effect |
| Up / Down | Brightness up / down |
| Backslash | Toggle lighting |
| `[` / `]` | Hue down / up |
| `;` / `'` | Saturation down / up |
| Comma / Period | Speed down / up |

Tap a control again for another step. Hue/saturation affect solid, breathing and
reactive fade; rainbow/heatmap/ripple choose their own colors. Speed controls the
time-based effects; reactive fades, heat decay and ripple expansion follow the
fixed frame cadence. Switching effects resets that effect's history. Settings
are held in RAM and return to the defaults after a power cycle.

USB suspend/deconfiguration shuts both LED controllers down; resume reinitializes
them and restores the current mode. Failed shutdown attempts retry with a short
backoff until shutdown succeeds or USB resumes. LED I2C transfers run
asynchronously in small DMA-backed chunks. A bus error stops the frame and
retries initialization without stopping the keyboard task.

Other Fn keys fall through. There is no VIA interface, persistent configuration,
detailed processor telemetry, stock Mac/Windows layer switching, or stock
custom system actions in this port. The stock matrix scan-count filter is
replaced by Tecla's 5 ms symmetric debounce; scan settling uses provisional
10 µs select and 3 µs release delays. These need physical validation.

Holding physical Esc through startup (20 ms settle, then two pressed scans
200 ms apart) requests a software reset and enters the STM32 ROM DFU loader.
This recovery check is independent of the keymap. The early hook consumes the
stock-compatible RAM marker before RAM initialization and maps ROM vectors.
It does not write flash, EEPROM or option bytes. Test this recovery path as part
of first hardware bring-up; it cannot yet be claimed as a proven recovery route.

## Hardware evidence and memory limits

The production readout SHA256 is
`4da91e79d3dda0ed0432c23f58e38ed1d5da196bde1f837a03b3db9d7e9eb222`.
It reports 128 KiB flash and has a 16 KiB RAM/vector layout, consistent with
STM32F072xB. `stm32f072cb` is the build model; the physical package marking has
not been inspected. Board-specific facts come from the production readout,
checked against the earlier Krypton75 QMK PR #21920, rather than assuming the
prototype's USB identity and layout are identical.

| Item | Production value |
| --- | --- |
| Runtime identity | `19f5:3235` (Tecla product string and distinct serial) |
| ROM DFU | `0483:df11`, alternate 0, `0x08000000:leave` |
| Rows, table at `0x0800D3B4` | PC14 PC15 PA0 PA1 PA2 PA3 |
| Columns, table at `0x0800D370` | PA4 PA5 PA6 PA7 PB0 PB1 PB10 PB11 PB12 PB13 PB14 PB15 PA8 PA9 PA10 PA15 PB3 |
| USB | PA11 D− / PA12 D+, internal HSI48 + USB clock recovery |
| Lighting | Two IS31FL3733-compatible devices at `0x50`/`0x53`, I2C1 PB6/PB7 |
| Board outputs | PC13/PB9 HIGH, matching production startup |
| LED mapping | 89 production entries at `0x0800CA54`, matrix/XY data at `0x0800DB00` |
| Application | `0x08000000..0x0801E000` (120 KiB) |
| Stock EEPROM | `0x0801E000..0x08020000` (8 KiB, preserved) |
| RAM | `0x20000000..0x20003FF8`, top 8 bytes reserved for DFU marker |

Idle rows and all columns are inputs with pull-ups; one selected row becomes
output-low, then returns to input. LSE is disabled because PC14/PC15 are rows.
The linker and flash coordinator both enforce the application ceiling. The
linker also requires at least 5 KiB between static RAM and the initial stack.
The CPU uses internal HSI48 at 48 MHz; I2C timing is computed by the HAL for
400 kHz. The USB configuration declares 500 mA, matching stock. Global LED
current is set to `0x3F`, one quarter of stock `0xFF`, with a separate whole-frame
RGB channel-sum budget equivalent to 350 mA assuming 20 mA/full channel. The
current-setting resistors are unknown, so this is a conservative software
budget, not a measured electrical limit. That budget may cap peak brightness.
Side positions for ripple effects are cosmetic because stock stores zeros there.
The prototype's PB9 WS2812 transport is not used by the production NOS75.

The processor is initialized statically to avoid a large startup stack copy;
USB runs in its own task. Detailed processor telemetry is omitted to fit the
MCU's RAM; the standalone WPM collector remains enabled.
Build this crate separately from other STM32 board crates to avoid chip-feature
unification: `cargo build -p tecla-stm32f0 --example nos75 --release --target thumbv6m-none-eabi`.

Keep the verified original readout and VIA backup outside build output. To
restore stock while retaining the current EEPROM, use only the first 122880
bytes of that verified readout at `0x08000000`, not the whole 128 KiB capture.
A full-flash restore also replaces settings with the capture's older contents.
Do not use a prototype Krypton75 or a different NuPhy model's binary as stock.
