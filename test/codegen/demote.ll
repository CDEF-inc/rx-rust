; Results that do not fit in R1-R4 and are returned by value in the IR, with
; no explicit sret: LLVM demotes them to a hidden sret pointer itself. That
; pointer must be passed like GCC's (R15, not counted against R1-R4) and its
; address returned in R1. Linked with demote_main.c.
target triple = "rx-none-elf"

%s20 = type { i32, i32, i32, i32, i32 }
%t20 = type { i32, i32, i32, i32, i8 }
%q24 = type { i64, i64, i64 }
%s6 = type { [6 x i8] }

; --- called from GCC
define %s20 @L_d20(i32 %a) {                 ; struct s20 through R15
  %a5 = mul i32 %a, 5
  %r0 = insertvalue %s20 undef, i32 %a, 0
  %r1 = insertvalue %s20 %r0, i32 2, 1
  %r2 = insertvalue %s20 %r1, i32 3, 2
  %r3 = insertvalue %s20 %r2, i32 4, 3
  %r4 = insertvalue %s20 %r3, i32 %a5, 4
  ret %s20 %r4
}
define [5 x i32] @L_a20(i32 %a) {            ; the same as an array
  %r0 = insertvalue [5 x i32] undef, i32 %a, 0
  %r1 = insertvalue [5 x i32] %r0, i32 -1, 1
  %r2 = insertvalue [5 x i32] %r1, i32 %a, 2
  %r3 = insertvalue [5 x i32] %r2, i32 -1, 3
  %r4 = insertvalue [5 x i32] %r3, i32 %a, 4
  ret [5 x i32] %r4
}
define %t20 @L_t20(i32 %a) {                 ; trailing char, padding after it
  %c = trunc i32 %a to i8
  %r0 = insertvalue %t20 undef, i32 1, 0
  %r1 = insertvalue %t20 %r0, i32 2, 1
  %r2 = insertvalue %t20 %r1, i32 3, 2
  %r3 = insertvalue %t20 %r2, i32 %a, 3
  %r4 = insertvalue %t20 %r3, i8 %c, 4
  ret %t20 %r4
}
define %q24 @L_q24(i64 %a, i64 %b) {         ; i64 fields; a in R1:R2, b in R3:R4
  %s = add i64 %a, %b
  %r0 = insertvalue %q24 undef, i64 %a, 0
  %r1 = insertvalue %q24 %r0, i64 %b, 1
  %r2 = insertvalue %q24 %r1, i64 %s, 2
  ret %q24 %r2
}
; The hidden pointer does not take R1: a-d in R1-R4, e on the stack.
define %s20 @L_d5(i32 %a, i32 %b, i32 %c, i32 %d, i32 %e) {
  %r0 = insertvalue %s20 undef, i32 %a, 0
  %r1 = insertvalue %s20 %r0, i32 %b, 1
  %r2 = insertvalue %s20 %r1, i32 %c, 2
  %r3 = insertvalue %s20 %r2, i32 %d, 3
  %r4 = insertvalue %s20 %r3, i32 %e, 4
  ret %s20 %r4
}
; With a byval argument on the stack and one after it in R2.
define %s20 @L_dv(ptr byval(%s6) align 1 %x, i32 %y) {
  %p = getelementptr i8, ptr %x, i32 5
  %c = load i8, ptr %p
  %e = zext i8 %c to i32
  %r0 = insertvalue %s20 undef, i32 %e, 0
  %r1 = insertvalue %s20 %r0, i32 %y, 1
  %r2 = insertvalue %s20 %r1, i32 0, 2
  %r3 = insertvalue %s20 %r2, i32 0, 3
  %r4 = insertvalue %s20 %r3, i32 %y, 4
  ret %s20 %r4
}
; Two paths that return different values (the demoted store is per return).
define %s20 @L_dsel(i32 %a) {
  %c = icmp slt i32 %a, 0
  br i1 %c, label %neg, label %pos
neg:
  %n = sub i32 0, %a
  %n0 = insertvalue %s20 zeroinitializer, i32 %n, 4
  ret %s20 %n0
pos:
  %p0 = insertvalue %s20 zeroinitializer, i32 %a, 0
  ret %s20 %p0
}

; --- calls into GCC, and to the functions above
declare %s20 @C_d20(i32)
declare %q24 @C_q24(i64, i64)
declare %s20 @C_d5(i32, i32, i32, i32, i32)

; r = sum over i of (i + 1) * field_i of each result, so that a wrong field
; or a missing store changes it.
define i32 @L_cross_d(i32 %k) {
  %c = call %s20 @C_d20(i32 %k)
  %c0 = extractvalue %s20 %c, 0
  %c4 = extractvalue %s20 %c, 4
  %q = call %q24 @C_q24(i64 7, i64 -3)
  %q2 = extractvalue %q24 %q, 2
  %q2l = trunc i64 %q2 to i32
  %d = call %s20 @C_d5(i32 %k, i32 2, i32 3, i32 4, i32 5)
  %d0 = extractvalue %s20 %d, 0
  %d4 = extractvalue %s20 %d, 4
  %l = call %s20 @L_d5(i32 1, i32 2, i32 3, i32 4, i32 %k)
  %l4 = extractvalue %s20 %l, 4
  %m = call %s20 @L_dsel(i32 %k)
  %m0 = extractvalue %s20 %m, 0
  %m4 = extractvalue %s20 %m, 4
  %v1 = mul i32 %c4, 2
  %v2 = mul i32 %q2l, 3
  %v3 = mul i32 %d0, 4
  %v4 = mul i32 %d4, 5
  %v5 = mul i32 %l4, 6
  %v6 = mul i32 %m0, 7
  %v7 = mul i32 %m4, 8
  %s1 = add i32 %c0, %v1
  %s2 = add i32 %s1, %v2
  %s3 = add i32 %s2, %v3
  %s4 = add i32 %s3, %v4
  %s5 = add i32 %s4, %v5
  %s6 = add i32 %s5, %v6
  %s7 = add i32 %s6, %v7
  ret i32 %s7
}
