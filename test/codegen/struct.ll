; Structure passing and returning, in the IR forms a frontend uses for the
; GCC rx ABI: structs GCC passes in registers (size a multiple of 4, <= 16)
; as [N x i32] / {i32, ...}; all others byval; results that GCC returns in
; memory through sret (R15). Linked with struct_main.c.
target triple = "rx-none-elf"

%s3 = type { [3 x i8] }
%s6 = type { [6 x i8] }
%s20 = type { [5 x i32] }

; --- called from GCC
define i32 @L_g1([2 x i32] %x) {             ; struct s8 in R1:R2
  %b = extractvalue [2 x i32] %x, 1
  ret i32 %b
}
define i32 @L_g2(i32 %a, [3 x i32] %x) {     ; struct s12 in R2-R4
  %c = extractvalue [3 x i32] %x, 2
  %r = sub i32 %c, %a
  ret i32 %r
}
define i32 @L_g3(i32 %a, i32 %b, [3 x i32] %x) {   ; s12 does not fit: stack
  %x0 = extractvalue [3 x i32] %x, 0
  %x2 = extractvalue [3 x i32] %x, 2
  %m = mul i32 %a, 100
  %s = add i32 %m, %b
  %t = add i32 %s, %x0
  %r = sub i32 %t, %x2
  ret i32 %r
}
define i32 @L_g4(ptr byval(%s3) align 1 %x, i32 %y) {   ; s3 on stack, y in R2
  %p = getelementptr i8, ptr %x, i32 2
  %c = load i8, ptr %p
  %e = zext i8 %c to i32
  %r = add i32 %e, %y
  ret i32 %r
}
define i32 @L_g5(ptr byval(%s6) align 1 %x, ptr byval(%s3) align 1 %y,
                 i32 %z) {                       ; s6 at +0, s3 at +6, z in R4
  %p = getelementptr i8, ptr %x, i32 5
  %c = load i8, ptr %p
  %q = getelementptr i8, ptr %y, i32 1
  %d = load i8, ptr %q
  %ec = zext i8 %c to i32
  %ed = zext i8 %d to i32
  %m = mul i32 %ed, 256
  %s = add i32 %ec, %m
  %r = add i32 %s, %z
  ret i32 %r
}
define i32 @L_g6(ptr byval(%s20) align 4 %x, i32 %y) {  ; both on the stack
  %p = getelementptr i32, ptr %x, i32 4
  %v = load i32, ptr %p
  %r = sub i32 %v, %y
  ret i32 %r
}
define i32 @L_g7(i32 %a, i32 %b, i32 %c, ptr byval(%s3) align 1 %x,
                 i32 %y) {                         ; s3 at +0, y at +4
  %v = load i8, ptr %x
  %e = zext i8 %v to i32
  %m = mul i32 %y, 1000
  %s1 = add i32 %e, %m
  %s2 = add i32 %s1, %a
  %s3 = add i32 %s2, %b
  %r = add i32 %s3, %c
  ret i32 %r
}
define i32 @L_g8(i32 %x) {                   ; struct {short a, b;} in R1
  %r = ashr i32 %x, 16
  ret i32 %r
}
; The byval copy belongs to the callee: writing it must not touch the caller.
define i32 @L_g10(ptr byval(%s20) align 4 %x) {
  store i32 99, ptr %x
  %v0 = load i32, ptr %x
  %p1 = getelementptr i32, ptr %x, i32 1
  %v1 = load i32, ptr %p1
  %r = add i32 %v0, %v1
  ret i32 %r
}
define { i32, i32 } @L_r8(i32 %a) {          ; struct s8 in R1:R2
  %b = add i32 %a, 1
  %r0 = insertvalue { i32, i32 } undef, i32 %a, 0
  %r1 = insertvalue { i32, i32 } %r0, i32 %b, 1
  ret { i32, i32 } %r1
}
define { i32, i32, i32, i32 } @L_r16(i32 %a) {   ; struct s16 in R1-R4
  %r0 = insertvalue { i32, i32, i32, i32 } undef, i32 %a, 0
  %r1 = insertvalue { i32, i32, i32, i32 } %r0, i32 2, 1
  %r2 = insertvalue { i32, i32, i32, i32 } %r1, i32 3, 2
  %r3 = insertvalue { i32, i32, i32, i32 } %r2, i32 4, 3
  ret { i32, i32, i32, i32 } %r3
}
define void @L_r20(ptr sret(%s20) align 4 %p, i32 %a) {   ; R15 = p, R1 = p
  store i32 %a, ptr %p
  %p1 = getelementptr i32, ptr %p, i32 1
  store i32 2, ptr %p1
  %p2 = getelementptr i32, ptr %p, i32 2
  store i32 3, ptr %p2
  %p3 = getelementptr i32, ptr %p, i32 3
  store i32 4, ptr %p3
  %p4 = getelementptr i32, ptr %p, i32 4
  %a5 = mul i32 %a, 5
  store i32 %a5, ptr %p4
  ret void
}
define void @L_r3(ptr sret(%s3) align 1 %p, i32 %a) {     ; size 3: memory
  %t = trunc i32 %a to i8
  store i8 %t, ptr %p
  %p1 = getelementptr i8, ptr %p, i32 1
  store i8 2, ptr %p1
  %p2 = getelementptr i8, ptr %p, i32 2
  store i8 3, ptr %p2
  ret void
}

