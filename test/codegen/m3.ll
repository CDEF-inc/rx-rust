; i64 / float / double test: compiled with llc, linked with m3_main.c (GCC
; -m64bit-doubles) and run under QEMU by scripts/run-codegen-test.sh.
target triple = "rx-none-elf"

; --- i64 arithmetic
define i64 @l_add(i64 %a, i64 %b) { %r = add i64 %a, %b
  ret i64 %r }
define i64 @l_sub(i64 %a, i64 %b) { %r = sub i64 %a, %b
  ret i64 %r }
define i64 @l_mul(i64 %a, i64 %b) { %r = mul i64 %a, %b
  ret i64 %r }
define i64 @l_sdiv(i64 %a, i64 %b) { %r = sdiv i64 %a, %b
  ret i64 %r }
define i64 @l_udiv(i64 %a, i64 %b) { %r = udiv i64 %a, %b
  ret i64 %r }
define i64 @l_srem(i64 %a, i64 %b) { %r = srem i64 %a, %b
  ret i64 %r }
define i64 @l_urem(i64 %a, i64 %b) { %r = urem i64 %a, %b
  ret i64 %r }
define i64 @l_and(i64 %a, i64 %b) { %r = and i64 %a, %b
  ret i64 %r }
define i64 @l_or(i64 %a, i64 %b) { %r = or i64 %a, %b
  ret i64 %r }
define i64 @l_xor(i64 %a, i64 %b) { %r = xor i64 %a, %b
  ret i64 %r }
define i64 @l_shl(i64 %a, i32 %s) { %e = zext i32 %s to i64
  %r = shl i64 %a, %e
  ret i64 %r }
define i64 @l_lshr(i64 %a, i32 %s) { %e = zext i32 %s to i64
  %r = lshr i64 %a, %e
  ret i64 %r }
define i64 @l_ashr(i64 %a, i32 %s) { %e = zext i32 %s to i64
  %r = ashr i64 %a, %e
  ret i64 %r }
define i64 @l_shl40(i64 %a) { %r = shl i64 %a, 40
  ret i64 %r }
define i64 @l_ashr3(i64 %a) { %r = ashr i64 %a, 3
  ret i64 %r }
define i64 @l_neg(i64 %a) { %r = sub i64 0, %a
  ret i64 %r }
define i64 @l_addimm(i64 %a) { %r = add i64 %a, 4294967297
  ret i64 %r }
