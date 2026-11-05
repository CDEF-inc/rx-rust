; float and double without fast-math flags: with -mattr=+flush-denormals
; (run-codegen-test.sh ATTRS) they use the FPU / DFPU, otherwise libgcc;
; either way NaNs, infinities and signed zeros must give the IEEE results,
; which fpu_ieee_main.c takes from libgcc. Denormals are left out: the FPU
; flushes them (fpu.ll / dfpu.ll check that).
target triple = "rx-none-elf"

define float @f_fadd(float %a, float %b) { %r = fadd float %a, %b
  ret float %r }
define float @f_fsub(float %a, float %b) { %r = fsub float %a, %b
  ret float %r }
define float @f_fmul(float %a, float %b) { %r = fmul float %a, %b
  ret float %r }
define float @f_fdiv(float %a, float %b) { %r = fdiv float %a, %b
  ret float %r }
define i32 @f_oeq(float %a, float %b) { %c = fcmp oeq float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_ogt(float %a, float %b) { %c = fcmp ogt float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_oge(float %a, float %b) { %c = fcmp oge float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_olt(float %a, float %b) { %c = fcmp olt float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_ole(float %a, float %b) { %c = fcmp ole float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_one(float %a, float %b) { %c = fcmp one float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_ord(float %a, float %b) { %c = fcmp ord float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_ueq(float %a, float %b) { %c = fcmp ueq float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_ugt(float %a, float %b) { %c = fcmp ugt float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_uge(float %a, float %b) { %c = fcmp uge float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_ult(float %a, float %b) { %c = fcmp ult float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_ule(float %a, float %b) { %c = fcmp ule float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_une(float %a, float %b) { %c = fcmp une float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @f_uno(float %a, float %b) { %c = fcmp uno float %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define float @f_sel(float %a, float %b) { %c = fcmp ogt float %a, %b
  %r = select i1 %c, float %a, float %b
  ret float %r }
define i32 @f_br(float %a, float %b) {
entry:
  %c = fcmp uge float %a, %b
  br i1 %c, label %y, label %n
y:
  ret i32 7
n:
  ret i32 3
}
define float @f_selueq(float %a, float %b) { %c = fcmp ueq float %a, %b
  %r = select i1 %c, float %a, float %b
  ret float %r }
define i32 @f_brone(float %a, float %b) {
entry:
  %c = fcmp one float %a, %b
  br i1 %c, label %y, label %n
y:
  ret i32 7
n:
  ret i32 3
}
define float @f_min(float %a, float %b) {
  %r = call float @llvm.minnum.f32(float %a, float %b)
  ret float %r }
define float @f_max(float %a, float %b) {
  %r = call float @llvm.maxnum.f32(float %a, float %b)
  ret float %r }
define float @f_min_fast(float %a, float %b) {
  %r = call nnan nsz float @llvm.minnum.f32(float %a, float %b)
  ret float %r }
declare float @llvm.minnum.f32(float, float)
declare float @llvm.maxnum.f32(float, float)
define i32 @f_toi(float %a) { %r = fptosi float %a to i32
  ret i32 %r }
define i32 @f_tou(float %a) { %r = fptoui float %a to i32
  ret i32 %r }
define i32 @f_sat(float %a) { %r = call i32 @llvm.fptosi.sat.i32.f32(float %a)
  ret i32 %r }
define i32 @f_usat(float %a) { %r = call i32 @llvm.fptoui.sat.i32.f32(float %a)
  ret i32 %r }
declare i32 @llvm.fptosi.sat.i32.f32(float)
declare i32 @llvm.fptoui.sat.i32.f32(float)

define double @d_fadd(double %a, double %b) { %r = fadd double %a, %b
  ret double %r }
define double @d_fsub(double %a, double %b) { %r = fsub double %a, %b
  ret double %r }
define double @d_fmul(double %a, double %b) { %r = fmul double %a, %b
  ret double %r }
define double @d_fdiv(double %a, double %b) { %r = fdiv double %a, %b
  ret double %r }
define i32 @d_oeq(double %a, double %b) { %c = fcmp oeq double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_ogt(double %a, double %b) { %c = fcmp ogt double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_oge(double %a, double %b) { %c = fcmp oge double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_olt(double %a, double %b) { %c = fcmp olt double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_ole(double %a, double %b) { %c = fcmp ole double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_one(double %a, double %b) { %c = fcmp one double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_ord(double %a, double %b) { %c = fcmp ord double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_ueq(double %a, double %b) { %c = fcmp ueq double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_ugt(double %a, double %b) { %c = fcmp ugt double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_uge(double %a, double %b) { %c = fcmp uge double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_ult(double %a, double %b) { %c = fcmp ult double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_ule(double %a, double %b) { %c = fcmp ule double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_une(double %a, double %b) { %c = fcmp une double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_uno(double %a, double %b) { %c = fcmp uno double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define double @d_sel(double %a, double %b) { %c = fcmp ogt double %a, %b
  %r = select i1 %c, double %a, double %b
  ret double %r }
define i32 @d_br(double %a, double %b) {
entry:
  %c = fcmp uge double %a, %b
  br i1 %c, label %y, label %n
y:
  ret i32 7
n:
  ret i32 3
}
define double @d_selueq(double %a, double %b) { %c = fcmp ueq double %a, %b
  %r = select i1 %c, double %a, double %b
  ret double %r }
define i32 @d_brone(double %a, double %b) {
entry:
  %c = fcmp one double %a, %b
  br i1 %c, label %y, label %n
y:
  ret i32 7
n:
  ret i32 3
}
define double @d_min(double %a, double %b) {
  %r = call double @llvm.minnum.f64(double %a, double %b)
  ret double %r }
define double @d_max(double %a, double %b) {
  %r = call double @llvm.maxnum.f64(double %a, double %b)
  ret double %r }
define double @d_max_fast(double %a, double %b) {
  %r = call nnan nsz double @llvm.maxnum.f64(double %a, double %b)
  ret double %r }
declare double @llvm.minnum.f64(double, double)
declare double @llvm.maxnum.f64(double, double)
define i32 @d_toi(double %a) { %r = fptosi double %a to i32
  ret i32 %r }
define i32 @d_tou(double %a) { %r = fptoui double %a to i32
  ret i32 %r }
define i32 @d_sat(double %a) { %r = call i32 @llvm.fptosi.sat.i32.d64(double %a)
  ret i32 %r }
define i32 @d_usat(double %a) { %r = call i32 @llvm.fptoui.sat.i32.d64(double %a)
  ret i32 %r }
declare i32 @llvm.fptosi.sat.i32.d64(double)
declare i32 @llvm.fptoui.sat.i32.d64(double)

define double @f_ext(float %a) { %r = fpext float %a to double
  ret double %r }
define float @d_trunc(double %a) { %r = fptrunc double %a to float
  ret float %r }
define double @d_sqrt(double %a) { %r = call double @llvm.sqrt.f64(double %a)
  ret double %r }
declare double @llvm.sqrt.f64(double)
