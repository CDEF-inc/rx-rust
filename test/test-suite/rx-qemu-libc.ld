/* Linker script for the llvm-test-suite programs on QEMU (gdbsim-r5f562n8:
   16 MB SDRAM at 0x01000000): everything in the SDRAM; the heap from _end
   to the argument block (64 KB, test/test-suite/syscalls.c), then 256 KB
   of stack.
   C names get a leading underscore here: ___args is `__args` in C. */
ENTRY(_start)
SECTIONS {
  . = 0x01000000;
  .text : { KEEP(*(.text.start)) *(.text .text.*) *(P) }
  .rodata : { *(.rodata .rodata.*) *(C) *(C_1) *(C_2) *(W*) }
  /* For __register_frame_info (syscalls.c), with the zero terminator the
     unwinder's search needs. */
  .eh_frame : { ___eh_frame_start = .; KEEP(*(.eh_frame)) LONG(0) }
  .gcc_except_table : { *(.gcc_except_table .gcc_except_table.*) }
  .init_array : {
    ___init_array_start = .;
    KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)))
    KEEP(*(.init_array))
    ___init_array_end = .;
  }
  .fini_array : {
    ___fini_array_start = .;
    KEEP(*(SORT_BY_INIT_PRIORITY(.fini_array.*)))
    KEEP(*(.fini_array))
    ___fini_array_end = .;
  }
  ___preinit_array_start = .;
  ___preinit_array_end = .;
  .data : { *(.data .data.*) *(D) *(D_1) *(D_2) }
  .bss : { *(.bss .bss.*) *(B) *(B_1) *(B_2) *(COMMON) }
  . = ALIGN(8);
  __end = .;    /* _end in syscalls.c */
  _end = .;     /* end: libnosys's sbrk, if anything pulls it in */
  ___args = 0x01fb0000;
  __stack = 0x01fffff0;
  ASSERT(__end <= ___args, "the program does not leave room for the heap")
}
