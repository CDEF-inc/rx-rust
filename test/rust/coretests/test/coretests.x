/* coretests on QEMU's gdbsim-r5f562n8: everything in the 16 MB SDRAM at
   0x01000000, loaded as an ELF; the heap after .bss, the stack at the
   top. Symbols the Rust code names get a leading _ (C symbols on RX). */
ENTRY(_start)
SECTIONS {
  . = 0x01000000;
  .text   : { KEEP(*(.text.start)) *(.text*) }
  .rodata : { *(.rodata*) }
  .data   : { *(.data*) }
  .bss    : { __bss_start = .; *(.bss*) *(COMMON) }
  __bss_size = SIZEOF(.bss);
  _coretests_heap_start = ALIGN(4096);
  _coretests_heap_end = 0x01f00000;
  __stack = 0x01fffff0;
  ___stack = __stack;
}
