; The double-precision FPU (RXv3 manual 1.10, 3.5.3). Run on cores with
; the DFPU. DPSW bits as FPSW: DCV 2, DCO 3, DCZ 4, DCU 5, DCX 6, DCE 7,
; DDN 8, DEV 10, DFV 26, ..., DFX 30, DFS 31; DECNT: EHM 0, EHS 16.
	.include "harness.inc"

; DSET drn, hi, lo: load a double as two words.
	.macro	DSET n, hi, lo
	dmov.l	#\hi, drh\n
	dmov.l	#\lo, drl\n
	.endm

; DEXPECT drn, hi, lo: check both halves of a double register.
	.macro	DEXPECT n, hi, lo
	dmov.l	drh\n, r9
	EXPECT	r9, \hi
	dmov.l	drl\n, r9
	EXPECT	r9, \lo
	.endm

; DPSW_SET value (mvtdc: cause flags can only be cleared)
	.macro	DPSW_SET v
	mov.l	#\v, r9
	mvtdc	r9, dpsw
	.endm

	.macro	EXPECT_DCR dcr, want
	mvfdc	\dcr, r9
	EXPECT	r9, \want
	.endm

	.text
	.global	_main
_main:
	mov.l	#_resume, r11

	; reset values
	EXPECT_DCR dpsw, 0x00000100	; DDN
	EXPECT_DCR dcmr, 0
	EXPECT_DCR decnt, 0x00000001	; EHM

	; dmov between registers and immediates
	DSET	1, 0xffffffff, 0xffffffff
	dmov.d	#0x3ff00000, drh1	; .d to the upper half clears the lower
	DEXPECT	1, 0x3ff00000, 0
	dmov.l	#0x40000000, drh2
	dmov.l	#0x12345678, drl2
	dmov.l	#0x40000000, drh2	; .l keeps the other half
	DEXPECT	2, 0x40000000, 0x12345678
	dmov.l	#0, drl2		; dr2 = 2.0
	mov.l	#0x40080000, r4
	mov.l	#0xffffffff, r5
	dmov.l	r5, drl3
	dmov.d	r4, drh3		; dr3 = 3.0
	DEXPECT	3, 0x40080000, 0
	dmov.l	r5, drl4
	dmov.l	r4, drh4
	DEXPECT	4, 0x40080000, 0xffffffff
	dmov.d	dr3, dr4
	DEXPECT	4, 0x40080000, 0

	; dmov.d to and from memory (little-endian, dsp scaled by 8)
	mov.l	#_buf, r7
	dmov.d	dr2, [r7]
	dmov.d	dr3, 8[r7]
	dmov.d	dr1, 2048[r7]
	mov.l	[r7], r4
	EXPECT	r4, 0			; lower word of 2.0
	mov.l	4[r7], r4
	EXPECT	r4, 0x40000000
	mov.l	12[r7], r4
	EXPECT	r4, 0x40080000
	mov.l	#_buf + 2048, r8
	mov.l	4[r8], r4
	EXPECT	r4, 0x3ff00000
	dmov.d	8[r7], dr5
	DEXPECT	5, 0x40080000, 0
	dmov.d	[r7], dr5
	DEXPECT	5, 0x40000000, 0

	; arithmetic: dest = src2 op src
	dadd	dr1, dr2, dr6		; 2 + 1
	DEXPECT	6, 0x40080000, 0
	dsub	dr1, dr3, dr6		; 3 - 1
	DEXPECT	6, 0x40000000, 0
	dmul	dr2, dr3, dr6		; 3 * 2
	DEXPECT	6, 0x40180000, 0
	EXPECT_DCR dpsw, 0x00000100	; exact: no flags
	ddiv	dr3, dr1, dr6		; 1 / 3
	DEXPECT	6, 0x3fd55555, 0x55555555
	EXPECT_DCR dpsw, 0x40000140	; DFX, DDN, DCX
	dsub	dr1, dr1, dr6		; exact: cause cleared, DFX stays
	DEXPECT	6, 0, 0
	EXPECT_DCR dpsw, 0x40000100
	DPSW_SET 0x102			; round towards +
	ddiv	dr3, dr1, dr6
	DEXPECT	6, 0x3fd55555, 0x55555556
	DPSW_SET 0x103			; round towards -
	dsub	dr1, dr1, dr6		; x - x is -0 in this mode
	DEXPECT	6, 0x80000000, 0
	DPSW_SET 0x100

	; dsqrt
	DSET	7, 0x40100000, 0	; 4.0
	dsqrt	dr7, dr6
	DEXPECT	6, 0x40000000, 0
	DSET	7, 0x40000000, 0	; 2.0
	dsqrt	dr7, dr6
	DEXPECT	6, 0x3ff6a09e, 0x667f3bcd
	DPSW_SET 0x100
	DSET	7, 0xbff00000, 0	; -1.0
	dsqrt	dr7, dr6
	DEXPECT	6, 0x7fffffff, 0xffffffff
	EXPECT_DCR dpsw, 0x84000104	; DFS, DFV, DDN, DCV

	; NaNs (table 1.14): two QNaNs give src2, an SNaN wins and is quieted
	DPSW_SET 0x100
	DSET	8, 0x7ff80000, 1	; QNaN A
	DSET	9, 0x7ff80000, 2	; QNaN B
	dadd	dr8, dr9, dr6
	DEXPECT	6, 0x7ff80000, 2
	dadd	dr9, dr8, dr6
	DEXPECT	6, 0x7ff80000, 1
	EXPECT_DCR dpsw, 0x00000100	; QNaN: no exception
	DSET	10, 0x7ff00000, 3	; SNaN
	dadd	dr10, dr9, dr6
	DEXPECT	6, 0x7ff80000, 3
	dadd	dr9, dr10, dr6
	DEXPECT	6, 0x7ff80000, 3
	EXPECT_DCR dpsw, 0x84000104	; invalid

	; dcmp and mvfdr: RES = src2 cm src, Z = RES
	DPSW_SET 0x100
	dcmplt	dr1, dr2		; 2 < 1
	EXPECT_DCR dcmr, 0
	dcmplt	dr2, dr1		; 1 < 2
	EXPECT_DCR dcmr, 1
	mvfdr
	mvfc	psw, r8
	and	#2, r8
	EXPECT	r8, 2			; Z = 1
	dcmpeq	dr1, dr1
	EXPECT_DCR dcmr, 1
	dcmple	dr1, dr2		; 2 <= 1
	EXPECT_DCR dcmr, 0
	mvfdr
	mvfc	psw, r8
	and	#2, r8
	EXPECT	r8, 0
	dcmple	dr1, dr1
	EXPECT_DCR dcmr, 1
	dcmpun	dr1, dr2
	EXPECT_DCR dcmr, 0
	dcmpun	dr8, dr1		; QNaN: unordered, no exception
	EXPECT_DCR dcmr, 1
	dcmpeq	dr8, dr8
	EXPECT_DCR dcmr, 0
	EXPECT_DCR dpsw, 0x00000100
	dcmpeq	dr10, dr1		; SNaN: invalid
	EXPECT_DCR dpsw, 0x84000104

	; conversions (the 32-bit results go to the lower half)
	DPSW_SET 0x100
	DSET	7, 0x40040000, 0	; 2.5
	dtoi	dr7, dr6
	dmov.l	drl6, r9
	EXPECT	r9, 2
	dround	dr7, dr6		; nearest even
	dmov.l	drl6, r9
	EXPECT	r9, 2
	DSET	7, 0x400c0000, 0	; 3.5
	dround	dr7, dr6
	dmov.l	drl6, r9
	EXPECT	r9, 4
	DSET	7, 0xc0040000, 0	; -2.5
	dtoi	dr7, dr6
	dmov.l	drl6, r9
	EXPECT	r9, 0xfffffffe
	DPSW_SET 0x100
	DSET	7, 0x41efffff, 0xffe00000 ; 4294967295.0
	dtou	dr7, dr6
	dmov.l	drl6, r9
	EXPECT	r9, 0xffffffff
	EXPECT_DCR dpsw, 0x00000100
	DSET	7, 0xbff00000, 0	; -1.0
	dtou	dr7, dr6
	dmov.l	drl6, r9
	EXPECT	r9, 0
	EXPECT_DCR dpsw, 0x84000104	; invalid
	DPSW_SET 0x100
	DSET	7, 0xfff80000, 0	; negative QNaN
	dtoi	dr7, dr6
	dmov.l	drl6, r9
	EXPECT	r9, 0x80000000
	dtou	dr7, dr6
	dmov.l	drl6, r9
	EXPECT	r9, 0
	DPSW_SET 0x100
	dmov.d	dr3, dr7
	ddiv	dr7, dr1, dr7		; 1/3
	dtof	dr7, dr6
	dmov.l	drl6, r9
	EXPECT	r9, 0x3eaaaaab
	mov.l	#0x3fc00000, r4		; 1.5f
	ftod	r4, dr6
	DEXPECT	6, 0x3ff80000, 0
	mov.l	#-1, r4
	itod	r4, dr6
	DEXPECT	6, 0xbff00000, 0
	utod	r4, dr6
	DEXPECT	6, 0x41efffff, 0xffe00000

	; dabs / dneg change no flags
	DPSW_SET 0x100
	ddiv	dr3, dr1, dr6		; sets DCX
	dneg	dr1, dr6
	DEXPECT	6, 0xbff00000, 0
	dabs	dr6, dr6
	DEXPECT	6, 0x3ff00000, 0
	EXPECT_DCR dpsw, 0x40000140

	; DDN = 0: a denormal operand is unimplemented processing; the
	; destination is unchanged, DEPC holds the instruction and, with EHM,
	; DECNT.EHS is set; while EHS is set the DPSW and DEPC stay.
	DPSW_SET 0
	DSET	7, 0, 1			; smallest denormal
	DSET	6, 0x12345678, 0x9abcdef0
