# RXv3 bit-field move and register bank save / restore; checked with
# scripts/mc-diff.sh lines.
bfmov #0, #0, #1, r1, r2
bfmov #4, #8, #8, r3, r4
bfmov #0, #16, #16, r5, r6
bfmov #31, #0, #1, r7, r8
bfmov #0, #31, #1, r9, r10
bfmov #8, #0, #24, r15, r0
bfmovz #0, #0, #1, r1, r2
bfmovz #3, #5, #7, r11, r12
bfmovz #16, #0, #16, r13, r14
save r1
save r15
save #0
save #255
rstr r2
rstr r0
rstr #1
rstr #200
