/* coretests on the RX74N (EK-RX74N / EK-RX74M), loaded by the debugger
   (scripts/hw/run.py): the code in the 1 MB code MRAM, everything else,
   and the start-up code, in the 1.5 MB on-chip RAM at its secure
   addresses. run.py starts the program at _start (in RAM, so it is not
   booted from the reset vector). The heap is between .bss and the
   stack's 256 KB. Symbols the Rust code names get a leading _ (C symbols
   on RX). */
ENTRY(_start)
MEMORY {
  ROM (rx) : ORIGIN = 0xfff00000, LENGTH = 0xfff80
  RAM (rwx) : ORIGIN = 0x82000000, LENGTH = 0x170000
}
SECTIONS {
  .text.start : { KEEP(*(.text.start)) } > RAM
  .text   : { *(.text*) } > ROM
  .rodata : { *(.rodata*) } > RAM
  .data   : { *(.data*) } > RAM
  .bss (NOLOAD) : { __bss_start = .; *(.bss*) *(COMMON) } > RAM
  __bss_size = SIZEOF(.bss);
  __stack = 0x82170000;
  ___stack = __stack;
  _coretests_heap_start = ALIGN(4096);
  _coretests_heap_end = __stack - 0x40000;
  ASSERT(_coretests_heap_end - _coretests_heap_start >= 0x80000,
         "less than 512 KB of heap: the tests need about 470 KB")
}
