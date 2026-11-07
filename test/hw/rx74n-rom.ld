/* Linker script for the tests in the code MRAM of the RX74N (EK-RX74N,
   R5K574NC: 1 MB at 0xfff00000). Variables live in the on-chip RAM at its
   secure addresses (start-rom.s copies .data and clears .bss); the fixed
   vector table is at 0xffffff80, a region of its own (lld would otherwise
   put it in ROM, whose attributes match, and find it outside). Nothing
   goes to the option-setting memory. */
ENTRY(_start)
MEMORY {
  ROM (rx) : ORIGIN = 0xfff00000, LENGTH = 0xfff80
  RAM (rw) : ORIGIN = 0x82000000, LENGTH = 0x170000
  FIXEDVEC (r) : ORIGIN = 0xffffff80, LENGTH = 0x80
}
SECTIONS {
  .text   : { KEEP(*(.text.start)) *(.text*) *(P) } > ROM
  .rodata : { *(.rodata*) *(C) *(C_1) *(C_2) *(W*) } > ROM
  .data   : { __data_start = .; *(.data*) *(D) *(D_1) *(D_2) } > RAM AT > ROM
  __data_load = LOADADDR(.data);
  __data_size = SIZEOF(.data);
  .bss (NOLOAD) : { __bss_start = .; *(.bss*) *(B) *(B_1) *(B_2) *(COMMON) } > RAM
  __bss_size = SIZEOF(.bss);
  __stack = 0x82170000;
  .fixedvec : { KEEP(*(.fixedvec)) } > FIXEDVEC
}
