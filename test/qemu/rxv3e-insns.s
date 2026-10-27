; RXv3e: the CPU side of the Protect Zone function (RXv3e manual,
; R01US0745EJ0101: 1.2, 1.3.2, 3 and 5). Run on rxv3e. llvm-mc has no
; RXv3e instructions yet, so they are written as bytes (manual section 4).
	.include "harness.inc"

; control register numbers
	.equ	IVC, 5
	.equ	BPSW, 8
	.equ	ISP, 10
	.equ	FINTV, 11
	.equ	INTB, 12
	.equ	EXTB, 13
	.equ	WPRI, 14		; mvtc / mvfc
	.equ	TRCTB, 14		; trmvtc / trmvfc
	.equ	WID, 15			; mvtc / mvfc
	.equ	WINTB, 15		; trmvtc / trmvfc

	.macro	TRCALL n
	.byte	0xfd, 0x75, 0x00, \n
	.endm
	.macro	TRRTE
	.byte	0x7f, 0x97
	.endm
	.macro	TRMVFC cr, rd
	.byte	0xfd, 0x5a, (\cr << 4) | \rd
	.endm
	.macro	TRMVTC rs, cr
	.byte	0xfd, 0x58, (\rs << 4) | \cr
	.endm
	.macro	TRMVTC_I imm, cr	; #IMM:32
	.byte	0xfd, 0x73, 0x20 | \cr
	.long	\imm
	.endm
	.macro	INT_R rs
	.byte	0xfd, 0x75, 0x10 | \rs, 0x00
	.endm
	.macro	MVTIPL n		; #IMM:5
	.byte	0x75, 0x70, \n
	.endm
	.macro	MVTC_R rs, cr		; mvtc to IVC / WPRI / WID
	.byte	0xfd, 0x68, (\rs << 4) | \cr
	.endm
	.macro	MVFC_R cr, rd
	.byte	0xfd, 0x6a, (\cr << 4) | \rd
	.endm

; EXPECT_CR cr, want (mvfc into r9)
	.macro	EXPECT_CR cr, want
	MVFC_R	\cr, 9
	EXPECT	r9, \want
	.endm
	.macro	EXPECT_TRCR cr, want
	TRMVFC	\cr, 9
	EXPECT	r9, \want
	.endm
; EXPECT_IPL want
	.macro	EXPECT_IPL want
	mvfc	psw, r9
	shlr	#24, r9
	EXPECT	r9, \want
	.endm

	.text
	.global	_main
_main:
	mov.l	#_resume, r11

	; reset values
	EXPECT_CR WID, 0
	EXPECT_CR IVC, 0
	EXPECT_CR WPRI, 0
	EXPECT_TRCR TRCTB, 0x80000000
	EXPECT_TRCR WINTB, 0x80000000

	; PSW.IPL has 5 bits
	MVTIPL	20
	EXPECT_IPL 20
	mvfc	psw, r5
	or	#0x1f000000, r5
	mvtc	r5, psw
	EXPECT_IPL 31
	MVTIPL	0

	; the writable bits of WPRI, IVC and WID (only BWID)
	mov.l	#0xffffffff, r4
	MVTC_R	4, WPRI
	EXPECT_CR WPRI, 0x1f000000
	MVTC_R	4, IVC
	EXPECT_CR IVC, 0x110300ff
	mov.l	#0x80030203, r4
	MVTC_R	4, WID
	EXPECT_CR WID, 0x00000200
	mov.l	#0, r4
	MVTC_R	4, IVC
	MVTC_R	4, WID

	; trmvtc / trmvfc with BWID = 0: the root registers
	TRMVTC_I _intb_root, INTB
	mvfc	intb, r9
	EXPECT	r9, _intb_root
	TRMVTC_I _trcall_table, TRCTB
	EXPECT_TRCR TRCTB, _trcall_table
	mov.l	#0x12345678, r4
	TRMVTC	4, WINTB
	EXPECT_TRCR WINTB, 0x12345678
	; with BWID = 2, those of the non-secure world
	mov.l	#0x200, r4
	MVTC_R	4, WID
	TRMVTC_I _intb_ns, INTB
	mvfc	intb, r9
	EXPECT	r9, _intb_root		; the root INTB is unchanged
	EXPECT_TRCR INTB, _intb_ns
	TRMVTC_I 0x1f000003, BPSW
	EXPECT_TRCR BPSW, 0x0f000003	; BPSW_NS bit 28 is always 0
	TRMVTC_I 0x110300ff, IVC
	EXPECT_TRCR IVC, 0x000300ff	; IVC_NS.NSIC / EXCSE are 0
	TRMVTC_I _ns_stack + 256, ISP
	TRMVTC_I 0xffffff80, EXTB	; the harness's fixed vectors
	mov.l	#0, r4
	TRMVTC	4, IVC
	; with BWID = 1, those of the secure world
	mov.l	#0x100, r4
	MVTC_R	4, WID
	TRMVTC_I 0x1f000003, BPSW
	EXPECT_TRCR BPSW, 0x1f000003
	mov.l	#0, r4
	MVTC_R	4, WID

	; int Rs: the lower 8 bits of Rs
	mov.l	#0x105, r4
	mov.l	#0, r13
	INT_R	4
	EXPECT	r13, 6

	; trcall in the root world: a trap through TRCTB, back with trrte
	mov.l	#0, r13
	TRCALL	0
	EXPECT	r13, 5
	EXPECT_CR WID, 0

	; into the non-secure world with trrte (BWID = 2; PSW and PC on the
	; root stack); FINTV and WPRI of the root world are set to see that
	; they read as 0 there
	mvtc	#0x1234, fintv
	mov.l	#0x05000000, r4
	MVTC_R	4, WPRI
	mov.l	#0x200, r4
	MVTC_R	4, WID
	mov.l	r0, r10			; the root SP, for later
	mov.l	#0, r4			; PSW: supervisor, I = 0, IPL 0
	push.l	r4
	mov.l	#_in_ns, r4
	push.l	r4
	TRRTE
