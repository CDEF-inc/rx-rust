; RXv3e: the CPU side of the Protect Zone function (RXv3e manual,
; R01US0745EJ0101: 1.2, 1.3.2, 3 and 5). Run on rxv3e.
	.include "harness.inc"

; EXPECT_CR cr, want (mvfc into r9)
	.macro	EXPECT_CR cr, want
	mvfc	\cr, r9
	EXPECT	r9, \want
	.endm
	.macro	EXPECT_TRCR cr, want
	trmvfc	\cr, r9
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
	EXPECT_CR wid, 0
	EXPECT_CR ivc, 0
	EXPECT_CR wpri, 0
	EXPECT_TRCR trctb, 0x80000000
	EXPECT_TRCR wintb, 0x80000000

	; PSW.IPL has 5 bits
	mvtipl	#20
	EXPECT_IPL 20
	mvfc	psw, r5
	or	#0x1f000000, r5
	mvtc	r5, psw
	EXPECT_IPL 31
	mvtipl	#0

	; the writable bits of WPRI, IVC and WID (only BWID)
	mov.l	#0xffffffff, r4
	mvtc	r4, wpri
	EXPECT_CR wpri, 0x1f000000
	mvtc	r4, ivc
	EXPECT_CR ivc, 0x110300ff
	mov.l	#0x80030203, r4
	mvtc	r4, wid
	EXPECT_CR wid, 0x00000200
	mov.l	#0x300, r4		; BWID 3 is reserved: not written
	mvtc	r4, wid
	EXPECT_CR wid, 0x00000200
	mov.l	#0, r4
	mvtc	r4, ivc
	mvtc	r4, wid

	; trmvtc / trmvfc with BWID = 0: the root registers
	trmvtc	#_intb_root, intb
	mvfc	intb, r9
	EXPECT	r9, _intb_root
	trmvtc	#_trcall_table, trctb
	EXPECT_TRCR trctb, _trcall_table
	mov.l	#0x12345678, r4
	trmvtc	r4, wintb
	EXPECT_TRCR wintb, 0x12345678
	; with BWID = 2, those of the non-secure world
	mov.l	#0x200, r4
	mvtc	r4, wid
	trmvtc	#_intb_ns, intb
	mvfc	intb, r9
	EXPECT	r9, _intb_root		; the root INTB is unchanged
	EXPECT_TRCR intb, _intb_ns
	trmvtc	#0x1f000003, bpsw
	EXPECT_TRCR bpsw, 0x0f000003	; BPSW_NS bit 28 is always 0
	trmvtc	#0x110300ff, ivc
	EXPECT_TRCR ivc, 0x000300ff	; IVC_NS.NSIC / EXCSE are 0
	trmvtc	#_ns_stack + 256, isp
	trmvtc	#0xffffff80, extb	; the harness's fixed vectors
	mov.l	#0, r4
	trmvtc	r4, ivc
	; with BWID = 1, those of the secure world
	mov.l	#0x100, r4
	mvtc	r4, wid
	trmvtc	#0x1f000003, bpsw
	EXPECT_TRCR bpsw, 0x1f000003
	mov.l	#0, r4
	mvtc	r4, wid

	; int Rs: the lower 8 bits of Rs
	mov.l	#0x105, r4
	mov.l	#0, r13
	int	r4
	EXPECT	r13, 6

	; trcall in the root world: a trap through TRCTB, back with trrte
	mov.l	#0, r13
	trcall	#0
	EXPECT	r13, 5
	EXPECT_CR wid, 0

	; into the non-secure world with trrte (BWID = 2; PSW and PC on the
	; root stack); FINTV and WPRI of the root world are set to see that
	; they read as 0 there
	mvtc	#0x1234, fintv
	mov.l	#0x05000000, r4
	mvtc	r4, wpri
	mov.l	#0x200, r4
	mvtc	r4, wid
	mov.l	r0, r10			; the root SP, for later
	mov.l	#0, r4			; PSW: supervisor, I = 0, IPL 0
	push.l	r4
	mov.l	#_in_ns, r4
	push.l	r4
	trrte
_in_ns:
	EXPECT_CR wid, 2		; BWID reads as 0 here
	EXPECT	r0, _ns_stack + 256	; ISP_NS
	mvfc	intb, r9
	EXPECT	r9, _intb_ns
	mvfc	fintv, r9
	EXPECT	r9, 0
	EXPECT_CR wpri, 0
	mov.l	#0x55, r4
	mvtc	r4, wid			; ignored
	EXPECT_CR wid, 2
	; bit 28 of BPSW_NS is always 0, also for mvtc and popc
	mov.l	#0x1f000005, r4
	mvtc	r4, bpsw
	mvfc	bpsw, r9
	EXPECT	r9, 0x0f000005
	push.l	r4
	popc	bpsw
	mvfc	bpsw, r9
	EXPECT	r9, 0x0f000005
	; the root-only instructions are undefined (the exception goes through
	; EXTB_NS with ISP_NS)
	TRAPS	1, trmvfc trctb, r9
	TRAPS	1, trmvtc r4, trctb
	TRAPS	1, trrte
	EXPECT	r0, _ns_stack + 256
	; IPL[4] is always 0 here
	mvtipl	#20
	EXPECT_IPL 4
	mvfc	psw, r5
	or	#0x1f000000, r5
	mvtc	r5, psw
	EXPECT_IPL 15
	mvtipl	#0
	; int uses INTB_NS
	mov.l	#1, r4
	mov.l	#0, r13
	int	r4
	EXPECT	r13, 6
	; back to the root world with trcall #1, with PSW.I = 1 (WID.BWI)
	setpsw	i
	trcall	#1
	bra.a	_fail			; not reached

_back_in_root:
	EXPECT_CR wid, 0x80000200	; BWI = 1, BWID = 2, WID = 0
	EXPECT_REG r0, r10		; ISP_TR
	mvfc	intb, r9
	EXPECT	r9, _intb_root
	mvfc	fintv, r9
	EXPECT	r9, 0x1234
	mov.l	#0, r4
	mvtc	r4, wid
	mvtc	r4, wpri

	; user mode in the root world: trcall and trmvfc are privileged, and
	; IVC.UIR = 01 restricts int to #0-#15 (no int Rs)
	mov.l	#0x10000, r4
	mvtc	r4, ivc
	USER_MODE
	TRAPS	4, trcall #0
	TRAPS	4, trmvfc trctb, r9
	mov.l	#3, r4
	TRAPS	4, int r4
	TRAPS	4, int #20
	TRAPS	6, int #3		; allowed: the handler sets r13 = 6
	SUPERVISOR_MODE

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
	trrte

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
