/* Linker script for the tests on the RX72M (AP-RX72M-0A) with everything
   in the on-chip RAM (0x00000000-0x0007ffff). The debugger uses work RAM
   at 0x1000 and 0x7fb40, so code starts at 0x4000 and the stack ends
   below 0x7f000. The fixed vector table is in RAM too; start-ram.s points
   EXTB at it. */
ENTRY(_start)
SECTIONS {
  . = 0x00004000;
  .text   : { KEEP(*(.text.start)) *(.text*) *(P) }
  .rodata : { *(.rodata*) *(C) *(C_1) *(C_2) *(W*) }
  .data   : { *(.data*) *(D) *(D_1) *(D_2) }
  .bss    : { *(.bss*) *(B) *(B_1) *(B_2) *(COMMON) }
  .fixedvec ALIGN(128) : { __fixedvec = .; KEEP(*(.fixedvec)) }
  __stack = 0x0007f000;
  __ustack = 0x0007e000;
}
