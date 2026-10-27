; GNU as syntax without a size: mov / movu / push / sstr / suntil / swhile /
; rmpa take their default size, bra / bsr Rn are .L, and bra / bsr /
; b<cond> to a label get the form the distance needs (bra.s / .b / .w / .a,
; bsr.w / .a, beq / bne .s / .b / .w, b<cond>.b, and a b<!cond> around a
; bra.w or bra.a when b<cond> cannot reach). An immediate A - B with B
; undefined becomes the R_RX_SYM / R_RX_OPsub / R_RX_ABS32 relocations.
; Checked with scripts/mc-diff.sh file.
	.global	start, g
start:
	mov	r1, r2
	mov	#1, r2
	mov	#300, r2
	mov	4[r1], r2
	mov	r1, [r2]
	mov	[r1+], r2
	mov	r1, [-r2]
	mov	[r1, r2], r3
	mov	[r1], [r2]
	movu	[r1], r2
	movu	2[r1], r2
	push	r1
	sstr
	suntil
	swhile
	rmpa
	bra	r1
	bsr	r1

; Forward and backward branches at the edges of each form.
	bra	1f
1:	bra	2f
	.space	8, 3
2:	bra	3f
	.space	9, 3
3:	bra	4f
	.space	125, 3
4:	bra	5f
	.space	126, 3
5:	bra	6f
	.space	32764, 3
6:	bra	7f
	.space	32765, 3
7:	bra	6b
	bra	5b
	beq	8f
	.space	8, 3
8:	bne	9f
	.space	9, 3
9:	bz	10f
	.space	125, 3
10:	bnz	11f
	.space	126, 3
11:	beq	12f
	.space	32764, 3
12:	bne	13f
	.space	32765, 3
13:	bgt	14f
	.space	125, 3
14:	bc	15f
	.space	126, 3
15:	bgeu	16f
	.space	32762, 3
16:	ble	17f
	.space	32763, 3
17:	bnc	12b
	bpz	5b
	bn	1b
	bsr	18f
	.space	32764, 3
18:	bsr	19f
	.space	32765, 3
19:	bsr	start

; Targets the assembler cannot resolve.
	bra	far
	bsr	far
	beq	far
	bne	far
	bgtu	far
	bleu	far
	bge	far
	blt	far
	bo	far
	bno	far
	bra	g
	bgt	g
	nop
g:	nop

; Differences of symbols that are not defined here.
	mov	#(_e - _s), r3
	mov.l	#(_e - _s + 4), r3
	add	#(_e - _s), r3
	cmp	#(_e - _s), r3
	mov.l	#(_e - _s), [r1]
	mov	#(_e - here), r3
here:	nop
	.long	_e - _s
