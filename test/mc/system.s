# System control instructions; checked with scripts/mc-diff.sh lines.
mvtc r1, psw
mvtc r2, usp
mvtc r3, fpsw
mvtc r4, bpsw
mvtc r5, bpc
mvtc r15, isp
mvtc r6, fintv
mvtc r7, intb
mvfc psw, r1
mvfc pc, r2
mvfc usp, r3
mvfc fpsw, r4
mvfc isp, r15
mvfc intb, r5
mvtc #0, psw
mvtc #127, fpsw
mvtc #-1, isp
mvtc #128, isp
mvtc #0x10000, intb
mvtc #0x7fffff, usp
mvtc #0x800000, intb
mvtc #0x17ff0, isp
mvtc #0x01fffff0, isp
mvtc #0xffffff80, intb
pushc psw
pushc fpsw
pushc intb
popc psw
popc isp
mvtipl #0
mvtipl #7
mvtipl #15
int #0
int #255
setpsw c
setpsw z
setpsw s
setpsw o
setpsw i
setpsw u
clrpsw c
clrpsw z
clrpsw s
clrpsw o
clrpsw i
clrpsw u
