/* Linker script for the tests in the RX72M code flash (AP-RX72M-0A,
   R5F572MN: 4 MB at 0xffc00000). Variables live in RAM from 0x4000
   (start-rom.s copies .data and clears .bss); the fixed vector table is at
   0xffffff80. Nothing goes to the option-setting memory. */
ENTRY(_start)
MEMORY {
  ROM (rx) : ORIGIN = 0xffc00000, LENGTH = 0x3fff80
  RAM (rw) : ORIGIN = 0x00004000, LENGTH = 0x7b000
}
SECTIONS {
  .text   : { KEEP(*(.text.start)) *(.text*) *(P) } > ROM
  .rodata : { *(.rodata*) *(C) *(C_1) *(C_2) *(W*) } > ROM
  .data   : { __data_start = .; *(.data*) *(D) *(D_1) *(D_2) } > RAM AT > ROM
  __data_load = LOADADDR(.data);
  __data_size = SIZEOF(.data);
  .bss (NOLOAD) : { __bss_start = .; *(.bss*) *(B) *(B_1) *(B_2) *(COMMON) } > RAM
  __bss_size = SIZEOF(.bss);
  __stack = 0x0007f000;
  .fixedvec 0xffffff80 : { KEEP(*(.fixedvec)) }
}
