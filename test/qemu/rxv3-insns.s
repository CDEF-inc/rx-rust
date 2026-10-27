; RXv3 additions: bfmov / bfmovz, xor rs, rs2, rd and the register bank
; save function, save / rstr (RXv3 manual section 3, 1.9). Run on RXv3 and
; later (16 save register banks by default).
	.include "harness.inc"

; BF insn, slsb, dlsb, width, src, dest, want: rd = dest, then insn.
	.macro	BF insn, slsb, dlsb, width, src, dest, want
	mov.l	#\src, r4
	mov.l	#\dest, r5
	\insn	#\slsb, #\dlsb, #\width, r4, r5
	EXPECT	r5, \want
	EXPECT	r4, \src		; the source is unchanged
	.endm

	.text
	.global	_main
_main:
	mov.l	#_resume, r11

	; bfmov keeps the other bits of the destination, bfmovz clears them
	BF	bfmov, 5, 10, 3, 0x000000e0, 0x00000000, 0x00001c00
	BF	bfmov, 5, 10, 3, 0x000000e0, 0xffffffff, 0xffffffff
	BF	bfmov, 5, 10, 3, 0xffffff1f, 0xffffffff, 0xffffe3ff
	BF	bfmov, 0, 0, 31, 0xffffffff, 0x80000000, 0xffffffff
	BF	bfmov, 0, 0, 31, 0xffffffff, 0x00000000, 0x7fffffff
	BF	bfmov, 28, 0, 4, 0xa0000000, 0x12345678, 0x1234567a
	BF	bfmov, 16, 16, 16, 0x12345678, 0xaaaaaaaa, 0x1234aaaa
	BF	bfmov, 0, 31, 1, 0x00000001, 0x00000000, 0x80000000
	BF	bfmov, 31, 0, 1, 0x80000000, 0xfffffffe, 0xffffffff
	BF	bfmovz, 0, 28, 4, 0x0000000b, 0xffffffff, 0xb0000000
	BF	bfmovz, 4, 8, 8, 0x12345ab0, 0xffffffff, 0x0000ab00
	BF	bfmovz, 31, 0, 1, 0x80000000, 0xffffffff, 0x00000001
	BF	bfmovz, 8, 0, 24, 0xabcdef01, 0x55555555, 0x00abcdef
	; the source and the destination may be the same register
	mov.l	#0x0000beef, r4
	bfmov	#0, #16, #16, r4, r4
	EXPECT	r4, 0xbeefbeef

	; xor rs, rs2, rd: rd = rs ^ rs2, sets Z and S
	mov.l	#0xff00ff00, r4
	mov.l	#0x0ff00ff0, r5
	xor	r4, r5, r6
	mvfc	psw, r8
	EXPECT	r6, 0xf0f0f0f0
	and	#6, r8
	EXPECT	r8, 4			; S
	xor	r4, r4, r6
	mvfc	psw, r8
	EXPECT	r6, 0
	and	#6, r8
	EXPECT	r8, 2			; Z
	EXPECT	r4, 0xff00ff00

	; save / rstr: R1-R15, USP, FPSW, ACC0 and ACC1
	mov.l	#0x55555555, r6
	mvtachi	r6, a0
	mvtacgu	r6, a0
	mov.l	#0x66666666, r6
	mvtaclo	r6, a0
	mov.l	#0x77777777, r6
	mvtachi	r6, a1
	mov.l	#0xffffff88, r6
	mvtacgu	r6, a1
	mov.l	#0x99999999, r6
	mvtaclo	r6, a1
	mvtc	#0x01001230, usp
	mvtc	#0x101, fpsw		; DN, round towards 0
	mov.l	#0x11110001, r1
	mov.l	#0x11110002, r2
	mov.l	#0x11110003, r3
	mov.l	#0x11110004, r4
	mov.l	#0x11110005, r5
	mov.l	#0x11110006, r6
	mov.l	#0x11110007, r7
	mov.l	#0x11110008, r8
	mov.l	#0x11110009, r9
	mov.l	#0x1111000a, r10
	mov.l	#0x1111000b, r11
	mov.l	#0x1111000c, r12
	mov.l	#0x1111000d, r13
	mov.l	#0x1111000e, r14
	mov.l	#0x1111000f, r15
	save	#3
	; a second bank with other values, selected by a register
	mov.l	#0x22220001, r1
	mov.l	#0x22220002, r2
	mov.l	#7, r3
	save	r3
	; clobber everything that save stores
	mov.l	#0, r1
	mov.l	#0, r2
	mov.l	#0, r3
	mov.l	#0, r4
	mov.l	#0, r5
	mov.l	#0, r6
	mov.l	#0, r7
	mov.l	#0, r8
	mov.l	#0, r9
	mov.l	#0, r10
	mov.l	#0, r11
	mov.l	#0, r12
	mov.l	#0, r13
	mov.l	#0, r14
	mov.l	#0, r15
	mvtachi	r1, a0
	mvtacgu	r1, a0
	mvtaclo	r1, a0
	mvtachi	r1, a1
	mvtacgu	r1, a1
	mvtaclo	r1, a1
	mvtc	#0, usp
	mvtc	#0x100, fpsw
	rstr	#3
	; keep r1, r2, r14 and r15 (used by EXPECT) on the stack
	sub	#16, r0
	mov.l	r1, [r0]
	mov.l	r2, 4[r0]
	mov.l	r14, 8[r0]
	mov.l	r15, 12[r0]
	EXPECT	r3, 0x11110003
	EXPECT	r4, 0x11110004
	EXPECT	r5, 0x11110005
	EXPECT	r6, 0x11110006
	EXPECT	r7, 0x11110007
	EXPECT	r8, 0x11110008
	EXPECT	r9, 0x11110009
	EXPECT	r10, 0x1111000a
	EXPECT	r11, 0x1111000b
	EXPECT	r12, 0x1111000c
	EXPECT	r13, 0x1111000d
	mov.l	[r0], r9
	EXPECT	r9, 0x11110001
	mov.l	4[r0], r9
	EXPECT	r9, 0x11110002
	mov.l	8[r0], r9
	EXPECT	r9, 0x1111000e
	mov.l	12[r0], r9
	EXPECT	r9, 0x1111000f
	add	#16, r0
	mvfc	usp, r9
	EXPECT	r9, 0x01001230
	mvfc	fpsw, r9
	EXPECT	r9, 0x101
	mvfacgu	#0, a0, r9
	EXPECT	r9, 0x55
	mvfachi	#0, a0, r9
	EXPECT	r9, 0x55555555
	mvfaclo	#0, a0, r9
	EXPECT	r9, 0x66666666
	mvfacgu	#0, a1, r9
	EXPECT	r9, 0xffffff88
	mvfachi	#0, a1, r9
	EXPECT	r9, 0x77777777
	mvfaclo	#0, a1, r9
	EXPECT	r9, 0x99999999
	; the restored FPSW.RM is in effect: 2/3 rounded towards 0
	mov.l	#0x40000000, r4		; 2.0
	mov.l	#0x40400000, r5		; 3.0
	fdiv	r5, r4
	EXPECT	r4, 0x3f2aaaaa
	mvtc	#0x100, fpsw

	; the other bank
	mov.l	#7, r9
	rstr	r9
	sub	#8, r0
	mov.l	r1, [r0]
	mov.l	r2, 4[r0]
	EXPECT	r3, 7
	EXPECT	r4, 0x11110004
	mov.l	[r0], r9
	EXPECT	r9, 0x22220001
	mov.l	4[r0], r9
	EXPECT	r9, 0x22220002
	add	#8, r0
	mvtc	#0x100, fpsw

	; save / rstr are privileged: in user mode they trap (vector 20)
	mov.l	#_resume, r11
	USER_MODE
	mvfc	psw, r4
	and	#0x00120000, r4
	EXPECT	r4, 0x00120000		; PM and U (the switch sets U)
	TRY	save #0
	EXPECT	r13, 4
	TRY	rstr #0
	EXPECT	r13, 4
	SUPERVISOR_MODE
	mvfc	psw, r4
	and	#0x00120000, r4
	EXPECT	r4, 0

	mov.l	#0, r1
	rts
