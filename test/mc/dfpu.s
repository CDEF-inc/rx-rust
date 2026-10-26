# RXv3 double-precision FPU instructions; checked with scripts/mc-diff.sh
# lines.
dadd dr1, dr2, dr3
dsub dr15, dr0, dr7
dmul dr4, dr5, dr6
ddiv dr8, dr9, dr10
dabs dr1, dr2
dneg dr3, dr4
dsqrt dr5, dr6
dtoi dr7, dr8
dtou dr9, dr10
dtof dr11, dr12
dround dr13, dr14
dmov.d dr15, dr0
dcmpun dr1, dr2
dcmpeq dr3, dr4
dcmplt dr5, dr6
dcmple dr7, dr15
ftod r1, dr2
itod r3, dr4
utod r15, dr0
dmov.d r1, drh2
dmov.l r3, drh4
dmov.l r5, drl6
dmov.l drh7, r8
dmov.l drl9, r10
dmov.l drh15, r0
dmov.d dr1, [r2]
dmov.d dr1, 0[r2]
dmov.d dr3, 8[r4]
dmov.d dr3, 2040[r4]
dmov.d dr5, 2048[r6]
dmov.d dr5, 524280[r6]
dmov.d [r7], dr8
dmov.d 16[r7], dr8
dmov.d 4096[r9], dr10
dmov.d #0, drh1
dmov.d #0x3ff00000, drh2
dmov.l #0x12345678, drh3
dmov.l #0xffffffff, drl4
dmov.l #1, drl15
dpushm.d dr0-dr15
dpushm.d dr4-dr4
dpushm.d dr8-dr11
dpopm.d dr0-dr15
dpopm.d dr8-dr11
dpushm.l dpsw-depc
dpushm.l dcr1-dcr2
dpopm.l dpsw-depc
dpopm.l dcmr-dcmr
mvtdc r1, dpsw
mvtdc r2, dcmr
mvtdc r3, decnt
mvtdc r4, depc
mvtdc r5, dcr0
mvfdc dpsw, r6
mvfdc depc, r7
mvfdc dcr2, r8
mvfdr
