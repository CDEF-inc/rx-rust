; Result recording for the QEMU instruction tests (read by qemu-run.sh).
; _check compares r1 (got) with r2 (want); it preserves r1-r13.
;
; The undefined instruction exception sets r13 to 1, and the floating-point
; exception sets it to 2; both resume at the address in _resume, so a test
; can check whether an instruction traps (TRY in harness.inc).
	.text
	.global	_check, _resume_at
_check:
	mov.l	#_checks, r14
	mov.l	[r14], r15
	add	#1, r15
	mov.l	r15, [r14]
	cmp	r2, r1
	beq.b	1f
	mov.l	#_failures, r14
	mov.l	[r14], r15
	add	#1, r15
	mov.l	r15, [r14]
	cmp	#1, r15
	bne.b	1f
	mov.l	#_checks, r14
	mov.l	[r14], r15
	sub	#1, r15
	mov.l	#_first_failure, r14
	mov.l	r15, [r14]
	mov.l	#_got_value, r14
	mov.l	r1, [r14]
	mov.l	#_want_value, r14
	mov.l	r2, [r14]
1:	rts

_undefined:
	mov.l	#1, r13
	bra.a	_resume_at
_fpu_exception:
	mov.l	#2, r13
; Return from an exception to the address in _resume.
_resume_at:
	mov.l	#_resume, r14
	mov.l	[r14], r14
	mov.l	r14, [r0]		; the stacked PC
	rte

; Fixed vector table (scripts/rx-qemu.ld places it at 0xffffff80).
	.section .fixedvec, "a"
	.space	23 * 4
	.long	_undefined		; 23: undefined instruction
	.long	0
	.long	_fpu_exception		; 25: floating-point exception
	.space	5 * 4
	.long	_start			; 31: reset

	.data
	.global	_checks, _failures, _first_failure, _got_value, _want_value
	.global	_resume
_checks:	.long	0
_failures:	.long	0
_first_failure:	.long	-1
_got_value:	.long	0
_want_value:	.long	0
_resume:	.long	0
