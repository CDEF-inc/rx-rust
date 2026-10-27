/* memory.x for rx-rt applications on the RX72M (AP-RX72M-0A), booted from
   the code flash (4 MB ending at 0xFFFFFFFF). RAM starts at 0x4000 and ends
   at 0x7F000 (the stack top), clear of the debugger's work RAM at 0x1000
   and 0x7FB40-0x7FFFF. scripts/hw-build.sh links the rx-rt tests with it
   through RX_MEMORY_X. */
MEMORY
{
  RAM (rwx) : ORIGIN = 0x00004000, LENGTH = 0x7B000
  ROM (rx)  : ORIGIN = 0xFFC00000, LENGTH = 4M
}
