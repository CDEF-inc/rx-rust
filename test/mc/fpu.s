# Single-precision FPU instructions (RXv1, plus the RXv2 additions); checked
# with scripts/mc-diff.sh lines.
fadd r1, r2
fsub r15, r0
fmul r3, r4
fdiv r5, r6
fcmp r7, r8
ftoi r9, r10
round r11, r12
itof r13, r14
fadd [r1], r2
fadd 0[r1], r2
fadd [r1].l, r2
fadd 4[r1], r2
fadd 4[r1].L, r2
fadd 1020[r15], r0
fadd 1024[r1], r2
fadd 262140[r1], r2
fsub [r2], r3
fsub 8[r2], r3
fsub 2000[r2], r3
fmul [r3], r4
fmul 12[r3], r4
fmul 4096[r3], r4
fdiv [r4], r5
fdiv 16[r4], r5
fdiv 65532[r4], r5
fcmp [r5], r6
fcmp 20[r5], r6
fcmp 1028[r5], r6
ftoi [r6], r7
ftoi 24[r6], r7
ftoi 8000[r6], r7
round [r7], r8
round 28[r7], r8
round 4000[r7], r8
fadd #0, r1
fadd #0x3f800000, r1
fsub #0xbf800000, r15
fmul #0x40000000, r2
fdiv #0x7f7fffff, r3
fcmp #0x80000000, r4
fcmp #1, r5
# RXv2
fadd r1, r2, r3
fsub r4, r5, r6
fmul r7, r8, r9
fadd r15, r0, r14
fsqrt r1, r2
fsqrt [r3], r4
fsqrt 4[r3], r4
fsqrt 2048[r3], r4
ftou r5, r6
ftou [r7], r8
ftou 252[r7], r8
ftou 256[r7], r8
utof r9, r10
utof r0, r15
