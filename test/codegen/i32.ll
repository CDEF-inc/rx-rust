; i32 code generation test: compiled with llc, linked with main.c (GCC) and
; run under QEMU by scripts/run-codegen-test.sh.
target triple = "rx-none-elf"

; --- arithmetic / logic
define i32 @t_add(i32 %a, i32 %b) { %r = add i32 %a, %b
  ret i32 %r }
define i32 @t_sub(i32 %a, i32 %b) { %r = sub i32 %a, %b
  ret i32 %r }
define i32 @t_mul(i32 %a, i32 %b) { %r = mul i32 %a, %b
  ret i32 %r }
define i32 @t_sdiv(i32 %a, i32 %b) { %r = sdiv i32 %a, %b
  ret i32 %r }
define i32 @t_udiv(i32 %a, i32 %b) { %r = udiv i32 %a, %b
  ret i32 %r }
define i32 @t_srem(i32 %a, i32 %b) { %r = srem i32 %a, %b
  ret i32 %r }
define i32 @t_urem(i32 %a, i32 %b) { %r = urem i32 %a, %b
  ret i32 %r }
define i32 @t_and(i32 %a, i32 %b) { %r = and i32 %a, %b
  ret i32 %r }
define i32 @t_or(i32 %a, i32 %b) { %r = or i32 %a, %b
  ret i32 %r }
define i32 @t_xor(i32 %a, i32 %b) { %r = xor i32 %a, %b
  ret i32 %r }
define i32 @t_shl(i32 %a, i32 %b) { %r = shl i32 %a, %b
  ret i32 %r }
define i32 @t_lshr(i32 %a, i32 %b) { %r = lshr i32 %a, %b
  ret i32 %r }
define i32 @t_ashr(i32 %a, i32 %b) { %r = ashr i32 %a, %b
  ret i32 %r }
define i32 @t_shl7(i32 %a) { %r = shl i32 %a, 7
  ret i32 %r }
define i32 @t_lshr20(i32 %a) { %r = lshr i32 %a, 20
  ret i32 %r }
define i32 @t_ashr31(i32 %a) { %r = ashr i32 %a, 31
  ret i32 %r }
define i32 @t_neg(i32 %a) { %r = sub i32 0, %a
  ret i32 %r }
define i32 @t_not(i32 %a) { %r = xor i32 %a, -1
  ret i32 %r }
define i32 @t_imm(i32 %a) {
  %x = mul i32 %a, 100
  %y = and i32 %x, 65280
  %z = or i32 %y, 305419896
  %w = xor i32 %z, -129
  %v = add i32 %w, -70000
  ret i32 %v
}
define i32 @t_rsub(i32 %a) { %r = sub i32 1000, %a
  ret i32 %r }

; --- extensions and bit operations
define i32 @t_sext8(i32 %a) { %t = trunc i32 %a to i8
  %r = sext i8 %t to i32
  ret i32 %r }
define i32 @t_zext8(i32 %a) { %t = trunc i32 %a to i8
  %r = zext i8 %t to i32
  ret i32 %r }
define i32 @t_sext16(i32 %a) { %t = trunc i32 %a to i16
  %r = sext i16 %t to i32
  ret i32 %r }
define i32 @t_zext16(i32 %a) { %t = trunc i32 %a to i16
  %r = zext i16 %t to i32
  ret i32 %r }
declare i32 @llvm.bswap.i32(i32)
define i32 @t_bswap(i32 %a) { %r = call i32 @llvm.bswap.i32(i32 %a)
  ret i32 %r }
declare i32 @llvm.smax.i32(i32, i32)
declare i32 @llvm.smin.i32(i32, i32)
declare i32 @llvm.umax.i32(i32, i32)
declare i32 @llvm.abs.i32(i32, i1)
declare i32 @llvm.ctpop.i32(i32)
declare i32 @llvm.ctlz.i32(i32, i1)
define i32 @t_smax(i32 %a, i32 %b) { %r = call i32 @llvm.smax.i32(i32 %a, i32 %b)
  ret i32 %r }
