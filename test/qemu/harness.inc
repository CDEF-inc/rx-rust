; Macros for the QEMU instruction tests (.include "harness.inc").
; harness.s has _check and the exception handlers.

; TRY insn...: run insn with r13 = 0 and continue after it even if it
; traps (r13 is then 1 for the undefined instruction exception, 2 for the
; floating-point exception). Needs r11 = #_resume; clobbers r12.
	.macro	TRY insn:vararg
	mov.l	#0, r13
	mov.l	#9999f, r12
	mov.l	r12, [r11]
	\insn
9999:
	.endm

; EXPECT reg, want: check a register against a value.
	.macro	EXPECT reg, want
	mov.l	\reg, r1
	mov.l	#\want, r2
	bsr.a	_check
	.endm

; EXPECT_FPSW want: check FPSW against a value.
	.macro	EXPECT_FPSW want
	mvfc	fpsw, r1
	mov.l	#\want, r2
	bsr.a	_check
	.endm

; TRAPS want, insn...: run insn and check how it trapped (0: it did not).
	.macro	TRAPS want, insn:vararg
	TRY	\insn
	EXPECT	r13, \want
	.endm
