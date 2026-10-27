; FPSW and floating-point exceptions of the single-precision FPU, as the
; RXv3 manual describes them (1.2.2.8, 1.3). Run on cores with the FPU.
; FPSW bits: CV 2, CO 3, CZ 4, CU 5, CX 6, CE 7, DN 8, EV 10,
; FV 26, FO 27, FZ 28, FU 29, FX 30, FS 31.
	.include "harness.inc"

	.text
	.global	_main
_main:
	mov.l	#_resume, r11
	; FPSW resets to 00000100h (DN = 1)
	EXPECT_FPSW 0x00000100

	; Cause flags describe the last instruction; the F flags accumulate.
	mov.l	#0x3f800000, r4		; 1.0
	mov.l	#0x40400000, r5		; 3.0
	fdiv	r5, r4			; 1/3: inexact
	EXPECT_FPSW 0x40000140		; FX, DN, CX
	mov.l	#0x3f800000, r4
	fadd	r4, r4			; 2.0: exact
	EXPECT	r4, 0x40000000
	EXPECT_FPSW 0x40000100		; FX stays, CX cleared

	; DN = 1: an underflow returns 0 and sets CU / FU, not CX.
	mvtc	#0x100, fpsw
	mov.l	#0x1e3ce508, r4		; 1e-20
	mov.l	r4, r5
	fmul	r4, r5			; 1e-40
	EXPECT	r5, 0
	EXPECT_FPSW 0xa0000120		; FS, FU, DN, CU

	; DN = 1: a denormalized operand is handled as 0.
	mvtc	#0x100, fpsw
	mov.l	#1, r4			; smallest positive denormal
	mov.l	#0x3f800000, r5
	fadd	r4, r5
	EXPECT	r5, 0x3f800000
	EXPECT_FPSW 0x00000100

	; DN = 0: a denormalized operand is unimplemented processing, which
	; cannot be masked; the destination is unchanged.
	mvtc	#0, fpsw
	TRY	fadd r4, r5
	EXPECT	r13, 2
	EXPECT	r5, 0x3f800000
	EXPECT_FPSW 0x00000080		; CE

	; DN = 0: so is an underflow.
	mvtc	#0, fpsw
	mov.l	#0x1e3ce508, r4
	mov.l	r4, r5
	TRY	fmul r4, r5
	EXPECT	r13, 2
	EXPECT	r5, 0x1e3ce508
	EXPECT_FPSW 0x00000080

	; Invalid operation, masked: inf - inf gives 7fffffff.
	mvtc	#0x100, fpsw
	mov.l	#0x7f800000, r4
	mov.l	r4, r5
	fsub	r4, r5
	EXPECT	r5, 0x7fffffff
	EXPECT_FPSW 0x84000104		; FS, FV, DN, CV

	; Invalid operation, enabled (EV): the exception, with the destination
	; unchanged and FV not set.
	mvtc	#0x500, fpsw
	mov.l	#0x7f800000, r5
	TRY	fsub r4, r5
	EXPECT	r13, 2
	EXPECT	r5, 0x7f800000
	EXPECT_FPSW 0x00000504		; EV, DN, CV

	; Integer conversion of a NaN follows its sign (table 1.6).
	mvtc	#0x100, fpsw
	mov.l	#0xffc00000, r4		; negative QNaN
	ftoi	r4, r5
	EXPECT	r5, 0x80000000
	round	r4, r5
	EXPECT	r5, 0x80000000
	mov.l	#0x7fc00000, r4		; positive QNaN
	ftoi	r4, r5
	EXPECT	r5, 0x7fffffff
	EXPECT_FPSW 0x84000104		; invalid

	; Z and S of an integer result: Z only for 0, also for 80000000h
	mvtc	#0x100, fpsw
	mov.l	#0xcf000000, r4		; -2147483648.0
	ftoi	r4, r5
	mvfc	psw, r8
	EXPECT	r5, 0x80000000
	and	#6, r8
	EXPECT	r8, 4			; S, not Z
	round	r4, r5
	mvfc	psw, r8
	and	#6, r8
	EXPECT	r8, 4
	mov.l	#0, r4
	ftoi	r4, r5
	mvfc	psw, r8
	and	#6, r8
	EXPECT	r8, 2			; Z

	; MVTC cannot set the cause flags, only clear them; FS is read-only.
	mvtc	#0x100, fpsw
	mvtc	#0x800001fc, fpsw
	EXPECT_FPSW 0x00000100
	mvtc	#0x04000100, fpsw	; FV
	EXPECT_FPSW 0x84000100		; FS follows FV

	mvtc	#0x100, fpsw
	mov.l	#0, r1
	rts