_in_ns:
	EXPECT_CR WID, 2		; BWID reads as 0 here
	EXPECT	r0, _ns_stack + 256	; ISP_NS
	mvfc	intb, r9
	EXPECT	r9, _intb_ns
	mvfc	fintv, r9
	EXPECT	r9, 0
	EXPECT_CR WPRI, 0
	mov.l	#0x55, r4
	MVTC_R	4, WID			; ignored
	EXPECT_CR WID, 2
	; the root-only instructions are undefined (the exception goes through
	; EXTB_NS with ISP_NS)
	TRAPS	1, TRMVFC TRCTB, 9
	TRAPS	1, TRMVTC 4, TRCTB
	TRAPS	1, TRRTE
	EXPECT	r0, _ns_stack + 256
	; IPL[4] is always 0 here
	MVTIPL	20
	EXPECT_IPL 4
	mvfc	psw, r5
	or	#0x1f000000, r5
	mvtc	r5, psw
	EXPECT_IPL 15
	MVTIPL	0
	; int uses INTB_NS
	mov.l	#1, r4
	mov.l	#0, r13
	INT_R	4
	EXPECT	r13, 6
	; back to the root world with trcall #1, with PSW.I = 1 (WID.BWI)
	setpsw	i
	TRCALL	1
	bra.a	_fail			; not reached

_back_in_root:
	EXPECT_CR WID, 0x80000200	; BWI = 1, BWID = 2, WID = 0
	EXPECT_REG r0, r10		; ISP_TR
	mvfc	intb, r9
	EXPECT	r9, _intb_root
	mvfc	fintv, r9
	EXPECT	r9, 0x1234
	mov.l	#0, r4
	MVTC_R	4, WID
	MVTC_R	4, WPRI

	; user mode in the root world: trcall and trmvfc are privileged, and
	; IVC.UIR = 01 restricts int to #0-#15 (no int Rs)
	mov.l	#0x10000, r4
	MVTC_R	4, IVC
	mvfc	psw, r4
	or	#0x00100000, r4		; PM = 1
	push.l	r4
	mov.l	#1f, r4
	push.l	r4
	rte
1:	TRAPS	4, TRCALL 0
	TRAPS	4, TRMVFC TRCTB, 9
	mov.l	#3, r4
	TRAPS	4, INT_R 4
	TRAPS	4, int #20
	TRAPS	6, int #3		; allowed: the handler sets r13 = 6

	mov.l	#0, r1
	rts

_fail:
	mov.l	#0xdead, r1
	mov.l	#0, r2
	bsr.a	_check
	bra.b	_fail

; int handler: r13 = 6
_int_handler:
	mov.l	#6, r13
	rte

; trcall #0: r13 = 5, back with trrte
_trcall0:
	mov.l	#5, r13
	TRRTE

; trcall #1 (from the non-secure world): drop the saved PC / PSW and
; continue in the root world
_trcall1:
	add	#8, r0
	bra.a	_back_in_root

	.data
	.balign	4
_trcall_table:
	.long	_trcall0, _trcall1
_intb_root:
	.long	0, 0, 0, _int_handler, 0, _int_handler
_intb_ns:
	.long	0, _int_handler

	.bss
	.balign	4
_ns_stack:
	.space	256
