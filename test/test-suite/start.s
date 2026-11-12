; Start-up for the llvm-test-suite programs on QEMU (newlib): set SP and
; INTB and go on in C (_rx_start in syscalls.c). QEMU's loader puts .data
; in place and the RAM starts zeroed, so neither is copied or cleared here.
	.section .text.start, "ax"
	.global	_start
_start:
	mov.l	#__stack, r0
	mvtc	#vectors, intb
	bsr.a	__rx_start
1:	bra.b	1b

; CMI0 (vector 28), CMT0's compare match every 65536 counts: the high part
; of the clock (syscalls.c). The interrupt saves PSW, so add may set the
; flags.
	.text
cmi0:
	pushm	r14-r15
	mov.l	#__rx_clock_high, r14
	mov.l	[r14], r15
	add	#1, r15
	mov.l	r15, [r14]
	popm	r14-r15
	rte

; The relocatable vector table: only CMI0. Any other interrupt jumps to 0,
; which stops the program with QEMU's error.
	.section .rodata
	.balign	4
vectors:
	.space	28 * 4
	.long	cmi0
	.space	(256 - 29) * 4
