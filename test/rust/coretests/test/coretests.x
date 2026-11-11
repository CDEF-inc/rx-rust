/* coretests on QEMU's gdbsim-r5f562n8: everything in the 16 MB SDRAM at
   0x01000000, loaded as an ELF; the stack at its top. */
ENTRY(_start)
SECTIONS {
  . = 0x01000000;
  .text   : { KEEP(*(.text.start)) *(.text*) }
  .rodata : { *(.rodata*) }
  .data   : { *(.data*) }
  .bss    : { *(.bss*) *(COMMON) }
  __stack = 0x01fffff0;
  /* The same, as Rust names it (C symbols get a leading _ on RX). */
  ___stack = __stack;
}
