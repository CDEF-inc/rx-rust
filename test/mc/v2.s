# stz / stnz and the RXv2 exclusive access instructions; checked with
# scripts/mc-diff.sh lines.
stz #0, r1
stz #127, r2
stz #-128, r3
stz #128, r4
stz #-32768, r5
stz #0x7fffff, r6
stz #0x800000, r7
stz #0xffffffff, r8
stnz #1, r9
stnz #-1, r10
stnz #1000, r11
stnz #0x12345678, r12
# RXv2
stz r1, r2
stnz r3, r4
stz r15, r0
movli [r1], r2
movli [r15], r0
movco r3, [r4]
movco r0, [r15]
