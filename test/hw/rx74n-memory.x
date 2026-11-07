/* memory.x for rx-rt applications on the RX74N (EK-RX74N), booted from the
   code MRAM (1 MB ending at 0xFFFFFFFF). RAM is the on-chip RAM at its
   secure addresses, which is how a reset leaves it. scripts/hw-build.sh
   links the rx-rt tests with it through RX_MEMORY_X. */
MEMORY
{
  RAM (rwx) : ORIGIN = 0x82000000, LENGTH = 0x170000
  ROM (rx)  : ORIGIN = 0xFFF00000, LENGTH = 1M
}
