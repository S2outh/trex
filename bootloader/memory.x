MEMORY
{
  /* STM32H723VG */ 
  /* BANK 1: eight 128 KiB erase sectors. */
  FLASH              : ORIGIN = 0x08000000, LENGTH = 128K
  BOOTLOADER_STATE   : ORIGIN = 0x08020000, LENGTH = 128K
  ACTIVE             : ORIGIN = 0x08040000, LENGTH = 256K
  DFU                : ORIGIN = 0x08080000, LENGTH = 384K
  /* AXISRAM: 320 KiB */
  RAM          (rwx) : ORIGIN = 0x24000000, LENGTH = 320K
}

/* Embassy flash partitions are offsets from the STM32 bank-1 base. */
__bootloader_state_start  = ORIGIN(BOOTLOADER_STATE) - ORIGIN(FLASH);
__bootloader_state_end    = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - ORIGIN(FLASH);

__bootloader_active_start = ORIGIN(ACTIVE) - ORIGIN(FLASH);
__bootloader_active_end   = ORIGIN(ACTIVE) + LENGTH(ACTIVE) - ORIGIN(FLASH);

__bootloader_dfu_start    = ORIGIN(DFU) - ORIGIN(FLASH);
__bootloader_dfu_end      = ORIGIN(DFU) + LENGTH(DFU) - ORIGIN(FLASH);
