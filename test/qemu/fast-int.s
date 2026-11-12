; The fast interrupt (ICU FIR: CMT0's CMI0, vector 28, as the fast
; interrupt): like a normal interrupt it is accepted only above PSW.IPL,
; and the handler runs at the interrupt's priority level (IPR, here 5),
; not at 15 (RXv3e manual 5.5.10; the same on RXv1-RXv3). BPC and BPSW hold
; the return address and PSW, and RTFI returns.
	.include "harness.inc"

	.equ	PRCR, 0x803fe
	.equ	MSTPCRA, 0x80010
	.equ	CMT, 0x88000		; CMSTR0
	.equ	CMCR0, 2
	.equ	CMCNT0, 4
	.equ	CMCOR0, 6
	.equ	IER03, 0x87203		; IEN4: vector 28
	.equ	IPR04, 0x87304		; CMI0's priority
	.equ	FIR, 0x872f0		; FIEN (bit 15), FVCT

; DELAY n: spin n times. Clobbers r3.
	.macro	DELAY n
	mov.l	#\n, r3
9997:	sub	#1, r3
	bne.b	9997b
	.endm

	.text
	.global	_main
_main:
	; CMT unit 0 out of module stop (RX72M; nothing on the RX62N model)
	mov.l	#PRCR, r4
	mov.w	#0xa502, [r4]
	mov.l	#MSTPCRA, r4
	mov.l	[r4], r5
	and	#~0x8000, r5
	mov.l	r5, [r4]
	mov.l	#PRCR, r4
	mov.w	#0xa500, [r4]

	mov.l	#CMT, r10
	mov.w	#0, [r10]
	mov.w	#0x0040, CMCR0[r10]	; CMIE, PCLK / 8
	mov.w	#99, CMCOR0[r10]
	mov.w	#0, CMCNT0[r10]
	mov.l	#IPR04, r4
	mov.b	#5, [r4]
	mov.l	#IER03, r4
	bset	#4, [r4].b
	mov.l	#FIR, r4
	mov.w	#0x801c, [r4]		; FIEN, FVCT = 28
	mvtc	#_fast, fintv
	mov.l	#0, r7			; fast interrupts taken
	mov.l	#0, r8			; the IPL the handler saw

	; PSW.I = 1, IPL 5: a level-5 interrupt is not accepted
	mvtc	#0x05010000, psw
	mov.w	#1, [r10]		; start CMT0
	DELAY	200000
	EXPECT	r7, 0

	; IPL 4: accepted, and the handler runs at IPL 5
	mvtipl	#4
	DELAY	200000
	EXPECT	r7, 1
	EXPECT	r8, 5
	mvfc	psw, r9			; RTFI restored IPL 4
	shlr	#24, r9
	and	#0x1f, r9
	EXPECT	r9, 4

	clrpsw	i
	mov.l	#FIR, r4
	mov.w	#0, [r4]
	rts

; The handler: record PSW.IPL, stop CMT0, return with RTFI.
_fast:
	mvfc	psw, r8
	shlr	#24, r8
	and	#0x1f, r8
	add	#1, r7
	mov.w	#0, [r10]
	rtfi
