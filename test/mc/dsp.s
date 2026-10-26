# DSP (accumulator) instructions: the RXv1 forms with the implicit A0, then
# the forms that name A0 / A1; checked with scripts/mc-diff.sh lines.
mulhi r1, r2
mullo r3, r4
machi r5, r6
maclo r7, r8
mvtachi r9
mvtaclo r10
mvfachi r11
mvfacmi r12
mvfaclo r13
racw #1
racw #2
# RXv2
mulhi r1, r2, a0
mulhi r1, r2, a1
mullo r3, r4, a1
mullh r5, r6, a0
mullh r5, r6, a1
emula r7, r8, a1
machi r9, r10, a1
maclo r11, r12, a1
maclh r13, r14, a0
maclh r13, r14, a1
emaca r15, r0, a1
msbhi r1, r2, a0
msbhi r1, r2, a1
msblo r3, r4, a1
msblh r5, r6, a1
emsba r7, r8, a0
emsba r7, r8, a1
mvtachi r1, a0
mvtachi r1, a1
mvtaclo r2, a1
mvtacgu r3, a0
mvtacgu r3, a1
mvfachi #0, a0, r1
mvfachi #1, a0, r1
mvfachi #2, a0, r1
mvfachi #2, a1, r15
mvfacmi #0, a1, r2
mvfacmi #1, a1, r2
mvfaclo #2, a1, r3
mvfacgu #0, a0, r4
mvfacgu #1, a1, r4
mvfacgu #2, a1, r4
racw #1, a1
racw #2, a1
rdacw #1, a0
rdacw #2, a1
racl #1, a0
racl #2, a1
rdacl #1, a1
rdacl #2, a0
