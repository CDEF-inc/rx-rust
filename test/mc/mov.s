# MOV / MOVU addressing forms beyond Rn, #imm and dsp[Rn]. One instruction
# per line; checked with scripts/mc-diff.sh lines. The encodings also agree
# with Renesas CC-RX V3.08, except where GNU as picks a shorter immediate
# field for the same value (mov.w #0xffff is #-1 in 8 bits).

# --- post-increment / pre-decrement
mov.b r1, [r2+]
mov.w r1, [r2+]
mov.l r1, [r2+]
mov.b r0, [r15+]
mov.b r1, [-r2]
mov.w r1, [-r2]
mov.l r1, [-r2]
mov.b [r2+], r1
mov.w [r2+], r1
mov.l [r2+], r1
mov.l [r1+], r2
mov.b [-r2], r1
mov.w [-r2], r1
mov.l [-r2], r1
movu.b [r2+], r1
movu.w [r2+], r1
movu.b [-r2], r1
movu.w [-r2], r1

# --- register-indexed: [Ri, Rb] is Rb + Ri * size
mov.b [r3, r4], r5
mov.w [r3, r4], r5
mov.l [r3, r4], r5
mov.l [r10, r11], r12
mov.l [r0, r0], r0
mov.b r5, [r3, r4]
mov.w r5, [r3, r4]
mov.l r5, [r3, r4]
movu.b [r3, r4], r5
movu.w [r3, r4], r5

# --- immediate to memory: the dsp:5 form (R0-R7, #0-255) where GNU as
# uses it, otherwise [Rd] / dsp:8 / dsp:16 with the shortest immediate
mov.b #5, 3[r2]
mov.w #5, 6[r2]
mov.l #5, 12[r2]
mov.l #5, 0[r2]
mov.b #200, 31[r7]
mov.w #200, 62[r7]
mov.l #200, 124[r7]
mov.l #255, 124[r2]
mov.w #128, 4[r2]
mov.b #200, 32[r2]
mov.b #5, [r2]
mov.w #5, [r2]
mov.l #5, [r2]
mov.b #5, [r9]
mov.b #5, 0[r9]
mov.b #-1, [r9]
mov.b #-1, 4[r2]
mov.b #255, 200[r9]
mov.b #5, 1000[r9]
mov.b #1, 4[r14]
mov.w #-5, [r9]
mov.w #-1, 4[r2]
mov.w #0xffff, 4[r2]
mov.w #0xffff, [r9]
mov.w #-129, 4[r2]
mov.w #1000, [r9]
mov.w #-1000, 20[r9]
mov.l #0, [r14]
mov.l #-5, [r9]
mov.l #-1, 124[r2]
mov.l #0xffffffff, [r9]
mov.l #256, 124[r2]
mov.l #300, [r2]
mov.l #1000, 20[r9]
mov.l #0x7fffff, [r2]
mov.l #-0x800000, [r2]
mov.l #0x800000, [r2]
mov.l #100000, [r9]
mov.l #200, 2000[r2]
mov.l #0x12345678, 3000[r9]

# --- memory to memory (a zero displacement is dropped)
mov.b [r1], [r2]
mov.w [r1], [r2]
mov.l [r1], [r2]
mov.l 0[r1], 0[r2]
mov.l [r1], 0[r9]
mov.l 4[r1], [r2]
mov.l [r1], 8[r2]
mov.b 3[r1], 1000[r2]
mov.w 510[r1], 2[r2]
mov.l 400[r1], 80000[r2]
