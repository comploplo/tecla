/* STM32F303xC (ZSA Voyager): 256K flash, 40K main SRAM, 8K CCM SRAM.
   Confirmed via the DFU bootloader's own flash report (124 * 2K pages from
   0x08002000 = 248K app flash, 256K total) and stm32-metapac's chip data —
   this is the xC density line, not xB (128K/32K).
   The ZSA DFU bootloader occupies the first 8K (0x08000000..0x08002000) and
   jumps to the application at 0x08002000 — so the app FLASH origin is offset.
   The app must also point VTOR at 0x08002000 (done in the example's main).

   CCM (0x10000000, 8K) is a separate address range, not contiguous with main
   SRAM — do not extend RAM here to "cover" it. 0x2000a000 (RAM origin + 40K)
   is the real, valid top of main SRAM for the xC line, so the reset stack
   pointer landing there is correct. */
MEMORY {
    FLASH : ORIGIN = 0x08002000, LENGTH = 248K
    RAM   : ORIGIN = 0x20000000, LENGTH = 40K
}
