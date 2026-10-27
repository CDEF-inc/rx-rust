; The memory access forms code generation uses since the RX MOV / string
; work (post-increment / pre-decrement, register-indexed, immediate to
; memory, memory to memory) and the string instructions, with the values
; the RXv3 manual's Operation gives. Run on every core.
	.include "harness.inc"

; FILL off, n, val: n bytes of _buf from off set to val, without the
; string instructions. Clobbers r1-r3.
	.macro	FILL off, n, val
	mov.l	#_buf + \off, r1
	mov.l	#\n, r3
	mov.l	#\val, r2
9997:	mov.b	r2, [r1]
	add	#1, r1
	sub	#1, r3
	bne.b	9997b
	.endm

; FLAGS: r6 = PSW.Z << 1 | PSW.C
	.macro	FLAGS
	mvfc	psw, r6
	and	#3, r6
	.endm

	.text
	.global	_main
_main:
	mov.l	#_resume, r11
	mov.l	#_buf, r7
	FILL	0, 512, 0

	; --- post-increment / pre-decrement
	mov.l	#0x11223344, r4
	mov.l	r7, r5
	mov.l	r4, [r5+]
	sub	r7, r5, r6
	EXPECT	r6, 4
	mov.l	[r7], r6
	EXPECT	r6, 0x11223344
	mov.b	r4, [r5+]		; buf[4] = 44h
	mov.w	r4, [-r5]		; buf[3..4] = 44h 33h
	sub	r7, r5, r6
	EXPECT	r6, 3
	mov.l	[r7], r6
	EXPECT	r6, 0x44223344
	movu.b	4[r7], r6
	EXPECT	r6, 0x33
	mov.b	#-128, 5[r7]
	mov.l	r7, r5
	mov.l	[r5+], r6
	EXPECT	r6, 0x44223344
	mov.b	[r5+], r6		; 33h
	EXPECT	r6, 0x33
	mov.b	[r5+], r6		; 80h, sign-extended
	EXPECT	r6, 0xffffff80
	movu.b	[-r5], r6
	EXPECT	r6, 0x80
	mov.w	[-r5], r6		; buf[3..4]
	EXPECT	r6, 0x3344
	sub	r7, r5, r6
	EXPECT	r6, 3
	movu.w	[r5+], r6
	EXPECT	r6, 0x3344
	mov.w	[r5+], r6		; buf[5..6] = 80h 00h
	EXPECT	r6, 0x0080
	; the same register: a load keeps the data (MOV note 4), a store
	; stores the value before the update (note 3)
	mov.l	r7, r5
	mov.l	[r5+], r5
	EXPECT	r5, 0x44223344
	add	#48, r7, r5
	mov.l	r5, [r5+]
	mov.l	48[r7], r6
	add	#48, r7, r9
	EXPECT_REG r6, r9

	; --- register-indexed: [Ri, Rb] is Rb + Ri * size
	mov.l	#0x01020304, r4
	mov.l	#2, r3
	mov.l	r4, [r3, r7]		; buf + 8
	mov.l	8[r7], r6
	EXPECT	r6, 0x01020304
	mov.l	[r3, r7], r6
	EXPECT	r6, 0x01020304
	mov.w	r4, [r3, r7]		; buf + 4
	movu.w	4[r7], r6
	EXPECT	r6, 0x0304
	mov.b	#0xfe, 2[r7]
	mov.b	[r3, r7], r6		; buf + 2
	EXPECT	r6, 0xfffffffe
	movu.b	[r3, r7], r6
	EXPECT	r6, 0xfe
	mov.w	#-3, 4[r7]
	mov.w	[r3, r7], r6
	EXPECT	r6, 0xfffffffd
	movu.w	[r3, r7], r6
	EXPECT	r6, 0xfffd
	mov.b	r4, [r3, r7]		; buf + 2
	movu.b	2[r7], r6
	EXPECT	r6, 0x04

	; --- immediate to memory
	mov.w	#200, 16[r7]		; dsp:5, zero-extended
	movu.w	16[r7], r6
	EXPECT	r6, 200
	mov.w	#0xffff, 18[r7]
	movu.w	18[r7], r6
	EXPECT	r6, 0xffff
	mov.l	#-129, 20[r7]
	mov.l	20[r7], r6
	EXPECT	r6, -129
	mov.l	#255, 12[r7]		; dsp:5
	mov.l	12[r7], r6
	EXPECT	r6, 255
	mov.b	#200, 300[r7]
	movu.b	300[r7], r6
	EXPECT	r6, 200
	movu.b	301[r7], r6
	EXPECT	r6, 0
	mov.l	#0x12345678, 400[r7]
	mov.l	400[r7], r6
	EXPECT	r6, 0x12345678
	add	#24, r7, r8
	mov.l	#0x800000, [r8]
	mov.l	24[r7], r6
	EXPECT	r6, 0x800000
	mov.w	#0x1234, [r8]
	mov.l	24[r7], r6
	EXPECT	r6, 0x801234

	; --- memory to memory
	mov.l	20[r7], 40[r7]
	mov.l	40[r7], r6
	EXPECT	r6, -129
	mov.w	16[r7], [r8]		; buf + 24
	mov.l	24[r7], r6
	EXPECT	r6, 0x8000c8
	mov.b	[r7], 44[r7]
	movu.b	44[r7], r6
	EXPECT	r6, 0x44
	mov.l	400[r7], [r8]
	mov.l	24[r7], r6
	EXPECT	r6, 0x12345678

	; --- string instructions on the bytes at _src: "abc", 0, "xyz", 0
	FILL	64, 64, 0x55
	mov.l	#_buf + 64, r1		; smovf: 5 bytes upwards
	mov.l	#_src, r2
	mov.l	#5, r3
	smovf
	mov.l	r1, r8
	mov.l	r2, r9
	mov.l	r3, r10
	sub	r7, r8
	EXPECT	r8, 69
	mov.l	#_src + 5, r6
	EXPECT_REG r9, r6
	EXPECT	r10, 0
	mov.l	64[r7], r6
	EXPECT	r6, 0x00636261
	movu.b	68[r7], r6
	EXPECT	r6, 0x78
	movu.b	69[r7], r6
	EXPECT	r6, 0x55
	mov.l	#_buf + 64, r1		; R3 = 0: nothing happens
	mov.l	#_src + 4, r2
	mov.l	#0, r3
	smovf
	mov.l	r1, r8
	sub	r7, r8
	EXPECT	r8, 64
	movu.b	64[r7], r6
	EXPECT	r6, 0x61

	mov.l	#_buf + 82, r1		; smovb: 3 bytes downwards
	mov.l	#_src + 2, r2
	mov.l	#3, r3
	smovb
	mov.l	r1, r8
	mov.l	r2, r9
	sub	r7, r8
	EXPECT	r8, 79
	mov.l	#_src - 1, r6
	EXPECT_REG r9, r6
	mov.l	80[r7], r6
	EXPECT	r6, 0x55636261
	movu.b	79[r7], r6
	EXPECT	r6, 0x55

	mov.l	#_buf + 96, r1		; smovu: up to the 0 (4 bytes) of 10
	mov.l	#_src, r2
	mov.l	#10, r3
	smovu
	EXPECT	r3, 6
	mov.l	96[r7], r6
	EXPECT	r6, 0x00636261
	movu.b	100[r7], r6
	EXPECT	r6, 0x55

	mov.l	#_buf + 112, r1		; sstr.b / .w / .l
	mov.l	#0x1a5, r2
	mov.l	#3, r3
	sstr.b
	mov.l	r1, r8
	mov.l	r3, r10
	sub	r7, r8
	EXPECT	r8, 115
	EXPECT	r10, 0
	mov.l	112[r7], r6
	EXPECT	r6, 0x55a5a5a5
	mov.l	#_buf + 116, r1
	mov.l	#0x12345678, r2
	mov.l	#2, r3
	sstr.w
	mov.l	r1, r8
	sub	r7, r8
	EXPECT	r8, 120
	mov.l	116[r7], r6
	EXPECT	r6, 0x56785678
	mov.l	#_buf + 120, r1
	mov.l	#0x12345678, r2
	mov.l	#2, r3
	sstr.l
	mov.l	120[r7], r6
	EXPECT	r6, 0x12345678
	mov.l	124[r7], r6
	EXPECT	r6, 0x12345678

	mov.l	#_src, r1		; scmpu: "abc" / "abd" differ at the 3rd
	mov.l	#_src2, r2
	mov.l	#10, r3
	scmpu
	FLAGS
	EXPECT	r3, 7
	EXPECT	r6, 0			; Z = 0, C = 0 ('c' < 'd')
	mov.l	#_src2, r1		; "abd" / "abc": C = 1
	mov.l	#_src, r2
	mov.l	#10, r3
	scmpu
	FLAGS
	EXPECT	r6, 1
	mov.l	#_src, r1		; "abc" / "abc": up to the 0
	mov.l	#_src, r2
	mov.l	#10, r3
	scmpu
	FLAGS
	EXPECT	r3, 6
	EXPECT	r6, 3			; Z = 1, C = 1

	mov.l	#_src, r1		; suntil.b 'c': found at the 3rd byte
	mov.l	#0x63, r2
	mov.l	#10, r3
	suntil.b
	FLAGS
	mov.l	r1, r8
	mov.l	#_src + 3, r9
	EXPECT_REG r8, r9
	EXPECT	r3, 7
	EXPECT	r6, 3			; Z = 1, C = 1
	mov.l	#_src, r1		; swhile.b 'a': "aaab" stops at 'b'
	add	#8, r1
	mov.l	#0x61, r2
	mov.l	#10, r3
	swhile.b
	FLAGS
	mov.l	r1, r8
	mov.l	#_src + 12, r9
	EXPECT_REG r8, r9
	EXPECT	r3, 6
	EXPECT	r6, 1			; Z = 0, C = 1 ('b' > 'a')
	mov.l	#_src, r1		; swhile.b 'a': "aaab" with R3 = 4, the
	add	#8, r1			; last one differs
	mov.l	#0x61, r2
	mov.l	#4, r3
	swhile.b
	FLAGS
	EXPECT	r3, 0
	EXPECT	r6, 1			; Z = 0 (not all matched), C = 1
	mov.l	#_src, r1		; swhile.b 'b': 'a' < 'b' at once
	mov.l	#0x62, r2
	mov.l	#10, r3
	swhile.b
	FLAGS
	mov.l	r1, r8
	mov.l	#_src + 1, r9
	EXPECT_REG r8, r9
	EXPECT	r3, 9
	EXPECT	r6, 0			; Z = 0, C = 0
	mov.l	#_src, r1		; suntil.b 'q': no match in 4
	mov.l	#0x71, r2
	mov.l	#4, r3
	suntil.b
	FLAGS
	mov.l	r1, r8
	mov.l	#_src + 4, r9
	EXPECT_REG r8, r9
	EXPECT	r3, 0
	EXPECT	r6, 0			; Z = 0, C = 0 (0 < 'q')
	mov.l	#_words, r1		; suntil.w 3
	mov.l	#3, r2
	mov.l	#10, r3
	suntil.w
	mov.l	r1, r8
	mov.l	#_words + 6, r9
	EXPECT_REG r8, r9
	EXPECT	r3, 7
	mov.l	#_longs, r1		; suntil.l 0x80000000
	mov.l	#0x80000000, r2
	mov.l	#10, r3
	suntil.l
	FLAGS
	mov.l	r1, r8
	mov.l	#_longs + 8, r9
	EXPECT_REG r8, r9
	EXPECT	r3, 8
	EXPECT	r6, 3
	mov.l	#_longs, r1		; swhile.l 5: all 2 match
	mov.l	#5, r2
	mov.l	#1, r3
	swhile.l
	FLAGS
	EXPECT	r3, 0
	EXPECT	r6, 3			; Z = 1, C = 1

	mov.l	#0, r1
	rts

	.data
_src:	.byte	0x61, 0x62, 0x63, 0, 0x78, 0x79, 0x7a, 0
	.byte	0x61, 0x61, 0x61, 0x62
_src2:	.byte	0x61, 0x62, 0x64, 0
	.balign	2
_words:	.short	1, 2, 3, 4
	.balign	4
_longs:	.long	5, 0x80000000, 7
	.bss
	.balign	4
_buf:	.space	512
