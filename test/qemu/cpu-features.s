; Instructions of a unit the core does not have raise the undefined
; instruction exception. FPU is 1 when the core has the FPU (set by
; run-qemu-test.sh).

; TRAPS want, insn...: run insn and check whether it trapped.
	.macro	TRAPS want, insn:vararg
	mov.l	#0, r13
	mov.l	#1f, r12
	mov.l	r12, [r11]
	\insn
1:	mov.l	r13, r1
	mov.l	#\want, r2
	bsr.a	_check
	.endm

	.text
	.global	_main
_main:
	mov.l	#_resume, r11
	mov.l	#0x3f800000, r4		; 1.0
	mov.l	#0x40000000, r5		; 2.0
	TRAPS	1 - FPU, fadd r4, r5
	TRAPS	1 - FPU, fsub #0x3f800000, r5
	TRAPS	1 - FPU, fmul [r0], r5
	TRAPS	1 - FPU, fdiv r4, r5
	TRAPS	1 - FPU, fcmp r4, r5
	TRAPS	1 - FPU, ftoi r4, r5
	TRAPS	1 - FPU, round r4, r5
	TRAPS	1 - FPU, itof r4, r5
	; integer instructions never trap
	TRAPS	0, add r4, r5
	TRAPS	0, mvfc fpsw, r5

	mov.l	#0, r1
	rts
