; Start-up for the tests on the RX72M with the program in RAM, loaded by the
; debugger (scripts/hw-run.sh): point EXTB at the fixed vector table in RAM,
; set SP, call main, record its result and set `finished`, then spin at
; _done (the runner's breakpoint).
	.section .text.start, "ax"
	.global	_start
_start:
	mov.l	#__fixedvec, r1
	mvtc	r1, extb
	mov.l	#__stack, r0
	bsr.a	_main
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
