/* STM32F303xB (ZSA Voyager): 128K flash, 32K main SRAM, 8K CCM SRAM.
   The ZSA DFU bootloader occupies the first 8K (0x08000000..0x08002000) and
   jumps to the application at 0x08002000 — so the app FLASH origin is offset.
   The app must also point VTOR at 0x08002000 (done in the example's main).

   Do not model CCM as contiguous with main SRAM: the reset stack pointer comes
   from the top of RAM, and placing it at 0x2000a000 can fault before USB starts. */
MEMORY {
    FLASH : ORIGIN = 0x08002000, LENGTH = 120K
    RAM   : ORIGIN = 0x20000000, LENGTH = 32K
}
