; Instructions of an ISA or unit the core does not have raise the undefined
; instruction exception. V2 and FPU are 1 when the core has RXv2 and the
; FPU (set by run-qemu-test.sh).
	.include "harness.inc"

	.text
	.global	_main
_main:
	mov.l	#_resume, r11
	mov.l	#0x3f800000, r4		; 1.0
	mov.l	#0x40000000, r5		; 2.0
	TRAPS	1 - FPU, fadd r4, r5
	TRAPS	1 - FPU, fsub #0x3f800000, r5
	TRAPS	1 - FPU, fmul [r0], r5
	TRAPS	1 - FPU, fdiv r4, r5
	TRAPS	1 - FPU, fcmp r4, r5
	TRAPS	1 - FPU, ftoi r4, r5
	TRAPS	1 - FPU, round r4, r5
	TRAPS	1 - FPU, itof r4, r5
	; integer instructions never trap
	TRAPS	0, add r4, r5
	TRAPS	0, mvfc fpsw, r5

	; RXv2 instructions
	mov.l	#0x3f800000, r4
	mov.l	#_word, r6
	TRAPS	1 - V2, stz r4, r5
	TRAPS	1 - V2, stnz r4, r5
	TRAPS	1 - V2, movli [r6], r5
	TRAPS	1 - V2, movco r5, [r6]
	TRAPS	1 - (V2 & FPU), fadd r4, r4, r5
	TRAPS	1 - (V2 & FPU), fsub r4, r4, r5
	TRAPS	1 - (V2 & FPU), fmul r4, r4, r5
	TRAPS	1 - (V2 & FPU), fsqrt r4, r5
	TRAPS	1 - (V2 & FPU), ftou r4, r5
	TRAPS	1 - (V2 & FPU), utof r4, r5
	; RXv2 DSP: A1, the shift of mvfac* and the new instructions
	TRAPS	0, mulhi r4, r5, a0
	TRAPS	1 - V2, mulhi r4, r5, a1
	TRAPS	1 - V2, mullh r4, r5, a0
	TRAPS	1 - V2, emaca r4, r5, a0
	TRAPS	1 - V2, msbhi r4, r5, a0
	TRAPS	0, mvfachi #0, a0, r5
	TRAPS	1 - V2, mvfachi #1, a0, r5
	TRAPS	1 - V2, mvfaclo #0, a0, r5
	TRAPS	1 - V2, mvtacgu r4, a0
	TRAPS	0, racw #2, a0
	TRAPS	1 - V2, racl #1, a0
	TRAPS	1 - V2, rdacw #1, a0
	TRAPS	1, .byte 0xfd, 0x1e, 0x45	; mvfachi with shift 3
	; RXv3 instructions (save / rstr with the default 16 banks)
	TRAPS	1 - V3, bfmov #0, #8, #8, r4, r5
	TRAPS	1 - V3, bfmovz #0, #8, #8, r4, r5
	TRAPS	1 - V3, xor r4, r4, r5
	TRAPS	0, xor r4, r5
	TRAPS	1 - V3, save #0
	TRAPS	1 - V3, rstr #0		; restores what save #0 stored
	; double-precision FPU (the pushm / popm pair keeps the stack balanced)
	TRAPS	1 - DFPU, dadd dr0, dr0, dr1
	TRAPS	1 - DFPU, dmov.l r4, drl1
	TRAPS	1 - DFPU, dmov.l #0, drl1
	TRAPS	1 - DFPU, mvfdc dpsw, r5
	TRAPS	1 - DFPU, itod r4, dr1
	TRAPS	1 - DFPU, dpushm.d dr0-dr0
	TRAPS	1 - DFPU, dpopm.d dr0-dr0
	TRAPS	1 - DFPU, mvfdr
	TRAPS	1, .byte 0xf8, 0x43, 0, 0, 0, 0	; mov #imm, [r4] with size 3
	; RXv3e: the new instructions trap on the other cores (on rxv3e they
	; are checked by rxv3e-insns), and PSW.IPL[4] is RXv3e only
	.if	V3E == 0
	TRAPS	1, .byte 0xfd, 0x75, 0x00, 0x00		; trcall #0
	TRAPS	1, .byte 0x7f, 0x97			; trrte
	TRAPS	1, .byte 0xfd, 0x5a, 0xe9		; trmvfc trctb, r9
	TRAPS	1, .byte 0xfd, 0x58, 0x4e		; trmvtc r4, trctb
	TRAPS	1, .byte 0xfd, 0x75, 0x14, 0x00		; int r4
	TRAPS	1, .byte 0x75, 0x70, 0x14		; mvtipl #20
	.endif
	mvfc	psw, r6
	mov.l	r6, r5
	or	#0x14000000, r5			; IPL 20
	mvtc	r5, psw
	mvfc	psw, r1
	mvtc	r6, psw
	shlr	#24, r1
	mov.l	#4 + 16 * V3E, r2
	bsr.a	_check
	; mov.<size> has no size 3: the other codes next to movco / movli
	TRAPS	1, .byte 0xfd, 0x23, 0x65	; mov rs, [rd+] with size 3
	TRAPS	1, .byte 0xfd, 0x2b, 0x65	; mov [rs+], rd with size 3

	mov.l	#0, r1
	rts

	.data
_word:	.long	0
