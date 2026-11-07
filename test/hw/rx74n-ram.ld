/* Linker script for the tests on the RX74N (EK-RX74N) with everything in the
   on-chip RAM, at its secure addresses (0x82000000-0x8217ffff): a reset
   leaves the RAM secure, so the debugger cannot reach it at 0x02000000. The
   fixed vector table is in RAM too; start-ram.s points EXTB at it. */
ENTRY(_start)
SECTIONS {
  . = 0x82000000;
  .text   : { KEEP(*(.text.start)) *(.text*) *(P) }
  .rodata : { *(.rodata*) *(C) *(C_1) *(C_2) *(W*) }
  .data   : { *(.data*) *(D) *(D_1) *(D_2) }
  .bss    : { *(.bss*) *(B) *(B_1) *(B_2) *(COMMON) }
  .fixedvec ALIGN(128) : { __fixedvec = .; KEEP(*(.fixedvec)) }
  __stack = 0x82170000;
  __ustack = 0x8216f000;
}
