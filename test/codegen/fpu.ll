; float with +fpu: compiled with llc (ATTRS=+fpu), linked with fpu_main.c
; and run under QEMU by scripts/run-codegen-test.sh. Operations marked
; nnan ninf in functions that allow flushing denormals (#0) use FPU
; instructions; the others must keep IEEE semantics (soft-float libcalls),
; including for NaNs, infinities and denormals.
target triple = "rx-none-elf"

; --- fast-math arithmetic (fadd / fsub / fmul / fdiv)
define float @fa_add(float %a, float %b) #0 { %r = fadd nnan ninf float %a, %b
  ret float %r }
define float @fa_sub(float %a, float %b) #0 { %r = fsub nnan ninf float %a, %b
  ret float %r }
define float @fa_mul(float %a, float %b) #0 { %r = fmul fast float %a, %b
  ret float %r }
define float @fa_div(float %a, float %b) #0 { %r = fdiv nnan ninf float %a, %b
  ret float %r }
; nnan ninf without reassoc, so the order matches the C code.
define float @fa_poly(float %x) #0 {
  %x2 = fmul nnan ninf float %x, %x
  %t = fmul nnan ninf float %x2, 0x3FD5555560000000   ; 1/3
  %u = fadd nnan ninf float %t, -2.500000e+00
  %v = fmul nnan ninf float %u, %x
  %w = fadd nnan ninf float %v, 1.000000e+00
  ret float %w
}

; --- fast-math compares (fcmp), as values, selects and branches
define i32 @fa_olt(float %a, float %b) #0 { %c = fcmp nnan ninf olt float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @fa_ole(float %a, float %b) #0 { %c = fcmp nnan ninf ole float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @fa_ogt(float %a, float %b) #0 { %c = fcmp fast ogt float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @fa_oge(float %a, float %b) #0 { %c = fcmp fast oge float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @fa_oeq(float %a, float %b) #0 { %c = fcmp fast oeq float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @fa_une(float %a, float %b) #0 { %c = fcmp fast une float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define float @fa_max(float %a, float %b) #0 {
  %c = fcmp fast ogt float %a, %b
  %r = select i1 %c, float %a, float %b
  ret float %r
}
define i32 @fa_branch(float %a, float %b) #0 {
entry:
  %c = fcmp fast olt float %a, %b
  br i1 %c, label %lt, label %ge
lt:
  ret i32 -1
ge:
  %d = fcmp fast oeq float %a, %b
  %r = select i1 %d, i32 0, i32 1
  ret i32 %r
}

; --- IEEE operations (libcalls), with NaN / infinity inputs from the driver
define float @f_add(float %a, float %b) { %r = fadd float %a, %b
  ret float %r }
; nnan ninf, but the default (IEEE) denormal environment: still a libcall,
; and denormals are added exactly.
define float @fa_add_ieee(float %a, float %b) { %r = fadd nnan ninf float %a, %b
  ret float %r }
define i32 @fa_olt_ieee(float %a, float %b) { %c = fcmp nnan ninf olt float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define float @f_mul(float %a, float %b) { %r = fmul nsz float %a, %b
  ret float %r }
define float @f_div(float %a, float %b) { %r = fdiv float %a, %b
  ret float %r }
define i32 @f_olt(float %a, float %b) { %c = fcmp olt float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_ult(float %a, float %b) { %c = fcmp ult float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_oeq(float %a, float %b) { %c = fcmp oeq float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_une(float %a, float %b) { %c = fcmp une float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_uno(float %a, float %b) { %c = fcmp uno float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define float @f_sel(float %a, float %b) {
  %c = fcmp ogt float %a, %b
  %r = select i1 %c, float %a, float %b
  ret float %r
}

; --- bit operations, conversions, constants, memory, calls
define float @f_neg(float %a) { %r = fneg float %a
  ret float %r }
define float @f_abs(float %a) { %r = call float @llvm.fabs.f32(float %a)
  ret float %r }
define float @f_copysign(float %a, float %b) {
  %r = call float @llvm.copysign.f32(float %a, float %b)
  ret float %r }
define float @f_fromi(i32 %a) { %r = sitofp i32 %a to float
  ret float %r }
define float @f_fromu(i32 %a) { %r = uitofp i32 %a to float
  ret float %r }
define i32 @f_toi(float %a) { %r = fptosi float %a to i32
  ret i32 %r }
define i32 @f_tou(float %a) { %r = fptoui float %a to i32
  ret i32 %r }
define double @f_ext(float %a) { %r = fpext float %a to double
  ret double %r }
define float @f_trunc(double %a) { %r = fptrunc double %a to float
  ret float %r }
define float @f_const(i32 %k) {
  %c = icmp eq i32 %k, 0
  %r = select i1 %c, float 1.500000e+00, float -0.000000e+00
  ret float %r
}
define float @f_load(ptr %p, i32 %i) {
  %q = getelementptr float, ptr %p, i32 %i
  %v = load float, ptr %q
  ret float %v
}
define void @f_store(ptr %p, float %v) {
  %q = getelementptr float, ptr %p, i32 3
  store float %v, ptr %q
  ret void
}
declare float @c_fmul(float, float)
; Six float arguments: R1-R4 and then the stack.
define float @f_args(float %a, float %b, float %c, float %d, float %e, float %f) #0 {
  %x = call float @c_fmul(float %e, float %f)
  %y = fadd nnan ninf float %a, %d
  %z = fsub nnan ninf float %y, %x
  %w = fmul nnan ninf float %z, %b
  %v = fdiv nnan ninf float %w, %c
  ret float %v
}
declare float @llvm.fabs.f32(float)
declare float @llvm.copysign.f32(float, float)

; Denormals may be flushed to a zero of the same sign (FPSW.DN = 1).
attributes #0 = { denormal_fpenv(float: preservesign) }
