; Memory access forms in code generation: stores of constants (mov.sz #imm,
; dsp[Rd]), register-indexed loads and stores ([Ri, Rb]), post-increment /
; pre-decrement ([Rn+] / [-Rn]) and memcpy / memset with the string
; instructions. Linked with mem_main.c; scripts/check-mem.sh also checks the
; instructions against mem.expected.
target triple = "rx-none-elf"

; --- stores of constants: the dsp:5 form (#0-255, R0-R7), otherwise the
; shortest displacement and immediate fields
define void @st_imm(ptr %p) {
  store i8 -1, ptr %p
  %b1 = getelementptr i8, ptr %p, i32 31
  store i8 200, ptr %b1
  %b2 = getelementptr i8, ptr %p, i32 300
  store i8 -128, ptr %b2
  %w1 = getelementptr i8, ptr %p, i32 2
  store i16 255, ptr %w1
  %w2 = getelementptr i8, ptr %p, i32 64
  store i16 -2, ptr %w2
  %w3 = getelementptr i8, ptr %p, i32 600
  store i16 4660, ptr %w3
  %l1 = getelementptr i8, ptr %p, i32 4
  store i32 0, ptr %l1
  %l2 = getelementptr i8, ptr %p, i32 124
  store i32 256, ptr %l2
  %l3 = getelementptr i8, ptr %p, i32 128
  store i32 -129, ptr %l3
  %l4 = getelementptr i8, ptr %p, i32 1020
  store i32 8388607, ptr %l4
  %l5 = getelementptr i8, ptr %p, i32 1024
  store i32 305419896, ptr %l5
  %l6 = getelementptr i8, ptr %p, i32 8000
  store i32 -8388608, ptr %l6
  ret void
}

; Base registers above R7 (no dsp:5 form): many live pointers.
define void @st_imm_hi(ptr %a, ptr %b, ptr %c, ptr %d, ptr %e, ptr %f,
                       ptr %g, ptr %h) {
  store i32 1, ptr %a
  store i32 2, ptr %b
  store i32 3, ptr %c
  store i32 4, ptr %d
  store i32 5, ptr %e
  store i32 6, ptr %f
  store i32 7, ptr %g
  store i32 8, ptr %h
  %a1 = getelementptr i8, ptr %a, i32 8
  store i32 100, ptr %a1
  %h1 = getelementptr i8, ptr %h, i32 8
  store i32 100, ptr %h1
  ret void
}

; The same large constant twice: loaded into a register once.
define void @st_twice(ptr %p, ptr %q) {
  store i32 305419896, ptr %p
  store i32 305419896, ptr %q
  ret void
}

; Stores to the stack.
define i32 @st_stack(i32 %i) {
  %x = alloca [8 x i32], align 4
  %x7 = getelementptr [8 x i32], ptr %x, i32 0, i32 7
  store volatile i32 7, ptr %x
  store volatile i32 -1000, ptr %x7
  %xi = getelementptr [8 x i32], ptr %x, i32 0, i32 %i
  %v = load volatile i32, ptr %xi
  ret i32 %v
}

; --- register-indexed: [Ri, Rb] is Rb + Ri * size
define i32 @ix_ld_l(ptr %p, i32 %i) {
  %a = getelementptr i32, ptr %p, i32 %i
  %v = load i32, ptr %a
  ret i32 %v
}
define i32 @ix_ld_sw(ptr %p, i32 %i) {
  %a = getelementptr i16, ptr %p, i32 %i
  %v = load i16, ptr %a
  %e = sext i16 %v to i32
  ret i32 %e
}
define i32 @ix_ld_uw(ptr %p, i32 %i) {
  %a = getelementptr i16, ptr %p, i32 %i
  %v = load i16, ptr %a
  %e = zext i16 %v to i32
  ret i32 %e
}
define i32 @ix_ld_sb(ptr %p, i32 %i) {
  %a = getelementptr i8, ptr %p, i32 %i
  %v = load i8, ptr %a
  %e = sext i8 %v to i32
  ret i32 %e
}
define i32 @ix_ld_ub(ptr %p, i32 %i) {
  %a = getelementptr i8, ptr %p, i32 %i
  %v = load i8, ptr %a
  %e = zext i8 %v to i32
  ret i32 %e
}
define void @ix_st_l(ptr %p, i32 %i, i32 %v) {
  %a = getelementptr i32, ptr %p, i32 %i
  store i32 %v, ptr %a
  ret void
}
define void @ix_st_w(ptr %p, i32 %i, i32 %v) {
  %a = getelementptr i16, ptr %p, i32 %i
  %t = trunc i32 %v to i16
  store i16 %t, ptr %a
  ret void
}
define void @ix_st_b(ptr %p, i32 %i, i32 %v) {
  %a = getelementptr i8, ptr %p, i32 %i
  %t = trunc i32 %v to i8
  store i8 %t, ptr %a
  ret void
}

@ix_table = global [8 x i32] [i32 10, i32 20, i32 30, i32 40, i32 50,
                              i32 60, i32 70, i32 80]
define i32 @ix_glob(i32 %i) {
  %a = getelementptr [8 x i32], ptr @ix_table, i32 0, i32 %i
  %v = load i32, ptr %a
  ret i32 %v
}

; --- post-increment / pre-decrement: mov.sz [Rn+] / [-Rn] in loops that
; walk a pointer by the access size
define i32 @pi_sum_l(ptr %p, i32 %n) {
entry:
  br label %loop
loop:
  %q = phi ptr [%p, %entry], [%q1, %loop]
  %k = phi i32 [%n, %entry], [%k1, %loop]
  %s = phi i32 [0, %entry], [%s1, %loop]
  %v = load i32, ptr %q
  %q1 = getelementptr i32, ptr %q, i32 1
  %s1 = add i32 %s, %v
  %k1 = add i32 %k, -1
  %c = icmp ne i32 %k1, 0
  br i1 %c, label %loop, label %done
done:
  ret i32 %s1
}
define i32 @pi_sum_sw(ptr %p, i32 %n) {
entry:
  br label %loop
loop:
  %q = phi ptr [%p, %entry], [%q1, %loop]
  %k = phi i32 [%n, %entry], [%k1, %loop]
  %s = phi i32 [0, %entry], [%s1, %loop]
  %v = load i16, ptr %q
  %e = sext i16 %v to i32
  %q1 = getelementptr i16, ptr %q, i32 1
  %s1 = add i32 %s, %e
  %k1 = add i32 %k, -1
  %c = icmp ne i32 %k1, 0
  br i1 %c, label %loop, label %done
done:
  ret i32 %s1
}
define i32 @pi_sum_ub(ptr %p, i32 %n) {
entry:
  br label %loop
loop:
  %q = phi ptr [%p, %entry], [%q1, %loop]
  %k = phi i32 [%n, %entry], [%k1, %loop]
  %s = phi i32 [0, %entry], [%s1, %loop]
  %v = load i8, ptr %q
  %e = zext i8 %v to i32
  %q1 = getelementptr i8, ptr %q, i32 1
  %s1 = add i32 %s, %e
  %k1 = add i32 %k, -1
  %c = icmp ne i32 %k1, 0
  br i1 %c, label %loop, label %done
done:
  ret i32 %s1
}
; Fill with ascending values.
define void @pi_fill_w(ptr %p, i32 %n) {
entry:
  br label %loop
loop:
  %q = phi ptr [%p, %entry], [%q1, %loop]
  %k = phi i32 [0, %entry], [%k1, %loop]
  %t = trunc i32 %k to i16
  store i16 %t, ptr %q
  %q1 = getelementptr i16, ptr %q, i32 1
  %k1 = add i32 %k, 1
  %c = icmp ne i32 %k1, %n
  br i1 %c, label %loop, label %done
done:
  ret void
}
; Fill downwards from the end: *--q = k.
define void @pd_fill_l(ptr %end, i32 %n) {
entry:
  br label %loop
loop:
  %q = phi ptr [%end, %entry], [%q1, %loop]
  %k = phi i32 [0, %entry], [%k1, %loop]
  %q1 = getelementptr i32, ptr %q, i32 -1
  store i32 %k, ptr %q1
  %k1 = add i32 %k, 1
  %c = icmp ne i32 %k1, %n
  br i1 %c, label %loop, label %done
done:
  ret void
}
; Sum downwards from the end: s += *--q (bytes, sign-extended).
define i32 @pd_sum_sb(ptr %end, i32 %n) {
entry:
  br label %loop
loop:
  %q = phi ptr [%end, %entry], [%q1, %loop]
  %k = phi i32 [%n, %entry], [%k1, %loop]
  %s = phi i32 [0, %entry], [%s1, %loop]
  %q1 = getelementptr i8, ptr %q, i32 -1
  %v = load i8, ptr %q1
  %e = sext i8 %v to i32
  %s1 = add i32 %s, %e
  %k1 = add i32 %k, -1
  %c = icmp ne i32 %k1, 0
  br i1 %c, label %loop, label %done
done:
  ret i32 %s1
}

; --- memcpy / memset that are not expanded to loads and stores: smovf,
; sstr.b, and sstr.l for a constant value over a multiple of 4 bytes
declare void @llvm.memcpy.p0.p0.i32(ptr, ptr, i32, i1)
declare void @llvm.memset.p0.i32(ptr, i8, i32, i1)

define void @sm_copy(ptr %d, ptr %s, i32 %n) {
  call void @llvm.memcpy.p0.p0.i32(ptr %d, ptr %s, i32 %n, i1 false)
  ret void
}
define void @sm_copy400(ptr %d, ptr %s) {
  call void @llvm.memcpy.p0.p0.i32(ptr align 4 %d, ptr align 4 %s, i32 400,
                                   i1 false)
  ret void
}
define void @sm_set(ptr %d, i32 %v, i32 %n) {
  %b = trunc i32 %v to i8
  call void @llvm.memset.p0.i32(ptr %d, i8 %b, i32 %n, i1 false)
  ret void
}
define void @sm_zero400(ptr %d) {
  call void @llvm.memset.p0.i32(ptr align 4 %d, i8 0, i32 400, i1 false)
  ret void
}
define void @sm_ff401(ptr %d) {
  call void @llvm.memset.p0.i32(ptr %d, i8 -1, i32 401, i1 false)
  ret void
}
; The size stays live across smovf (which clobbers R1-R3).
define i32 @sm_live(ptr %d, ptr %s, i32 %n, i32 %k) {
  call void @llvm.memcpy.p0.p0.i32(ptr %d, ptr %s, i32 %n, i1 false)
  %r = add i32 %k, %n
  ret i32 %r
}
