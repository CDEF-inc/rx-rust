# RXv3e Protect Zone instructions, INT Rs, the 5-bit MVTIPL and the new
# control registers; checked with scripts/mc-diff.sh lines. GNU as 2.43 has
# no RXv3e, so the RXv3e forms are compared with the encoding in the RXv3e
# manual (R01US0745EJ0101, section 4).
# mcpu: rxv3e
trcall #0  ; bytes: fd 75 00 00
trcall #128  ; bytes: fd 75 00 80
trcall #255  ; bytes: fd 75 00 ff
trrte  ; bytes: 7f 97
trmvfc ivc, r1  ; bytes: fd 5a 51
trmvfc bpsw, r2  ; bytes: fd 5a 82
trmvfc bpc, r3  ; bytes: fd 5a 93
trmvfc isp, r4  ; bytes: fd 5a a4
trmvfc intb, r5  ; bytes: fd 5a c5
trmvfc extb, r6  ; bytes: fd 5a d6
trmvfc trctb, r0  ; bytes: fd 5a e0
trmvfc wintb, r15  ; bytes: fd 5a ff
trmvtc r1, ivc  ; bytes: fd 58 15
trmvtc r2, bpsw  ; bytes: fd 58 28
trmvtc r3, bpc  ; bytes: fd 58 39
trmvtc r4, isp  ; bytes: fd 58 4a
trmvtc r5, intb  ; bytes: fd 58 5c
trmvtc r6, extb  ; bytes: fd 58 6d
trmvtc r0, trctb  ; bytes: fd 58 0e
trmvtc r15, wintb  ; bytes: fd 58 ff
trmvtc #0, ivc  ; bytes: fd 77 25 00
trmvtc #127, intb  ; bytes: fd 77 2c 7f
trmvtc #-128, intb  ; bytes: fd 77 2c 80
trmvtc #128, intb  ; bytes: fd 7b 2c 80 00
trmvtc #-32768, extb  ; bytes: fd 7b 2d 00 80
trmvtc #32768, bpsw  ; bytes: fd 7f 28 00 80 00
trmvtc #-8388608, bpc  ; bytes: fd 7f 29 00 00 80
trmvtc #8388608, isp  ; bytes: fd 73 2a 00 00 80 00
trmvtc #0x80000000, trctb  ; bytes: fd 73 2e 00 00 00 80
trmvtc #0x12345678, wintb  ; bytes: fd 73 2f 78 56 34 12
int r0  ; bytes: fd 75 10 00
int r7  ; bytes: fd 75 17 00
int r15  ; bytes: fd 75 1f 00
int #3
mvtipl #0
mvtipl #15
mvtipl #16  ; bytes: 75 70 10
mvtipl #31  ; bytes: 75 70 1f
mvtc r1, ivc  ; bytes: fd 68 15
mvtc r2, wpri  ; bytes: fd 68 2e
mvtc r3, wid  ; bytes: fd 68 3f
mvtc #1, ivc  ; bytes: fd 77 05 01
mvtc #0x1000, wpri  ; bytes: fd 7b 0e 00 10
mvtc #0x80000000, wid  ; bytes: fd 73 0f 00 00 00 80
mvfc ivc, r1  ; bytes: fd 6a 51
mvfc wpri, r2  ; bytes: fd 6a e2
mvfc wid, r15  ; bytes: fd 6a ff
pushc ivc  ; bytes: 7e c5
pushc wpri  ; bytes: 7e ce
pushc wid  ; bytes: 7e cf
popc ivc  ; bytes: 7e e5
popc wpri  ; bytes: 7e ee
popc wid  ; bytes: 7e ef