define i32 @l_slt(i64 %a, i64 %b) { %c = icmp slt i64 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @l_ult(i64 %a, i64 %b) { %c = icmp ult i64 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @l_eq(i64 %a, i64 %b) { %c = icmp eq i64 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i64 @l_select(i64 %a, i64 %b) {
  %c = icmp sgt i64 %a, %b
  %r = select i1 %c, i64 %a, i64 %b
  ret i64 %r
}
define i64 @l_sext(i32 %a) { %r = sext i32 %a to i64
  ret i64 %r }
define i64 @l_zext(i32 %a) { %r = zext i32 %a to i64
  ret i64 %r }
define i32 @l_trunc(i64 %a) { %r = trunc i64 %a to i32
  ret i32 %r }

; --- argument placement (GCC: running byte count over R1-R4)
define i64 @p_2(i32 %a, i64 %x) {          ; x in R2:R3
  %e = sext i32 %a to i64
  %r = add i64 %x, %e
  ret i64 %r
}
define i64 @p_3(i32 %a, i32 %b, i32 %c, i64 %x, i32 %y) {
  ; x does not fit after 12 bytes: stack +0; y follows it on the stack (+8)
  %e = sext i32 %y to i64
  %m = mul i64 %x, 3
  %r = add i64 %m, %e
  ret i64 %r
}
define i64 @p_5(i32 %a, i32 %b, i32 %c, i32 %d, i32 %e, i64 %x) {
  %t = sext i32 %e to i64
  %r = sub i64 %x, %t
  ret i64 %r
}
define i64 @p_ll3(i64 %a, i64 %b, i64 %c) {
  %b2 = mul i64 %b, 2
  %c3 = mul i64 %c, 3
  %s = add i64 %a, %b2
  %r = add i64 %s, %c3
  ret i64 %r
}

; --- double
define double @d_add(double %a, double %b) { %r = fadd double %a, %b
  ret double %r }
define double @d_sub(double %a, double %b) { %r = fsub double %a, %b
  ret double %r }
define double @d_mul(double %a, double %b) { %r = fmul double %a, %b
  ret double %r }
define double @d_div(double %a, double %b) { %r = fdiv double %a, %b
  ret double %r }
define double @d_neg(double %a) { %r = fneg double %a
  ret double %r }
declare double @llvm.fabs.f64(double)
define double @d_abs(double %a) { %r = call double @llvm.fabs.f64(double %a)
  ret double %r }
define i32 @d_lt(double %a, double %b) { %c = fcmp olt double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_eq(double %a, double %b) { %c = fcmp oeq double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_ge(double %a, double %b) { %c = fcmp oge double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_uno(double %a, double %b) { %c = fcmp uno double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define double @d_fromi(i32 %a) { %r = sitofp i32 %a to double
  ret double %r }
define double @d_fromu(i32 %a) { %r = uitofp i32 %a to double
  ret double %r }
define i32 @d_toi(double %a) { %r = fptosi double %a to i32
  ret i32 %r }
define double @d_fromll(i64 %a) { %r = sitofp i64 %a to double
  ret double %r }
define i64 @d_toll(double %a) { %r = fptosi double %a to i64
  ret i64 %r }
define double @d_fromf(float %a) { %r = fpext float %a to double
  ret double %r }
define float @d_tof(double %a) { %r = fptrunc double %a to float
  ret float %r }
define double @d_poly(double %x) {
  %a = fmul double %x, 1.5
  %b = fadd double %a, 2.25
  %c = fmul double %b, %x
  %d = fsub double %c, 0.125
  %r = fdiv double %d, 3.0
  ret double %r
}
define double @d_pos(i32 %a, double %x, i32 %b) {   ; x in R2:R3, b in R4
  %fa = sitofp i32 %a to double
  %fb = sitofp i32 %b to double
  %m = fmul double %x, %fa
  %r = fadd double %m, %fb
  ret double %r
}
define double @d_stack(i32 %a, i32 %b, i32 %c, double %x, double %y) {
  %r = fsub double %x, %y                          ; x at +0, y at +8
  ret double %r
}
define double @d_select(double %a, double %b) {
  %c = fcmp ogt double %a, %b
  %r = select i1 %c, double %a, double %b
  ret double %r
}

; --- float
define float @f_add(float %a, float %b) { %r = fadd float %a, %b
  ret float %r }
define float @f_sub(float %a, float %b) { %r = fsub float %a, %b
  ret float %r }
define float @f_mul(float %a, float %b) { %r = fmul float %a, %b
  ret float %r }
define float @f_div(float %a, float %b) { %r = fdiv float %a, %b
  ret float %r }
define i32 @f_lt(float %a, float %b) { %c = fcmp olt float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define float @f_fromi(i32 %a) { %r = sitofp i32 %a to float
  ret float %r }
define i32 @f_toi(float %a) { %r = fptosi float %a to i32
  ret i32 %r }
define double @f_mix(float %a, double %b) {     ; a in R1, b in R2:R3
  %e = fpext float %a to double
  %r = fmul double %e, %b
  ret double %r
}

; --- calls into GCC code
declare double @c_mix(i32, i64, double, float, i64)
declare i64 @c_ret64(i32)
declare double @c_retd(i32)
declare double @c_va(i32, ...)

define double @l_cross(i64 %x, double %y) {
  %f = fptrunc double %y to float
  %x2 = mul i64 %x, 2
  %a = call double @c_mix(i32 3, i64 %x, double %y, float %f, i64 %x2)
  %t = trunc i64 %x to i32
  %b = call i64 @c_ret64(i32 %t)
  %c = call double @c_retd(i32 %t)
  %d = call double (i32, ...) @c_va(i32 3, i64 %b, double %c, i32 %t)
  %bf = sitofp i64 %b to double
  %s1 = fadd double %a, %bf
  %s2 = fadd double %s1, %c
  %s3 = fadd double %s2, %d
  ret double %s3
}
