target triple = "rx-none-elf"

define i64 @add64(i64 %a, i64 %b) {
  %r = add i64 %a, %b
  ret i64 %r
}

define i64 @sub64(i64 %a, i64 %b) {
  %r = sub i64 %a, %b
  ret i64 %r
}

define i64 @pos3(i32 %a, i32 %b, i32 %c, i64 %x, i32 %y) {
  %e = sext i32 %y to i64
  %r = add i64 %x, %e
  ret i64 %r
}

define double @dmul(double %a, double %b) {
  %r = fmul double %a, %b
  ret double %r
}

define float @fadd(float %a, float %b) {
  %r = fadd float %a, %b
  ret float %r
}

declare i32 @va(i32, ...)
define i32 @callva(double %d) {
  %r = call i32 (i32, ...) @va(i32 1, i32 2, double %d)
  ret i32 %r
}
