/* RX62N (R5F562N8) as modelled by QEMU's gdbsim-r5f562n8: 96 KB on-chip
   RAM at 0 and 512 KB code flash ending at 0xFFFFFFFF. RAM starts at 4 so
   no object sits at the null address. */
MEMORY
{
  RAM (rwx) : ORIGIN = 0x00000004, LENGTH = 96K - 4
  ROM (rx)  : ORIGIN = 0xFFF80000, LENGTH = 512K
}
