/* Linker script for the QEMU tests (gdbsim-r5f562n8: 16 MB SDRAM at
   0x01000000). GNU as names its default sections P/D/B/C (Renesas
   convention); LLVM uses .text/.data/.bss/.rodata. */
ENTRY(_start)
SECTIONS {
  . = 0x01000000;
  .text   : { KEEP(*(.text.start)) *(.text*) *(P) }
  .rodata : { *(.rodata*) *(C) *(C_1) *(C_2) *(W*) }
  .data   : { *(.data*) *(D) *(D_1) *(D_2) }
  .bss    : { *(.bss*) *(B) *(B_1) *(B_2) *(COMMON) }
  __stack = 0x01fffff0;
}