; --- calls into GCC
declare i32 @C_g3(i32, i32, [3 x i32])
declare i32 @C_g4(ptr byval(%s3) align 1, i32)
declare i32 @C_g5(ptr byval(%s6) align 1, ptr byval(%s3) align 1, i32)
declare i32 @C_g6(ptr byval(%s20) align 4, i32)
declare i32 @C_g7(i32, i32, i32, ptr byval(%s3) align 1, i32)
declare { i32, i32 } @C_r8(i32)
declare void @C_r20(ptr sret(%s20) align 4, i32)
declare void @C_r3(ptr sret(%s3) align 1, i32)

define i32 @L_cross_s(ptr %p3, ptr %p6, ptr %p20, i32 %k) {
  %r1 = call i32 @C_g4(ptr byval(%s3) align 1 %p3, i32 %k)
  %r2 = call i32 @C_g5(ptr byval(%s6) align 1 %p6, ptr byval(%s3) align 1 %p3,
                       i32 %k)
  %r3 = call i32 @C_g6(ptr byval(%s20) align 4 %p20, i32 %k)
  %r4 = call i32 @C_g7(i32 1, i32 2, i32 3, ptr byval(%s3) align 1 %p3,
                       i32 %k)
  %k1 = add i32 %k, 1
  %k2 = add i32 %k, 2
  %a0 = insertvalue [3 x i32] undef, i32 %k, 0
  %a1 = insertvalue [3 x i32] %a0, i32 %k1, 1
  %a2 = insertvalue [3 x i32] %a1, i32 %k2, 2
  %r5 = call i32 @C_g3(i32 10, i32 20, [3 x i32] %a2)
  %q = call { i32, i32 } @C_r8(i32 %k)
  %q0 = extractvalue { i32, i32 } %q, 0
  %q1 = extractvalue { i32, i32 } %q, 1
  %buf = alloca %s20, align 4
  call void @C_r20(ptr sret(%s20) align 4 %buf, i32 %k)
  %bp = getelementptr i32, ptr %buf, i32 4
  %b4 = load i32, ptr %bp
  %b3 = alloca %s3, align 1
  call void @C_r3(ptr sret(%s3) align 1 %b3, i32 %k)
  %cp = getelementptr i8, ptr %b3, i32 2
  %c2 = load i8, ptr %cp
  %c2e = zext i8 %c2 to i32
  ; r1 + 2 r2 + 3 r3 + 4 r4 + 5 r5 + 6 q0 + 7 q1 + 8 b4 + 9 c2
  %m2 = mul i32 %r2, 2
  %m3 = mul i32 %r3, 3
  %m4 = mul i32 %r4, 4
  %m5 = mul i32 %r5, 5
  %m6 = mul i32 %q0, 6
  %m7 = mul i32 %q1, 7
  %m8 = mul i32 %b4, 8
  %m9 = mul i32 %c2e, 9
  %s1 = add i32 %r1, %m2
  %s2 = add i32 %s1, %m3
  %s3 = add i32 %s2, %m4
  %s4 = add i32 %s3, %m5
  %s5 = add i32 %s4, %m6
  %s6 = add i32 %s5, %m7
  %s7 = add i32 %s6, %m8
  %s8 = add i32 %s7, %m9
  ret i32 %s8
}
