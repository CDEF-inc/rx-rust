; RXv2 single-precision FPU instructions: three-operand fadd / fsub / fmul,
; fsqrt, ftou and utof (RXv3 manual section 3). Run on RXv2 and later.
	.include "harness.inc"

	.text
	.global	_main
_main:
	mov.l	#_resume, r11
	mvtc	#0x100, fpsw

	; rd = rs2 op rs; the sources are unchanged
	mov.l	#0x3f800000, r4		; 1.0
	mov.l	#0x40400000, r5		; 3.0
	fadd	r4, r5, r6
	EXPECT	r6, 0x40800000		; 4.0
	fsub	r4, r5, r6
	EXPECT	r6, 0x40000000		; 3.0 - 1.0
	fmul	r5, r5, r6
	EXPECT	r6, 0x41100000		; 9.0
	EXPECT	r4, 0x3f800000
	EXPECT	r5, 0x40400000
	clrpsw	z			; the opposite of the expected flags
	setpsw	s
	fsub	r5, r5, r6
	mvfc	psw, r8			; before EXPECT, whose cmp sets the flags
	EXPECT	r6, 0			; +0 (round to nearest)
	and	#6, r8
	EXPECT	r8, 2			; Z = 1, S = 0

	; fsqrt
	mov.l	#0x40800000, r4		; 4.0
	fsqrt	r4, r6
	EXPECT	r6, 0x40000000
	mov.l	#0x40000000, r4		; 2.0
	fsqrt	r4, r6
	EXPECT	r6, 0x3fb504f3
	EXPECT_FPSW 0x40000140		; inexact
	mvtc	#0x100, fpsw
	mov.l	#_nine, r7
	fsqrt	[r7], r6
	EXPECT	r6, 0x40400000
	fsqrt	4[r7], r6		; sqrt(0.25)
	EXPECT	r6, 0x3f000000
	mov.l	#0x80000000, r4		; -0
	fsqrt	r4, r6
	EXPECT	r6, 0x80000000
	mov.l	#0xbf800000, r4		; -1.0
	fsqrt	r4, r6
	EXPECT	r6, 0x7fffffff
	EXPECT_FPSW 0x84000104		; invalid

	; ftou rounds towards 0; out of range and NaN are invalid
	mvtc	#0x100, fpsw
	mov.l	#0x406ccccd, r4		; 3.7
	ftou	r4, r6
	EXPECT	r6, 3
	mov.l	#0x4f7fffff, r4		; 4294967040.0
	ftou	r4, r6
	mvfc	psw, r8			; before EXPECT, whose cmp sets the flags
	EXPECT	r6, 0xffffff00
	and	#6, r8
	EXPECT	r8, 4			; S from bit 31 of the result
	mov.l	#0x4f000000, r4		; 2147483648.0
	ftou	r4, r6
	mvfc	psw, r8
	EXPECT	r6, 0x80000000
	and	#6, r8
	EXPECT	r8, 4			; S, but not Z: Z is for 0 only
	mov.l	#0x3f000000, r4		; 0.5
	ftou	r4, r6
	mvfc	psw, r8
	EXPECT	r6, 0
	and	#6, r8
	EXPECT	r8, 2			; Z
	mov.l	#0xbf000000, r4		; -0.5
	ftou	r4, r6
	EXPECT	r6, 0
	EXPECT_FPSW 0x40000140		; inexact only
	mvtc	#0x100, fpsw
	mov.l	#0x4f800000, r4		; 4294967296.0
	ftou	r4, r6
	EXPECT	r6, 0xffffffff
	EXPECT_FPSW 0x84000104		; invalid
	mov.l	#0xbf800000, r4		; -1.0
	ftou	r4, r6
	EXPECT	r6, 0
	mov.l	#0x7fc00000, r4		; +NaN
	ftou	r4, r6
	EXPECT	r6, 0xffffffff
	mov.l	#0xffc00000, r4		; -NaN
	ftou	r4, r6
	EXPECT	r6, 0
	mvtc	#0x100, fpsw
	mov.l	#_nine, r7
	ftou	[r7], r6
	EXPECT	r6, 9

	; utof
	mov.l	#1, r4
	utof	r4, r6
	EXPECT	r6, 0x3f800000
	mov.l	#0x80000000, r4
	utof	r4, r6
	EXPECT	r6, 0x4f000000		; 2147483648.0, not negative
	mov.l	#0xffffffff, r4
	utof	r4, r6
	EXPECT	r6, 0x4f800000		; rounded to nearest
	EXPECT_FPSW 0x40000140		; inexact
	mvtc	#0x100, fpsw
	mov.l	#_bytes, r7
	.byte	0xfc, 0x54, 0x76	; utof [r7].ub, r6
	EXPECT	r6, 0x437f0000		; 255.0
	; .w is sign-extended, then converted as unsigned: 0xffffc0ff
	.byte	0x06, 0x60, 0x15, 0x76	; utof [r7].w, r6
	EXPECT	r6, 0x4f7fffc1		; 4294951168.0 (rounded)
	.byte	0x06, 0xe0, 0x15, 0x76	; utof [r7].uw, r6
	EXPECT	r6, 0x4740ff00		; 49407.0
	.byte	0x06, 0xa1, 0x15, 0x76, 0x01 ; utof 4[r7].l, r6
	EXPECT	r6, 0x4f800000		; 0xffffffff

	mov.l	#0, r1
	rts

	.data
	.balign	4
_nine:	.long	0x41100000		; 9.0
	.long	0x3e800000		; 0.25
_bytes:	.byte	0xff, 0xc0, 0, 0	; .ub = 255, .w / .uw = 0xc0ff
	.long	0xffffffff
