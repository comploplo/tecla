/* Confirmed 128 KiB flash / 16 KiB RAM on the acquired NOS75.
 * Keep stock EEPROM snapshot + journal at 0x0801E000..0x08020000.
 * Reserve the top 8 RAM bytes for the stock-compatible ROM DFU marker. */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 120K
  RAM   : ORIGIN = 0x20000000, LENGTH = 16K - 8
}
/* Leave at least 5 KiB for the main/interrupt stack below the DFU marker. */
ASSERT(_stack_start - __sheap >= 5120, "NOS75 static RAM leaves less than 5 KiB stack");
