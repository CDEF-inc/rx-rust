; FCMP sets Z, S and O from the comparison and leaves C alone, whatever the
; flags were before it in the same translation block.
; PSW bits: C = 0, Z = 1, S = 2, O = 3.

; FCMP_CASE prefix, src, src2, want: run a flag-setting prefix, compare
; src2 with src, and check PSW[3:0].
	.macro	FCMP_CASE prefix, src, src2, want
	.ifc	\prefix, neg
	mov.l	#0x7fffffff, r3		; add -> 0x80000000: Z=0 S=1 O=1 C=0
	.else
	mov.l	#0xffffffff, r3		; add -> 0: Z=1 S=0 O=0 C=1
	.endif
	add	#1, r3
	mov.l	#\src, r4
	mov.l	#\src2, r5
	fcmp	r4, r5
	mvfc	psw, r1
	and	#15, r1
	mov.l	#\want, r2
	bsr.a	_check
	.endm

	.text
	.global	_main
_main:
	; prefix "neg" leaves C = 0, prefix "zero" leaves C = 1.
	; 0x3f800000 = 1.0, 0x40000000 = 2.0, 0x7fc00000 = QNaN
	FCMP_CASE neg,  0x3f800000, 0x3f800000, 0x2	; equal: Z
	FCMP_CASE neg,  0x40000000, 0x3f800000, 0x4	; src2 < src: S
	FCMP_CASE neg,  0x3f800000, 0x40000000, 0x0	; src2 > src
	FCMP_CASE neg,  0x7fc00000, 0x3f800000, 0x8	; unordered: O
	FCMP_CASE zero, 0x3f800000, 0x3f800000, 0x3	; equal: Z, C kept
	FCMP_CASE zero, 0x40000000, 0x3f800000, 0x5	; src2 < src: S, C kept
	FCMP_CASE zero, 0x3f800000, 0x40000000, 0x1	; src2 > src: C kept
	FCMP_CASE zero, 0x3f800000, 0x7fc00000, 0x9	; unordered: O, C kept

	; The sequence that first showed the problem: bne after fcmp of 0, 0.
	mov.l	#1, r3
	add	#1, r3
	mov.l	#0, r4
	mov.l	#0, r5
	fcmp	r4, r5
	mov.l	#0, r1
	bne.b	1f
	mov.l	#1, r1
1:	mov.l	#1, r2
	bsr.a	_check

	mov.l	#0, r1
	rts
