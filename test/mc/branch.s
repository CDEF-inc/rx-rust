; Branches to local labels (resolved by the assembler) and to an undefined
; symbol (relocations). Checked with scripts/mc-diff.sh file.
	.global	start
start:
	nop
	bra.s	.Ls1		; 3..10 bytes ahead
	nop
	nop
.Ls1:
	beq.s	.Ls2
	nop
	nop
	nop
	nop
	nop
	nop
	nop
.Ls2:
	bne.s	.Ls3
	nop
	nop
.Ls3:
	bra.b	start		; backward
	bra.b	.Lfwd
	beq.b	.Lfwd
	bne.b	.Lfwd
	bgeu.b	.Lfwd
	bltu.b	.Lfwd
	bgtu.b	.Lfwd
	bleu.b	.Lfwd
	bpz.b	.Lfwd
	bn.b	.Lfwd
	bge.b	.Lfwd
	blt.b	.Lfwd
	bgt.b	.Lfwd
	ble.b	.Lfwd
	bo.b	.Lfwd
	bno.b	.Lfwd
	bc.b	.Lfwd
	bnc.b	.Lfwd
	bz.b	.Lfwd
	bnz.b	.Lfwd
	bra.w	start
	bra.w	.Lfwd
	beq.w	.Lfwd
	bne.w	.Lfwd
	bsr.w	start
	bra.a	start
	bra.a	.Lfwd
	bsr.a	.Lfwd
.Lfwd:
	rts

	; References to an undefined symbol.
	bra.a	ext
	bsr.a	ext
	bra.w	ext
	bsr.w	ext
	bra.b	ext
	beq.b	ext
	rts
