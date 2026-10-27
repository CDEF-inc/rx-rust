; test/qemu/harness.s for the RX72M: the same _check and exception entries,
; but every failure is also logged in _faillog (up to 64 entries of check
; index, got, want and the address of the bsr.a _check that failed), which
; scripts/hw/run.py writes to NAME.fail. Also holds the fixed vector table
; that start-ram.s points EXTB at.
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
	bne.b	2f
	mov.l	#_checks, r14
	mov.l	[r14], r15
	sub	#1, r15
	mov.l	#_first_failure, r14
	mov.l	r15, [r14]
	mov.l	#_got_value, r14
	mov.l	r1, [r14]
	mov.l	#_want_value, r14
	mov.l	r2, [r14]
2:	mov.l	#_failures, r14
	mov.l	[r14], r15
	cmp	#64, r15
	bgt.b	1f
	sub	#1, r15
	shll	#4, r15
	add	#_faillog, r15
	mov.l	#_checks, r14
	mov.l	[r14], r14
	sub	#1, r14
	mov.l	r14, [r15]
	mov.l	r1, 4[r15]
	mov.l	r2, 8[r15]
	mov.l	[r0], r14		; return address
	sub	#4, r14			; the bsr.a
	mov.l	r14, 12[r15]
1:	rts

_privilege:
	mov.l	#4, r13
	bra.a	_resume_at
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
	mov.l	#_to_supervisor, r14
	mov.l	[r14], r14
	cmp	#0, r14
	beq.b	3f
	mov.l	#0, r13
	mov.l	#_to_supervisor, r14
	mov.l	r13, [r14]
	mov.l	4[r0], r14		; the stacked PSW: PM = 0, U = 0
	and	#~0x00120000, r14
	mov.l	r14, 4[r0]
3:	rte

; Fixed vector table: in RAM for start-ram.s, at 0xffffff80 for start-rom.s.
	.section .fixedvec, "a"
	.space	20 * 4
	.long	_privilege		; 20: privileged instruction
	.space	2 * 4
	.long	_undefined		; 23: undefined instruction
	.long	0
	.long	_fpu_exception		; 25: floating-point exception
	.space	5 * 4
	.long	_start			; 31: reset

	.data
	.global	_checks, _failures, _first_failure, _got_value, _want_value
	.global	_resume, _faillog, _to_supervisor, _ustack
	.balign	4
_checks:	.long	0
_failures:	.long	0
_first_failure:	.long	-1
_got_value:	.long	0
_want_value:	.long	0
_resume:	.long	0
_to_supervisor:	.long	0
	.bss
	.balign	4
_faillog:	.space	64 * 16
	.space	256
_ustack:
