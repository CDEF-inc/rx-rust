; RXv2 integer and system additions: movli / movco, stz / stnz with a
; register source, and EXTB (RXv3 manual section 3, 1.2.2.9, 1.7.1).
; Run on RXv2 and later.
	.include "harness.inc"

	.text
	.global	_main
_main:
	mov.l	#_resume, r11
	mov.l	#_word, r7

	; movli / movco: the store happens only after movli, and once
	mov.l	#0x11111111, r4
	mov.l	r4, [r7]
	movli	[r7], r5
	EXPECT	r5, 0x11111111
	mov.l	#0x22222222, r6
	movco	r6, [r7]
	EXPECT	r6, 0			; stored
	mov.l	[r7], r4
	EXPECT	r4, 0x22222222
	mov.l	#0x33333333, r6
	movco	r6, [r7]
	EXPECT	r6, 1			; LI was cleared by the last movco
	mov.l	[r7], r4
	EXPECT	r4, 0x22222222

	; RTE clears LI (int #0 returns with rte)
	mvtc	#_ivt, intb
	movli	[r7], r5
	int	#0
	mov.l	#0x44444444, r6
	movco	r6, [r7]
	EXPECT	r6, 1
	mov.l	[r7], r4
	EXPECT	r4, 0x22222222

	; stz / stnz rs, rd
	mov.l	#5, r4
	mov.l	#7, r5
	mov.l	#9, r6
	cmp	r4, r4			; Z = 1
	stz	r4, r5
	stnz	r4, r6
	EXPECT	r5, 5
	EXPECT	r6, 9
	mov.l	#7, r5
	cmp	#1, r4			; Z = 0
	stz	r4, r5
	stnz	r4, r6
	EXPECT	r5, 7
	EXPECT	r6, 5

	; EXTB resets to ffffff80h and moves the fixed vectors
	mvfc	extb, r5
	EXPECT	r5, 0xffffff80
	mov.l	#_vectors, r4
	mvtc	r4, extb
	mvfc	extb, r5
	EXPECT	r5, _vectors
	pushc	extb
	popc	extb
	mvfc	extb, r5
	EXPECT	r5, _vectors
	mvtc	#0x500, fpsw		; EV, DN
	mov.l	#0x7f800000, r4
	mov.l	r4, r5
	TRY	fsub r4, r5		; invalid: vector 25 of the new table
	EXPECT	r13, 3
	mvtc	#0xffffff80, extb
	mvtc	#0x500, fpsw
	TRY	fsub r4, r5		; back to the table at ffffff80h
	EXPECT	r13, 2
	mvtc	#0x100, fpsw

	mov.l	#0, r1
	rts

_rte_only:
	rte

_fpu_exception_moved:
	mov.l	#3, r13
	bra.a	_resume_at

	.data
	.balign	4
_word:	.long	0
_ivt:	.long	_rte_only		; int #0
_vectors:
	.space	25 * 4
	.long	_fpu_exception_moved	; 25: floating-point exception
	.space	6 * 4
