; double with +dfpu (RXv3): fast-math arithmetic and compares in functions
; that allow flushing denormals (#0), and integer to double conversions, use
; the DFPU through DR0 / DR1; everything else stays soft-float. Run with
; dfpu_main.c on a core with the DFPU (MCPU=rxv3 QEMU_CPU=rxv3
; scripts/run-codegen-test.sh, CDEF-inc/qemu); scripts/check-dfpu.sh also
; checks the instruction sequences.
target triple = "rx-none-elf"

define double @da_add(double %a, double %b) #0 { %r = fadd nnan ninf double %a, %b
  ret double %r }
define double @da_sub(double %a, double %b) #0 { %r = fsub nnan ninf double %a, %b
  ret double %r }
define double @da_mul(double %a, double %b) #0 { %r = fmul fast double %a, %b
  ret double %r }
define double @da_div(double %a, double %b) #0 { %r = fdiv nnan ninf double %a, %b
  ret double %r }
define double @da_sqrt(double %a) #0 {
  %r = call nnan ninf double @llvm.sqrt.f64(double %a)
  ret double %r }
define double @da_chain(double %a, double %b, double %c) #0 {
  %x = fmul nnan ninf double %a, %b
  %y = fsub nnan ninf double %x, %c
  %z = fdiv nnan ninf double %y, %a
  ret double %z }
define i32 @da_olt(double %a, double %b) #0 { %c = fcmp nnan ninf olt double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @da_ole(double %a, double %b) #0 { %c = fcmp nnan ninf ole double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @da_ogt(double %a, double %b) #0 { %c = fcmp fast ogt double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @da_oge(double %a, double %b) #0 { %c = fcmp fast oge double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @da_oeq(double %a, double %b) #0 { %c = fcmp fast oeq double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @da_une(double %a, double %b) #0 { %c = fcmp fast une double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define double @da_max(double %a, double %b) #0 {
  %c = fcmp fast ogt double %a, %b
  %r = select i1 %c, double %a, double %b
  ret double %r }
define i32 @da_branch(double %a, double %b) #0 {
entry:
  %c = fcmp fast olt double %a, %b
  br i1 %c, label %lt, label %ge
lt:
  ret i32 -1
ge:
  ret i32 1
}
define double @d_fromi(i32 %a) { %r = sitofp i32 %a to double
  ret double %r }
define double @d_fromu(i32 %a) { %r = uitofp i32 %a to double
  ret double %r }
; IEEE (no fast-math flags): libcalls as before.
define double @d_add(double %a, double %b) { %r = fadd double %a, %b
  ret double %r }
; nnan ninf, but the default (IEEE) denormal environment.
define double @da_add_ieee(double %a, double %b) { %r = fadd nnan ninf double %a, %b
  ret double %r }
define i32 @da_olt_ieee(double %a, double %b) { %c = fcmp nnan ninf olt double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_olt(double %a, double %b) { %c = fcmp olt double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @d_uno(double %a, double %b) { %c = fcmp fast uno double %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define double @d_sqrt(double %a) {
  %r = call double @llvm.sqrt.f64(double %a)
  ret double %r }
define double @d_fromll(i64 %a) { %r = sitofp i64 %a to double
  ret double %r }
define i32 @d_toi(double %a) { %r = fptosi double %a to i32
  ret i32 %r }
; Values live across calls stay in general registers.
declare double @ext(double)
define double @da_call(double %a, double %b) #0 {
  %x = fadd nnan ninf double %a, %b
  %y = call double @ext(double %x)
  %z = fmul nnan ninf double %y, %a
  ret double %z }
declare double @llvm.sqrt.f64(double)

; Denormals may be flushed to a zero of the same sign (DPSW.DDN = 1).
attributes #0 = { denormal_fpenv(preservesign) }