define i32 @t_smin(i32 %a, i32 %b) { %r = call i32 @llvm.smin.i32(i32 %a, i32 %b)
  ret i32 %r }
define i32 @t_umax(i32 %a, i32 %b) { %r = call i32 @llvm.umax.i32(i32 %a, i32 %b)
  ret i32 %r }
define i32 @t_abs(i32 %a) { %r = call i32 @llvm.abs.i32(i32 %a, i1 false)
  ret i32 %r }
define i32 @t_popcount(i32 %a) { %r = call i32 @llvm.ctpop.i32(i32 %a)
  ret i32 %r }
define i32 @t_clz(i32 %a) { %r = call i32 @llvm.ctlz.i32(i32 %a, i1 false)
  ret i32 %r }

; --- comparisons (0/1 results)
define i32 @t_eq(i32 %a, i32 %b) { %c = icmp eq i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_ne(i32 %a, i32 %b) { %c = icmp ne i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_slt(i32 %a, i32 %b) { %c = icmp slt i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_sle(i32 %a, i32 %b) { %c = icmp sle i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_sgt(i32 %a, i32 %b) { %c = icmp sgt i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_sge(i32 %a, i32 %b) { %c = icmp sge i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_ult(i32 %a, i32 %b) { %c = icmp ult i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_ule(i32 %a, i32 %b) { %c = icmp ule i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_ugt(i32 %a, i32 %b) { %c = icmp ugt i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_uge(i32 %a, i32 %b) { %c = icmp uge i32 %a, %b
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_lt100(i32 %a) { %c = icmp slt i32 %a, 100
  %r = zext i1 %c to i32
  ret i32 %r }
define i32 @t_select(i32 %a, i32 %b, i32 %x, i32 %y) {
  %c = icmp ult i32 %a, %b
  %r = select i1 %c, i32 %x, i32 %y
  ret i32 %r
}

; --- control flow
define i32 @t_collatz(i32 %n) {
entry:
  br label %loop
loop:
  %x = phi i32 [ %n, %entry ], [ %next, %step ]
  %steps = phi i32 [ 0, %entry ], [ %s1, %step ]
  %done = icmp ule i32 %x, 1
  br i1 %done, label %exit, label %body
body:
  %odd = and i32 %x, 1
  %isodd = icmp ne i32 %odd, 0
  br i1 %isodd, label %oddb, label %evenb
oddb:
  %m = mul i32 %x, 3
  %o = add i32 %m, 1
  br label %step
evenb:
  %e = lshr i32 %x, 1
  br label %step
step:
  %next = phi i32 [ %o, %oddb ], [ %e, %evenb ]
  %s1 = add i32 %steps, 1
  br label %loop
exit:
  ret i32 %steps
}

define i32 @t_fib(i32 %n) {
entry:
  %small = icmp slt i32 %n, 2
  br i1 %small, label %base, label %rec
base:
  ret i32 %n
rec:
  %n1 = sub i32 %n, 1
  %n2 = sub i32 %n, 2
  %f1 = call i32 @t_fib(i32 %n1)
  %f2 = call i32 @t_fib(i32 %n2)
  %r = add i32 %f1, %f2
  ret i32 %r
}

; A conditional branch over more than 128 bytes (needs branch relaxation);
; the volatile stores keep LLVM from folding the chain.
@g_sink = global i32 0
define i32 @t_longbranch(i32 %a) {
entry:
  %c = icmp eq i32 %a, 0
  br i1 %c, label %exit, label %big
big:
  %v0 = mul i32 %a, 305419896
  store volatile i32 %v0, ptr @g_sink
  %v1 = add i32 %v0, 19088743
  store volatile i32 %v1, ptr @g_sink
  %v2 = mul i32 %v1, 287454020
  store volatile i32 %v2, ptr @g_sink
  %v3 = add i32 %v2, 1450744508
  store volatile i32 %v3, ptr @g_sink
  %v4 = mul i32 %v3, 591751049
  store volatile i32 %v4, ptr @g_sink
  %v5 = add i32 %v4, 286331153
  store volatile i32 %v5, ptr @g_sink
  %v6 = mul i32 %v5, 1985229328
  store volatile i32 %v6, ptr @g_sink
  %v7 = add i32 %v6, 2023406814
  store volatile i32 %v7, ptr @g_sink
  %v8 = mul i32 %v7, 1515870810
  store volatile i32 %v8, ptr @g_sink
  %v9 = add i32 %v8, 1431655765
  store volatile i32 %v9, ptr @g_sink
  %v10 = mul i32 %v9, 858993459
  store volatile i32 %v10, ptr @g_sink
  %v11 = add i32 %v10, 1717986918
  store volatile i32 %v11, ptr @g_sink
  %v12 = mul i32 %v11, 2004318071
  store volatile i32 %v12, ptr @g_sink
  %v13 = add i32 %v12, 1145324612
  store volatile i32 %v13, ptr @g_sink
  %v14 = mul i32 %v13, 572662306
  store volatile i32 %v14, ptr @g_sink
  %v15 = add i32 %v14, 1999999999
  store volatile i32 %v15, ptr @g_sink
  %v16 = mul i32 %v15, 1234567890
  store volatile i32 %v16, ptr @g_sink
  %v17 = add i32 %v16, 987654321
  store volatile i32 %v17, ptr @g_sink
  %v18 = mul i32 %v17, 1357913579
  store volatile i32 %v18, ptr @g_sink
  %v19 = add i32 %v18, 246802468
  store volatile i32 %v19, ptr @g_sink
  %v20 = mul i32 %v19, 1122334455
  store volatile i32 %v20, ptr @g_sink
  %v21 = add i32 %v20, 1928374655
  store volatile i32 %v21, ptr @g_sink
  %v22 = mul i32 %v21, 1029384756
  store volatile i32 %v22, ptr @g_sink
  %v23 = add i32 %v22, 1203948576
  store volatile i32 %v23, ptr @g_sink
  %v24 = mul i32 %v23, 1111111111
  store volatile i32 %v24, ptr @g_sink
  %v25 = add i32 %v24, 1222222222
  store volatile i32 %v25, ptr @g_sink
  br label %exit
exit:
  %r = phi i32 [ -1, %entry ], [ %v25, %big ]
  ret i32 %r
}

; --- memory
@g_table = global [8 x i32] [i32 1, i32 -2, i32 3, i32 -4, i32 5, i32 -6, i32 7, i32 -8]
@g_counter = global i32 0
@g_bytes = global [4 x i8] [i8 -1, i8 127, i8 -128, i8 1]
@g_half = global [2 x i16] [i16 -32768, i16 32767]

define i32 @t_global_sum() {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %i1, %loop ]
  %s = phi i32 [ 0, %entry ], [ %s1, %loop ]
  %p = getelementptr [8 x i32], ptr @g_table, i32 0, i32 %i
  %v = load i32, ptr %p
  %s1 = add i32 %s, %v
  %i1 = add i32 %i, 1
  %c = icmp ne i32 %i1, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %s1
}

define i32 @t_counter(i32 %d) {
  %v = load volatile i32, ptr @g_counter
  %n = add i32 %v, %d
  store volatile i32 %n, ptr @g_counter
  ret i32 %n
}

; Sum of sign-extended bytes, zero-extended bytes and both halves.
define i32 @t_narrow() {
  %b0 = load i8, ptr @g_bytes
  %p1 = getelementptr i8, ptr @g_bytes, i32 1
  %b1 = load i8, ptr %p1
  %p2 = getelementptr i8, ptr @g_bytes, i32 2
  %b2 = load i8, ptr %p2
  %s0 = sext i8 %b0 to i32
  %s1 = zext i8 %b1 to i32
  %s2 = zext i8 %b2 to i32
  %h0 = load i16, ptr @g_half
  %ph1 = getelementptr i16, ptr @g_half, i32 1
  %h1 = load i16, ptr %ph1
  %x0 = sext i16 %h0 to i32
  %x1 = zext i16 %h1 to i32
  %a = add i32 %s0, %s1
  %b = add i32 %a, %s2
  %c = add i32 %b, %x0
  %d = add i32 %c, %x1
  ret i32 %d
}

; Stores through a pointer argument at several offsets.
define void @t_store(ptr %p, i32 %v) {
  store i32 %v, ptr %p
  %q1 = getelementptr i8, ptr %p, i32 4
  %t8 = trunc i32 %v to i8
  store i8 %t8, ptr %q1
  %q2 = getelementptr i8, ptr %p, i32 6
  %t16 = trunc i32 %v to i16
  store i16 %t16, ptr %q2
  %q3 = getelementptr i8, ptr %p, i32 400
  store i32 %v, ptr %q3
  ret void
}

; Local array with dynamic indexing (frame indices, stack addressing).
define i32 @t_stack(i32 %n) {
entry:
  %arr = alloca [16 x i32], align 4
  br label %fill
fill:
  %i = phi i32 [ 0, %entry ], [ %i1, %fill ]
  %p = getelementptr [16 x i32], ptr %arr, i32 0, i32 %i
  %sq = mul i32 %i, %i
  store i32 %sq, ptr %p
  %i1 = add i32 %i, 1
  %c = icmp ne i32 %i1, 16
  br i1 %c, label %fill, label %sum
sum:
  %j = phi i32 [ 0, %fill ], [ %j1, %sum ]
  %s = phi i32 [ 0, %fill ], [ %s1, %sum ]
  %q = getelementptr [16 x i32], ptr %arr, i32 0, i32 %j
  %v = load i32, ptr %q
  %s1 = add i32 %s, %v
  %j1 = add i32 %j, 1
  %d = icmp slt i32 %j1, %n
  br i1 %d, label %sum, label %exit
exit:
  ret i32 %s1
}

; --- calls
; Eight arguments: four in registers, four on the stack.
define i32 @t_args8(i32 %a, i32 %b, i32 %c, i32 %d, i32 %e, i32 %f, i32 %g,
                    i32 %h) {
  %m1 = mul i32 %b, 2
  %m2 = mul i32 %c, 3
  %m3 = mul i32 %d, 4
  %m4 = mul i32 %e, 5
  %m5 = mul i32 %f, 6
  %m6 = mul i32 %g, 7
  %m7 = mul i32 %h, 8
  %s1 = add i32 %a, %m1
  %s2 = add i32 %s1, %m2
  %s3 = add i32 %s2, %m3
  %s4 = add i32 %s3, %m4
  %s5 = add i32 %s4, %m5
  %s6 = add i32 %s5, %m6
  %s7 = add i32 %s6, %m7
  ret i32 %s7
}

; Calls a GCC-compiled function with stack arguments and keeps values live
; across the call (callee-saved registers).
declare i32 @c_weighted6(i32, i32, i32, i32, i32, i32)
define i32 @t_call_c(i32 %x, i32 %y) {
  %a = mul i32 %x, 3
  %b = add i32 %y, 7
  %r = call i32 @c_weighted6(i32 %x, i32 %y, i32 %a, i32 %b, i32 50, i32 60)
  %s = add i32 %r, %a
  %t = add i32 %s, %b
  ret i32 %t
}

; Small integer arguments and results are promoted (GCC: char/short -> int).
define signext i8 @t_char(i8 signext %a, i8 signext %b) {
  %r = add i8 %a, %b
  ret i8 %r
}
define zeroext i16 @t_ushort(i16 zeroext %a) {
  %r = mul i16 %a, 3
  ret i16 %r
}