1:	dadd	dr7, dr1, dr6
	DEXPECT	6, 0x12345678, 0x9abcdef0
	EXPECT_DCR dpsw, 0x00000080	; DCE
	mvfdc	depc, r9
	mov.l	#1b, r8
	EXPECT_REG r9, r8
	EXPECT_DCR decnt, 0x00010001
	DPSW_SET 0x400			; DEV, DDN = 0
	DSET	7, 0xbff00000, 0
2:	dsqrt	dr7, dr6		; held: DPSW and DEPC not updated
	EXPECT_DCR dpsw, 0x00000400
	mvfdc	depc, r9
	mov.l	#1b, r8
	EXPECT_REG r9, r8
	mov.l	#1, r9
	mvtdc	r9, decnt		; EHS = 0: release
	EXPECT_DCR decnt, 0x00000001
3:	dsqrt	dr7, dr6		; DEV = 1: invalid, dest unchanged
	DEXPECT	6, 0x12345678, 0x9abcdef0
	EXPECT_DCR dpsw, 0x00000404	; DCV, no DFV (enabled)
	mvfdc	depc, r9
	mov.l	#3b, r8
	EXPECT_REG r9, r8
	mov.l	#1, r9
	mvtdc	r9, decnt
	DPSW_SET 0x100

	; mvtdc: cause flags can only be cleared, DFS follows the flags,
	; DCMR keeps RES only, DEPC is read-only
	ddiv	dr3, dr1, dr6		; DCX
	DPSW_SET 0x1fc			; 1 written to the cause flags
	EXPECT_DCR dpsw, 0x00000140
	DPSW_SET 0x04000100		; DFV
	EXPECT_DCR dpsw, 0x84000100
	DPSW_SET 0x100
	mov.l	#0xffffffff, r9
	mvtdc	r9, dcmr
	EXPECT_DCR dcmr, 1
	mvfdc	depc, r4
	mvtdc	r9, depc
	mvfdc	depc, r9
	EXPECT_REG r9, r4

	; dpushm / dpopm .d: the first register at the lowest address
	dpushm.d dr1-dr3
	mov.l	4[r0], r9
	EXPECT	r9, 0x3ff00000		; dr1
	mov.l	12[r0], r9
	EXPECT	r9, 0x40000000		; dr2
	mov.l	20[r0], r9
	EXPECT	r9, 0x40080000		; dr3
	DSET	1, 0, 0
	DSET	2, 0, 0
	DSET	3, 0, 0
	dpopm.d	dr1-dr3
	DEXPECT	1, 0x3ff00000, 0
	DEXPECT	2, 0x40000000, 0
	DEXPECT	3, 0x40080000, 0

	; .l: dpsw, dcmr, decnt, depc upwards; dpopm.l writes them as they are
	dpushm.l dpsw-depc
	mov.l	[r0], r9
	EXPECT	r9, 0x00000100
	mov.l	4[r0], r9
	EXPECT	r9, 1
	mov.l	8[r0], r9
	EXPECT	r9, 1
	add	#16, r0
	mov.l	#0x00000144, r9		; DCX and DCV through dpopm
	push.l	r9
	dpopm.l	dpsw-dpsw
	EXPECT_DCR dpsw, 0x00000144
	mov.l	#0x00000005, r9
	push.l	r9
	dpopm.l	dcmr-dcmr
	EXPECT_DCR dcmr, 1
	DPSW_SET 0x100

	mov.l	#0, r1
	rts

	.bss
	.balign	8
_buf:	.space	2048 + 16
