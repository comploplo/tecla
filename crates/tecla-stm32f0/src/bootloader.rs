//! STM32F072 ROM DFU entry without changing flash or emulated EEPROM.
//!
//! The NOS75 stock image writes `0xDEADBEEF` to `0x20003FFC` at
//! `0x080097D4`, requests SYSRESETREQ at `0x08009808`, and consumes the
//! marker before RAM initialization at `0x08009844`. Its ROM vectors are
//! at `0x1FFFC800`. See the NOS75 research `nos75-storage.md` and the
//! acquired image SHA256
//! `4da91e79d3dda0ed0432c23f58e38ed1d5da196bde1f837a03b3db9d7e9eb222`.
//!
//! ST AN2606, section "STM32F071xx/072xx devices", confirms the ROM base,
//! HSI48 USB clock and PA11/PA12 USB pins:
//! <https://www.st.com/resource/en/application_note/an2606-stm32-microcontroller-system-memory-boot-mode-stmicroelectronics.pdf>
//! RM0091 section 9.1.1 documents the additional explicit system-memory
//! remap used here because Cortex-M0 has no VTOR:
//! <https://www.st.com/resource/en/reference_manual/DM00031936-.pdf>

// Only this module accesses fixed hardware addresses and transfers CPU control.
#![allow(unsafe_code)]

use core::ptr::{read_volatile, write_volatile};

const DFU_MARKER: *mut u32 = 0x2000_3ffc as *mut u32;
const DFU_MAGIC: u32 = 0xdead_beef;
const ROM_VECTORS: *const u32 = 0x1fff_c800 as *const u32;

/// Reset into ROM DFU through the early startup hook.
///
/// The application must call [`maybe_enter_rom_dfu`] from the `__pre_init` assembly hook
/// and reserve the upper eight bytes of its 16 KiB RAM (usable RAM ends
/// at `0x20003FF8`). Reserving eight bytes keeps the stack aligned and
/// prevents the hook's own stack frame from overwriting the marker.
/// This writes one RAM word and requests a system reset; it never erases
/// EEPROM, changes option bytes, or writes application flash.
pub fn request_dfu() -> ! {
    cortex_m::interrupt::disable();
    // SAFETY: This fixed RAM word is excluded from the application's linker RAM.
    unsafe { write_volatile(DFU_MARKER, DFU_MAGIC) };
    cortex_m::asm::dsb();
    cortex_m::peripheral::SCB::sys_reset()
}

/// Consume a reset-persistent DFU request before initializing application RAM.
///
/// Returns normally when there is no request. A request enters the ROM and
/// does not return. The marker is cleared before the transfer.
///
/// # Safety
///
/// Call only in privileged thread mode from Cortex-M-RT the `__pre_init` assembly hook,
/// immediately after reset and before clocks, peripherals, RAM or an
/// executor are initialized. The chip must be an STM32F072 with 16 KiB RAM,
/// and the linker must exclude `0x20003FF8..0x20004000` from ordinary RAM.
/// The hook deliberately uses only constants, CPU instructions and volatile
/// accesses to fixed addresses; it does not access uninitialized Rust statics.
pub unsafe fn maybe_enter_rom_dfu() {
    // SAFETY: The caller guarantees the chip, reserved RAM and early reset state.
    unsafe {
        if read_volatile(DFU_MARKER) != DFU_MAGIC {
            return;
        }
        write_volatile(DFU_MARKER, 0);
        cortex_m::interrupt::disable();

        // After SYSRESETREQ, peripheral clocks and PLLs already have their reset
        // state. Clear the Cortex-M0's one NVIC register bank and SysTick anyway.
        write_volatile(0xe000_e010 as *mut u32, 0); // SysTick CTRL
        write_volatile(0xe000_e014 as *mut u32, 0); // SysTick LOAD
        write_volatile(0xe000_e018 as *mut u32, 0); // SysTick VAL
        write_volatile(0xe000_e180 as *mut u32, u32::MAX); // NVIC ICER
        write_volatile(0xe000_e280 as *mut u32, u32::MAX); // NVIC ICPR
        write_volatile(0xe000_ed04 as *mut u32, (1 << 25) | (1 << 27)); // Clear SysTick/PendSV

        // Ensure exceptions use the ROM vectors at address zero. This is an
        // explicit remap in addition to the stock image's reset-and-jump path.
        let apb2enr = 0x4002_1018 as *mut u32;
        write_volatile(apb2enr, read_volatile(apb2enr) | 1); // SYSCFGEN
        let _ = read_volatile(apb2enr); // Let the peripheral clock take effect.
        let cfgr1 = 0x4001_0000 as *mut u32;
        write_volatile(cfgr1, (read_volatile(cfgr1) & !3) | 1); // MEM_MODE=system flash
        cortex_m::asm::dsb();
        cortex_m::asm::isb();

        cortex_m::register::control::write(cortex_m::register::control::Control::from_bits(0));
        // USB ROM DFU requires interrupts enabled. Every application source is
        // disabled/pending-cleared above. bootload performs the final MSP switch
        // and branch together; there is no Rust stack access after that switch.
        cortex_m::interrupt::enable();
        cortex_m::asm::bootload(ROM_VECTORS)
    }
}
