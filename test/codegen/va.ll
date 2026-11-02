; Variadic function definitions (va_start / va_arg / va_copy), called by
; va_main.c (GCC): all arguments of a variadic function are on the stack,
; 4-byte aligned, including the last named one (docs/NOTES-codegen.md).
; va_main.c also passes a va_list made here to GCC's c_vsum.
target triple = "rx-none-elf"

declare void @llvm.va_start.p0(ptr)
declare void @llvm.va_end.p0(ptr)
declare void @llvm.va_copy.p0(ptr, ptr)
declare i32 @c_vsum(i32, ptr)

; int va_sum(int n, ...): n ints
define i32 @va_sum(i32 %n, ...) {
entry:
  %ap = alloca ptr
  call void @llvm.va_start.p0(ptr %ap)
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %i1, %body ]
  %s = phi i32 [ 0, %entry ], [ %s1, %body ]
  %c = icmp slt i32 %i, %n
  br i1 %c, label %body, label %done
body:
  %v = va_arg ptr %ap, i32
  %s1 = add i32 %s, %v
  %i1 = add i32 %i, 1
  br label %loop
done:
  call void @llvm.va_end.p0(ptr %ap)
  ret i32 %s
}

; long long va_mixed(int a, int b, int c, ...): long long, int, double
; (returns the double's high word added in), a pointer
define i64 @va_mixed(i32 %a, i32 %b, i32 %c, ...) {
  %ap = alloca ptr
  call void @llvm.va_start.p0(ptr %ap)
  %x = va_arg ptr %ap, i64
  %y = va_arg ptr %ap, i32
  %d = va_arg ptr %ap, double
  %p = va_arg ptr %ap, ptr
  call void @llvm.va_end.p0(ptr %ap)
  %di = bitcast double %d to i64
  %dh = lshr i64 %di, 32
  %pv = load i32, ptr %p
  %ye = sext i32 %y to i64
  %pe = sext i32 %pv to i64
  %ab = add i32 %a, %b
  %abc = add i32 %ab, %c
  %abce = sext i32 %abc to i64
  %r1 = add i64 %x, %ye
  %r2 = add i64 %r1, %dh
  %r3 = add i64 %r2, %pe
  %r4 = add i64 %r3, %abce
  ret i64 %r4
}

; double va_dsum(int n, ...): n doubles (soft-float adds)
define double @va_dsum(i32 %n, ...) {
entry:
  %ap = alloca ptr
  call void @llvm.va_start.p0(ptr %ap)
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %i1, %body ]
  %s = phi double [ 0.0, %entry ], [ %s1, %body ]
  %c = icmp slt i32 %i, %n
  br i1 %c, label %body, label %done
body:
  %v = va_arg ptr %ap, double
  %s1 = fadd double %s, %v
  %i1 = add i32 %i, 1
  br label %loop
done:
  call void @llvm.va_end.p0(ptr %ap)
  ret double %s
}

; va_copy: read the first argument twice
define i32 @va_twice(i32 %n, ...) {
  %ap = alloca ptr
  %aq = alloca ptr
  call void @llvm.va_start.p0(ptr %ap)
  call void @llvm.va_copy.p0(ptr %aq, ptr %ap)
  %x = va_arg ptr %ap, i32
  %y = va_arg ptr %ap, i32
  %z = va_arg ptr %aq, i32
  call void @llvm.va_end.p0(ptr %aq)
  call void @llvm.va_end.p0(ptr %ap)
  %m = mul i32 %z, 1000
  %s = add i32 %x, %y
  %r = add i32 %s, %m
  ret i32 %r
}

; a va_list passed on to GCC code
define i32 @va_fwd(i32 %n, ...) {
  %ap = alloca ptr
  call void @llvm.va_start.p0(ptr %ap)
  %list = load ptr, ptr %ap
  %r = call i32 @c_vsum(i32 %n, ptr %list)
  call void @llvm.va_end.p0(ptr %ap)
  ret i32 %r
}

; with locals and callee-saved registers, so that the arguments are found
; above them
define i32 @va_frame(i32 %n, ...) {
  %buf = alloca [5 x i32]
  %ap = alloca ptr
  call void asm sideeffect "mov.l #0, r6\0A\09mov.l #0, r9", "~{r6},~{r9}"()
  call void @llvm.va_start.p0(ptr %ap)
  %a = va_arg ptr %ap, i32
  %b = va_arg ptr %ap, i32
  call void @llvm.va_end.p0(ptr %ap)
  %q = getelementptr i32, ptr %buf, i32 2
  store volatile i32 %a, ptr %q
  %v = load volatile i32, ptr %q
  %r = sub i32 %v, %b
  %rn = add i32 %r, %n
  ret i32 %rn
}
