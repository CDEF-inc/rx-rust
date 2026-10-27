; Start-up for the tests on the RX72M with the program in the code flash:
; copy .data to RAM, clear .bss, set SP, call main, record its result and
; set `finished`, then spin at _done. The fixed vector table stays at
; 0xffffff80 (EXTB's reset value), so tests that move EXTB run from here.
	.section .text.start, "ax"
	.global	_start
_start:
	mov.l	#__stack, r0
	mov.l	#__data_load, r2
	mov.l	#__data_start, r1
	mov.l	#__data_size, r3
1:	cmp	#0, r3
	beq.b	2f
	mov.b	[r2], r4
	mov.b	r4, [r1]
	add	#1, r2
	add	#1, r1
	sub	#1, r3
	bra.b	1b
2:	mov.l	#__bss_start, r1
	mov.l	#0, r2
	mov.l	#__bss_size, r3
3:	cmp	#0, r3
	beq.b	4f
	mov.b	r2, [r1]
	add	#1, r1
	sub	#1, r3
	bra.b	3b
4:	bsr.a	_main
	mov.l	#_exit_code, r2
	mov.l	r1, [r2]
	mov.l	#_finished, r2
	mov.l	#1, r1
	mov.l	r1, [r2]
	.global	_done
_done:
	bra.b	_done
	.section .bss
	.global	_exit_code, _finished
_exit_code:
	.long	0
_finished:
	.long	0
