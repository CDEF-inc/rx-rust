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
	; mov.<size> has no size 3: the other codes next to movco / movli
	TRAPS	1, .byte 0xfd, 0x23, 0x65	; mov rs, [rd+] with size 3
	TRAPS	1, .byte 0xfd, 0x2b, 0x65	; mov [rs+], rd with size 3

	mov.l	#0, r1
	rts

	.data
_word:	.long	0
