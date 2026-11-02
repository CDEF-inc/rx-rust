; The high half of a 32 x 32 multiply (emul / emulu) and what uses it:
; division and remainder by a constant, multiply-high, multiplies with an
; overflow check and 64-bit multiplies. Linked with mulh_main.c, which
; compares every function with a real division or a 64-bit multiply.
target triple = "rx-none-elf"

define i32 @L_udiv3(i32 %a) {
  %r = udiv i32 %a, 3
  ret i32 %r
}
define i32 @L_udiv7(i32 %a) {
  %r = udiv i32 %a, 7
  ret i32 %r
}
define i32 @L_udiv10(i32 %a) {
  %r = udiv i32 %a, 10
  ret i32 %r
}
define i32 @L_urem10(i32 %a) {
  %r = urem i32 %a, 10
  ret i32 %r
}
define i32 @L_udiv1000(i32 %a) {
  %r = udiv i32 %a, 1000
  ret i32 %r
}
define i32 @L_urem10000(i32 %a) {
  %r = urem i32 %a, 10000
  ret i32 %r
}
define i32 @L_udiv641(i32 %a) {
  %r = udiv i32 %a, 641
  ret i32 %r
}
define i32 @L_udivbig(i32 %a) {
  %r = udiv i32 %a, 2147483649
  ret i32 %r
}
define i32 @L_udiv16(i32 %a) {             ; a power of two: a shift
  %r = udiv i32 %a, 16
  ret i32 %r
}
define i32 @L_sdiv3(i32 %a) {
  %r = sdiv i32 %a, 3
  ret i32 %r
}
define i32 @L_sdiv7(i32 %a) {
  %r = sdiv i32 %a, 7
  ret i32 %r
}
define i32 @L_sdiv10(i32 %a) {
  %r = sdiv i32 %a, 10
  ret i32 %r
}
define i32 @L_srem10(i32 %a) {
  %r = srem i32 %a, 10
  ret i32 %r
}
define i32 @L_sdivm3(i32 %a) {
  %r = sdiv i32 %a, -3
  ret i32 %r
}
define i32 @L_sremm7(i32 %a) {
  %r = srem i32 %a, -7
  ret i32 %r
}
define i32 @L_sdiv1000(i32 %a) {
  %r = sdiv i32 %a, 1000
  ret i32 %r
}
define i32 @L_udivh10(i32 %a) {            ; a u16 divided by 10
  %t = trunc i32 %a to i16
  %d = udiv i16 %t, 10
  %r = zext i16 %d to i32
  ret i32 %r
}
define i32 @L_udivvar(i32 %a, i32 %b) {    ; not a constant: divu
  %r = udiv i32 %a, %b
  ret i32 %r
}

define i32 @L_mulhu(i32 %a, i32 %b) {
  %x = zext i32 %a to i64
  %y = zext i32 %b to i64
  %m = mul i64 %x, %y
  %h = lshr i64 %m, 32
  %r = trunc i64 %h to i32
  ret i32 %r
}
define i32 @L_mulhs(i32 %a, i32 %b) {
  %x = sext i32 %a to i64
  %y = sext i32 %b to i64
  %m = mul i64 %x, %y
  %h = ashr i64 %m, 32
  %r = trunc i64 %h to i32
  ret i32 %r
}
; The product in *p, and whether it overflowed.
define i32 @L_umulo(i32 %a, i32 %b, ptr %p) {
  %m = call { i32, i1 } @llvm.umul.with.overflow.i32(i32 %a, i32 %b)
  %v = extractvalue { i32, i1 } %m, 0
  %o = extractvalue { i32, i1 } %m, 1
  store i32 %v, ptr %p
  %r = zext i1 %o to i32
  ret i32 %r
}
define i32 @L_smulo(i32 %a, i32 %b, ptr %p) {
  %m = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %a, i32 %b)
  %v = extractvalue { i32, i1 } %m, 0
  %o = extractvalue { i32, i1 } %m, 1
  store i32 %v, ptr %p
  %r = zext i1 %o to i32
  ret i32 %r
}
define i64 @L_mulzz(i32 %a, i32 %b) {
  %x = zext i32 %a to i64
  %y = zext i32 %b to i64
  %m = mul i64 %x, %y
  ret i64 %m
}
define i64 @L_mulss(i32 %a, i32 %b) {
  %x = sext i32 %a to i64
  %y = sext i32 %b to i64
  %m = mul i64 %x, %y
  ret i64 %m
}
define i64 @L_mul64(i64 %a, i64 %b) {
  %m = mul i64 %a, %b
  ret i64 %m
}
define i64 @L_udiv64x10(i64 %a) {
  %r = udiv i64 %a, 10
  ret i64 %r
}
define i64 @L_urem64x3(i64 %a) {
  %r = urem i64 %a, 3
  ret i64 %r
}

declare { i32, i1 } @llvm.umul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
