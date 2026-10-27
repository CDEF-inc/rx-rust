; Macros for the QEMU instruction tests (.include "harness.inc").
; harness.s has _check and the exception handlers.

; TRY insn...: run insn with r13 = 0 and continue after it even if it
; traps (r13 is then 1 for the undefined instruction exception, 2 for the
; floating-point exception, 4 for the privileged instruction exception).
; Needs r11 = #_resume; clobbers r12.
	.macro	TRY insn:vararg
	mov.l	#0, r13
	mov.l	#9999f, r12
	mov.l	r12, [r11]
	\insn
9999:
	.endm

; USER_MODE: switch to user mode with RTE (PSW.PM = 1). The switch sets
; PSW.U, so the stack becomes the USP; it is pointed at the harness's user
; stack first. Clobbers r4.
	.macro	USER_MODE
	mvtc	#_ustack, usp
	mvfc	psw, r4
	or	#0x00100000, r4
	push.l	r4
	mov.l	#9998f, r4
	push.l	r4
	rte
9998:
	.endm

; SUPERVISOR_MODE: back from user mode to supervisor mode (PSW.PM = 0,
; PSW.U = 0) through the privileged instruction exception of an RTE. Needs
; r11 = #_resume; clobbers r12 and r13.
	.macro	SUPERVISOR_MODE
	mov.l	#_to_supervisor, r13
	mov.l	#1, r12
	mov.l	r12, [r13]
	TRY	rte
	.endm

; EXPECT reg, want: check a register against a value.
	.macro	EXPECT reg, want
	mov.l	\reg, r1
	mov.l	#\want, r2
	bsr.a	_check
	.endm

; EXPECT_REG reg, wreg: check a register against another one.
	.macro	EXPECT_REG reg, wreg
	mov.l	\reg, r1
	mov.l	\wreg, r2
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
