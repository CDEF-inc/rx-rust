; Compare match timer unit 0 (CMT0 / CMT1 at 0x88000, the same address on
; the RX62N and the RX72M): register read-back, CMCNT counting and holding,
; and the compare match period. Run on every core (the peripherals are the
; RX62N's in QEMU). The expected values are the RX72M board's: only CKS and
; CMIE of CMCR can be written (bit 7 reads as 1 after a compare match until
; CMCR is written), and a compare match comes every CMCOR + 1
; counts (CMT1, started with CMT0, is 1000 ahead of CMT0 with CMCOR = 99
; after ten compare matches).
	.include "harness.inc"

	.equ	PRCR, 0x803fe
	.equ	MSTPCRA, 0x80010
	.equ	CMT, 0x88000		; CMSTR0
	.equ	CMCR0, 2
	.equ	CMCNT0, 4
	.equ	CMCOR0, 6
	.equ	CMCR1, 8
	.equ	CMCNT1, 10
	.equ	CMCOR1, 12
	.equ	IR028, 0x8701c		; CMI0

; DELAY n: spin n times. Clobbers r3.
	.macro	DELAY n
	mov.l	#\n, r3
9997:	sub	#1, r3
	bne.b	9997b
	.endm

	.text
	.global	_main
_main:
	; The RX72M stops the CMT after reset (MSTPCRA.MSTPA15, write-protected
	; by PRCR); on the RX62N model these writes go nowhere.
	mov.l	#PRCR, r4
	mov.w	#0xa502, [r4]
	mov.l	#MSTPCRA, r4
	mov.l	[r4], r5
	and	#~0x8000, r5
	mov.l	r5, [r4]
	mov.l	#PRCR, r4
	mov.w	#0xa500, [r4]

	mov.l	#CMT, r10
	mov.w	#0, [r10]		; both channels stopped

	; --- read-back
	mov.w	#0x0043, CMCR0[r10]	; CMIE, CKS = PCLK/512
	mov.w	#0x0001, CMCR1[r10]	; CKS = PCLK/32
	movu.w	CMCR0[r10], r6
	EXPECT	r6, 0x0043
	movu.w	CMCR1[r10], r6
	EXPECT	r6, 0x0001
	mov.w	#0xffff, CMCR1[r10]	; only CKS and CMIE are kept
	movu.w	CMCR1[r10], r6
	EXPECT	r6, 0x0043
	mov.w	#0x0080, CMCR1[r10]	; bit 7 reads as 0 (RX72M)
	movu.w	CMCR1[r10], r6
	EXPECT	r6, 0x0000
	mov.w	#0x0001, CMCR1[r10]
	mov.w	#0x1234, CMCOR0[r10]
	movu.w	CMCOR0[r10], r6
	EXPECT	r6, 0x1234
	mov.w	#0x0567, CMCNT0[r10]	; stopped: the value stays
	DELAY	1000
	movu.w	CMCNT0[r10], r6
	EXPECT	r6, 0x0567
	movu.w	[r10], r6
	EXPECT	r6, 0

	; --- CMCNT counts while started and holds when stopped
	mov.w	#0xffff, CMCOR0[r10]
	mov.w	#0x0000, CMCR0[r10]	; PCLK/8, no interrupt
	mov.w	#0, CMCNT0[r10]
	mov.w	#1, [r10]
	movu.w	[r10], r6
	EXPECT	r6, 1
	DELAY	2000
	movu.w	CMCNT0[r10], r4
	DELAY	2000
	movu.w	CMCNT0[r10], r5
	mov.w	#0, [r10]
	sub	r4, r5, r6		; advanced
	mov.l	#0, r1
	cmp	#0, r6
	beq.b	1f
	mov.l	#1, r1
1:	mov.l	#1, r2
	bsr.a	_check
	movu.w	CMCNT0[r10], r4
	DELAY	2000
	movu.w	CMCNT0[r10], r5
	EXPECT_REG r5, r4		; held
	mov.l	#0, r1			; and not cleared by stopping
	cmp	#0, r4
	beq.b	1f
	mov.l	#1, r1
1:	mov.l	#1, r2
	bsr.a	_check

	; --- the compare match period, against CMT1 counting freely at the
	; same clock: CMT0 with CMCOR0 = 99, ten compare matches (IR028)
	mov.w	#99, CMCOR0[r10]
	mov.w	#0xffff, CMCOR1[r10]
	mov.w	#0x0043, CMCR0[r10]	; CMIE, PCLK/512
	mov.w	#0x0003, CMCR1[r10]	; PCLK/512
	mov.w	#0, CMCNT0[r10]
	mov.w	#0, CMCNT1[r10]
	mov.l	#IR028, r7
	mov.b	#0, [r7]
	mov.l	#0, r8			; compare matches seen
	mov.l	#20000000, r9		; time limit
	mov.w	#3, [r10]		; start both
2:	sub	#1, r9
	beq.b	3f
	movu.b	[r7], r6
	cmp	#0, r6
	beq.b	2b
	mov.b	#0, [r7]
	add	#1, r8
	cmp	#10, r8
	bne.b	2b
3:	mov.w	#0, [r10]		; stop both at once
	movu.w	CMCNT1[r10], r5
	movu.w	CMCNT0[r10], r4
	sub	r4, r5			; CMT1 - CMT0 = 10 periods, however
	EXPECT	r8, 10			; late the last match was seen
	EXPECT	r5, 1000

	; --- CMCR bit 7 reads as 1 after a compare match, with or without
	; CMIE, and stays so (also when stopped) until CMCR is written
	movu.w	CMCR0[r10], r6
	EXPECT	r6, 0x00c3
	movu.w	CMCR1[r10], r6		; CMT1 did not reach CMCOR1
	EXPECT	r6, 0x0003
	mov.w	#0x0043, CMCR0[r10]
	movu.w	CMCR0[r10], r6
	EXPECT	r6, 0x0043
	mov.w	#9, CMCOR1[r10]
	mov.w	#0, CMCNT1[r10]
	mov.w	#2, [r10]		; CMT1 alone, no CMIE
	mov.l	#20000000, r9
4:	sub	#1, r9
	beq.b	5f
	movu.w	CMCR1[r10], r6
	btst	#7, r6
	beq.b	4b
5:	mov.w	#0, [r10]
	movu.w	CMCR1[r10], r6
	EXPECT	r6, 0x0083
	mov.w	#0x0003, CMCR1[r10]
	movu.w	CMCR1[r10], r6
	EXPECT	r6, 0x0003
	rts
