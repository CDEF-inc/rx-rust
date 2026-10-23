# One instruction per line; checked with scripts/mc-diff.sh lines.
# Values sit on the boundaries where GNU as switches encodings.

# --- mov register / immediate
mov.b r1, r2
mov.w r1, r15
mov.l r15, r0
mov.l #0, r1
mov.l #15, r1
mov.l #16, r1
mov.l #255, r15
mov.l #256, r1
mov.l #-1, r1
mov.l #-128, r1
mov.l #-129, r1
mov.l #32767, r1
mov.l #32768, r1
mov.l #-32768, r1
mov.l #-32769, r1
mov.l #0x7fffff, r1
mov.l #0x800000, r1
mov.l #-0x800000, r1
mov.l #-0x800001, r1
mov.l #0x7fffffff, r1
mov.l #0x80000000, r1
mov.l #0xffffffff, r1
mov.l #0xffffff80, r1

# --- mov loads
mov.b [r1], r2
mov.w [r9], r10
mov.l [r15], r0
mov.b 0[r1], r2
mov.b 31[r7], r7
mov.b 32[r1], r2
mov.b 255[r1], r2
mov.b 256[r1], r2
mov.b 65535[r1], r2
mov.b 4[r8], r2
mov.b 4[r1], r8
mov.b 0[r9], r2
movu.w 0[r12], r1
mov.w 2[r1], r2
mov.w 62[r1], r2
mov.w 64[r1], r2
mov.w 510[r1], r2
mov.w 512[r1], r2
mov.w 131070[r1], r2
mov.l 4[r1], r2
mov.l 124[r1], r2
mov.l 128[r1], r2
mov.l 1020[r1], r2
mov.l 1024[r1], r2
mov.l 262140[r1], r2
mov.l 8[r14], r15

# --- mov stores
mov.b r2, [r1]
mov.w r10, [r9]
mov.l r0, [r15]
mov.b r2, 0[r1]
mov.b r7, 31[r7]
mov.b r2, 32[r1]
mov.b r2, 256[r1]
mov.b r8, 4[r1]
mov.l r2, 0[r9]
mov.w r9, 0[r1]
mov.w r2, 62[r1]
mov.w r2, 64[r1]
mov.w r2, 512[r1]
mov.l r2, 124[r1]
mov.l r2, 128[r1]
mov.l r2, 1024[r1]
mov.l r15, 262140[r14]

# --- movu
movu.b r1, r2
movu.w r15, r0
movu.b [r1], r2
movu.w [r9], r10
movu.b 31[r1], r2
movu.b 32[r1], r2
movu.b 256[r1], r2
movu.w 62[r7], r1
movu.w 64[r1], r2
movu.w 512[r1], r2
movu.b 4[r8], r1

# --- add
add r1, r2
add r15, r0
add #0, r1
add #15, r1
add #16, r1
add #127, r1
add #128, r1
add #-1, r1
add #-128, r1
add #-129, r1
add #32767, r1
add #32768, r1
add #0x800000, r1
add #0xffffffff, r1
add #1, r2, r3
add #128, r2, r3
add #0x12345678, r15, r14
add r1, r2, r3
add r15, r14, r13

# --- sub
sub r1, r2
sub #0, r1
sub #15, r1
sub #16, r1
sub #1000, r1
sub #-1, r1
sub r1, r2, r3

# --- cmp
cmp r1, r2
cmp #0, r1
cmp #15, r1
cmp #16, r1
cmp #255, r1
cmp #256, r1
cmp #-1, r1
cmp #-128, r1
cmp #-129, r1
cmp #0x12345678, r1

# --- mul / and / or
mul r1, r2
mul #3, r1
mul #16, r1
mul #-1, r1
mul #1000, r1
mul #0x100000, r1
mul #0x10000000, r1
mul r1, r2, r3
and r1, r2
and #15, r1
and #0xff, r1
and #0xffff, r1
and #0xffffff00, r1
and #0x00ffffff, r1
and r1, r2, r3
or r1, r2
or #1, r1
or #0x80, r1
or #0x80000000, r1
or r1, r2, r3

# --- FC register ops
sbb r1, r2
neg r1, r2
adc r1, r2
abs r1, r2
max r1, r2
min r1, r2
emul r1, r2
emulu r1, r2
div r1, r2
divu r1, r2
tst r1, r2
xor r1, r2
not r1, r2
xor r1, r2, r3

# --- FD 70 immediate ops
adc #1, r1
max #-1, r1
min #127, r1
emul #128, r1
emulu #0x1234, r1
div #0x123456, r1
divu #0x12345678, r1
xor #0, r15
xor #0xffffffff, r1

# --- unary
not r1
neg r2
abs r3
sat r4
rorc r5
rolc r6

# --- shifts / rotates
shlr #0, r1
shlr #15, r1
shlr #16, r1
shlr #31, r15
shar #1, r1
shar #31, r1
shll #2, r1
shll #17, r1
shlr r1, r2
shar r1, r2
shll r1, r2
shlr #3, r1, r2
shar #31, r1, r2
shll #16, r15, r14
rotl #1, r1
rotl #16, r1
rotr #31, r1
rotl r1, r2
rotr r1, r2
revw r1, r2
revl r1, r2

# --- stack
push.b r1
push.w r2
push.l r15
pop r1
pop r15
pushm r1-r2
pushm r6-r13
pushm r1-r15
popm r6-r13
popm r1-r15
rtsd #0
rtsd #4
rtsd #1020
rtsd #8, r6-r13
rtsd #0, r1-r15

# --- control
brk
dbt
rts
nop
satr
rtfi
rte
wait
jmp r1
jsr r15
bra.l r2
bsr.l r3
