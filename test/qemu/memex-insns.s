; Memory sources with a size extension (memex), xchg, push from memory and
; the bit operations, with the values the RXv3 manual's Operation gives.
; Run on every core (itof needs the FPU, utof RXv2).
	.include "harness.inc"

; FLAGS: r6 = PSW.Z << 1 | PSW.C
	.macro	FLAGS
	mvfc	psw, r6
	and	#3, r6
	.endm

	.text
	.global	_main
_main:
	mov.l	#_resume, r11
	mov.l	#_d, r7
	mov.l	#_d - 512, r8		; for the dsp:16 forms
	mov.l	#_buf, r9

	; --- the size extension of the source
	mov.l	#0, r5
	add	[r7].b, r5		; 80h sign-extended
	EXPECT	r5, 0xffffff80
	mov.l	#0, r5
	add	[r7].ub, r5
	EXPECT	r5, 0x80
	mov.l	#0, r5
	add	4[r7].w, r5		; 8001h
	EXPECT	r5, 0xffff8001
	mov.l	#0, r5
	add	4[r7].uw, r5
	EXPECT	r5, 0x8001
	mov.l	#0, r5
	add	8[r7].l, r5
	EXPECT	r5, 0x80000001
	mov.l	#0, r5
	add	8[r7], r5		; no size extension: .L
	EXPECT	r5, 0x80000001
	mov.l	#0, r5
	add	514[r8].ub, r5		; dsp:16, feh
	EXPECT	r5, 0xfe
	mov.l	#0, r5
	add	518[r8].w, r5		; dsp:16 (259 words), 7ffeh
	EXPECT	r5, 0x7ffe
	mov.l	#0, r5
	add	524[r8].l, r5		; dsp:8 (131 longs), 12345678h
	EXPECT	r5, 0x12345678

	; --- sub / cmp / mul / and / or
	mov.l	#10, r5
	sub	16[r7].l, r5		; 10 - 100
	EXPECT	r5, -90
	mov.l	#100, r5
	cmp	16[r7].l, r5
	FLAGS
	EXPECT	r6, 3			; Z = 1, C = 1
	mov.l	#3, r5
	mul	20[r7].l, r5		; 3 * -7
	EXPECT	r5, -21
	mov.l	#0xffff, r5
	and	2[r7].ub, r5		; feh
	EXPECT	r5, 0xfe
	mov.l	#0x100, r5
	or	[r7].b, r5		; ffffff80h
	EXPECT	r5, 0xffffff80

	; --- max / min / emul / emulu / div / divu / tst / xor
	mov.l	#5, r5
	max	[r7].b, r5
	EXPECT	r5, 5
	min	[r7].b, r5
	EXPECT	r5, -128
	mov.l	#5, r5
	max	[r7].ub, r5
	EXPECT	r5, 0x80
	mov.l	#3, r2
	emul	20[r7].l, r2		; r3:r2 = 3 * -7
	mov.l	r2, r4
	EXPECT	r4, -21
	EXPECT	r3, -1
	mov.l	#3, r2
	emulu	[r7].ub, r2		; 3 * 80h
	mov.l	r2, r4
	EXPECT	r4, 0x180
	EXPECT	r3, 0
	mov.l	#100, r5
	div	20[r7].l, r5		; 100 / -7
	EXPECT	r5, -14
	mov.l	#1000, r5
	divu	16[r7].l, r5
	EXPECT	r5, 10
	mov.l	#0x7f, r5
	tst	[r7].ub, r5		; 80h & 7fh = 0
	FLAGS
	and	#2, r6
	EXPECT	r6, 2
	mov.l	#0xffffffff, r5
	xor	12[r7].l, r5
	EXPECT	r5, 0xedcba987

	; --- adc / sbb (only .L)
	mov.l	#1, r5
	setpsw	c
	adc	16[r7].l, r5		; 1 + 100 + C
	EXPECT	r5, 102
	mov.l	#200, r5
	setpsw	c
	sbb	16[r7], r5		; 200 - 100 - !C
	EXPECT	r5, 100
	mov.l	#200, r5
	clrpsw	c
	sbb	16[r7].l, r5
	EXPECT	r5, 99

	; --- xchg: the memory gets the register's low byte / word / longword,
	; the register the extended memory value
	mov.l	#0x11223380, r4
	mov.l	r4, [r9]
	mov.l	#0xaabbccdd, r5
	xchg	[r9].b, r5
	EXPECT	r5, 0xffffff80
	mov.l	[r9], r4
	EXPECT	r4, 0x112233dd
	mov.l	#0x0102fffe, r5
	xchg	2[r9].uw, r5		; buf[2..3] = 1122h
	EXPECT	r5, 0x1122
	mov.l	[r9], r4
	EXPECT	r4, 0xfffe33dd
	mov.l	#7, r5
	xchg	[r9], r5
	EXPECT	r5, 0xfffe33dd
	mov.l	[r9], r4
	EXPECT	r4, 7
	mov.l	#1, r4
	mov.l	#2, r5
	xchg	r4, r5
	EXPECT	r4, 2
	EXPECT	r5, 1

	; --- push.sz from memory (the upper bits of .B / .W are undefined)
	mov.l	r0, r10
	push.l	12[r7]
	pop	r5
	EXPECT	r5, 0x12345678
	push	8[r7]			; .L
	pop	r5
	EXPECT	r5, 0x80000001
	push.w	4[r7]
	pop	r5
	and	#0xffff, r5
	EXPECT	r5, 0x8001
	push.b	2[r7]
	pop	r5
	and	#0xff, r5
	EXPECT	r5, 0xfe
	sub	r10, r0, r5		; SP back where it was
	EXPECT	r5, 0

	; --- bit operations on a byte in memory (the bit number is src & 7)
	mov.l	#0, r4
	mov.l	r4, [r9]
	bset	#3, [r9].b
	bset	#7, 1[r9].b
	mov.l	#13, r2
	bset	r2, [r9].b		; bit 5
	mov.l	[r9], r4
	EXPECT	r4, 0x8028
	bclr	#3, [r9].b
	bnot	#0, [r9].b
	mov.l	#9, r2
	bnot	r2, 1[r9]		; bit 1, without the .B
	mov.l	#15, r2
	bclr	r2, 1[r9].b		; bit 7
	mov.l	[r9], r4
	EXPECT	r4, 0x0221
	btst	#5, [r9].b
	FLAGS
	EXPECT	r6, 1			; Z = 0, C = 1
	mov.l	#12, r2
	btst	r2, [r9].b		; bit 4
	FLAGS
	EXPECT	r6, 2			; Z = 1, C = 0
	btst	#1, 1[r9].b
	FLAGS
	EXPECT	r6, 1
	mov.l	#0, r4
	cmp	#0, r4			; Z = 1, C = 1
	bmeq	#6, [r9].b
	bmne	#5, [r9].b
	bmc	#4, 2[r9].b
	bmgtu	#3, 2[r9].b		; C & ~Z = 0
	mov.l	[r9], r4
	EXPECT	r4, 0x00100201 | 0x40
	bnot	#6, 512[r8].b		; dsp:16 = _d: 80h -> c0h
	movu.b	[r7], r4
	EXPECT	r4, 0xc0
	bnot	#6, [r7].b		; back to 80h
	movu.b	[r7], r4
	EXPECT	r4, 0x80

	; --- bit operations on a register (the bit number is src & 31)
	mov.l	#0, r5
	bset	#31, r5
	bset	#4, r5
	mov.l	#33, r2
	bset	r2, r5			; bit 1
	EXPECT	r5, 0x80000012
	bclr	#31, r5
	mov.l	#4, r2
	bclr	r2, r5
	bnot	#0, r5
	mov.l	#35, r2
	bnot	r2, r5			; bit 3
	EXPECT	r5, 0x0000000b
	btst	#3, r5
	FLAGS
	EXPECT	r6, 1
	mov.l	#2, r2
	btst	r2, r5
	FLAGS
	EXPECT	r6, 2
	mov.l	#1, r4
	cmp	#2, r4			; 1 - 2: C = 0, Z = 0, S = 1
	bmltu	#20, r5
	bmgeu	#0, r5
	bmn	#21, r5
	bmle	#22, r5
	bmgt	#1, r5
	EXPECT	r5, 0x00700008

	; --- itof / utof from memory
	.if	FPU
	itof	20[r7].l, r5		; -7
	EXPECT	r5, 0xc0e00000
	itof	[r7].ub, r5		; 128
	EXPECT	r5, 0x43000000
	itof	4[r7].w, r5		; -32767
	EXPECT	r5, 0xc6fffe00
	.endif
	.if	V2
	utof	4[r7].uw, r5		; 32769
	EXPECT	r5, 0x47000100
	utof	16[r7], r5		; 100
	EXPECT	r5, 0x42c80000
	.endif

	mov.l	#0, r1
	rts

	.data
	.balign	4
_d:	.byte	0x80, 0x01, 0xfe, 0x7f	; _d + 0
	.short	0x8001, 0x7ffe		; _d + 4
	.long	0x80000001		; _d + 8
	.long	0x12345678		; _d + 12
	.long	100			; _d + 16
	.long	-7			; _d + 20
	.bss
	.balign	4
_buf:	.space	64
