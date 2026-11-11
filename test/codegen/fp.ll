; Frame pointer (R6, as in GCC), dynamic alloca and stack realignment.
; Linked with fp_main.c,
; which checks the results and that R6-R13 survive the calls;
; scripts/check-fp.sh also checks the instructions and the CFI.
target triple = "rx-none-elf"

declare void @touch(ptr, i32)
declare i32 @c_sum6(i32, i32, i32, i32, i32, i32)
declare ptr @llvm.stacksave.p0()
declare void @llvm.stackrestore.p0(ptr)

%S3 = type { i8, i8, i8 }
declare i32 @c_s3(ptr byval(%S3) align 1)
declare i32 @c_mis(ptr, i32)

define internal i32 @elem(ptr %p, i32 %i) {
  %q = getelementptr i32, ptr %p, i32 %i
  %v = load i32, ptr %q
  ret i32 %v
}

; --- frame-pointer=all: locals and the incoming stack arguments through R6
define i32 @fp_locals(i32 %n) #0 {
  %p = alloca [8 x i32]
  call void @touch(ptr %p, i32 8)
  %v = call i32 @elem(ptr %p, i32 %n)
  ret i32 %v
}

define i32 @fp_args(i32 %a, i32 %b, i32 %c, i32 %d, i32 %e, i32 %f) #0 {
  %p = alloca [4 x i32]
  call void @touch(ptr %p, i32 4)
  %v = call i32 @elem(ptr %p, i32 3)
  %s1 = mul i32 %e, 100
  %s2 = mul i32 %f, 10000
  %s3 = add i32 %s1, %s2
  %s4 = add i32 %s3, %a
  %s5 = add i32 %s4, %d
  %s6 = add i32 %s5, %v
  ret i32 %s6
}

; no locals: mov.l r0, r6
define i32 @fp_leaf(i32 %a) #0 {
  %r = add i32 %a, 7
  ret i32 %r
}

; a frame over rtsd's 1020 bytes
define i32 @fp_big(i32 %n) #0 {
  %p = alloca [300 x i32]
  call void @touch(ptr %p, i32 300)
  %v = call i32 @elem(ptr %p, i32 %n)
  ret i32 %v
}

; --- dynamic alloca: R6 is set up without frame-pointer=all
define i32 @dyn(i32 %n) {
  %p = alloca i32, i32 %n
  call void @touch(ptr %p, i32 %n)
  %i = sub i32 %n, 1
  %v = call i32 @elem(ptr %p, i32 %i)
  ret i32 %v
}

; a call with stack arguments after the alloca: the call allocates its own
; argument area below it
define i32 @dyn_call(i32 %n) {
  %p = alloca i32, i32 %n
  call void @touch(ptr %p, i32 %n)
  %s = call i32 @c_sum6(i32 1, i32 2, i32 3, i32 4, i32 5, i32 %n)
  %v = call i32 @elem(ptr %p, i32 0)
  %r = add i32 %s, %v
  ret i32 %r
}

; alloca in a loop with stacksave / stackrestore, and a fixed local
define i32 @dyn_loop(i32 %n) {
entry:
  %fixed = alloca [2 x i32]
  call void @touch(ptr %fixed, i32 2)
  br label %loop
loop:
  %i = phi i32 [ 1, %entry ], [ %i1, %loop ]
  %acc = phi i32 [ 0, %entry ], [ %acc1, %loop ]
  %sp = call ptr @llvm.stacksave.p0()
  %p = alloca i32, i32 %i
  call void @touch(ptr %p, i32 %i)
  %last = sub i32 %i, 1
  %v = call i32 @elem(ptr %p, i32 %last)
  call void @llvm.stackrestore.p0(ptr %sp)
  %acc1 = add i32 %acc, %v
  %i1 = add i32 %i, 1
  %c = icmp sle i32 %i1, %n
  br i1 %c, label %loop, label %exit
exit:
  %f = call i32 @elem(ptr %fixed, i32 1)
  %r = add i32 %acc1, %f
  ret i32 %r
}

; a large fixed frame and a dynamic alloca
define i32 @dyn_big(i32 %n) {
  %fixed = alloca [300 x i32]
  call void @touch(ptr %fixed, i32 300)
  %p = alloca i32, i32 %n
  call void @touch(ptr %p, i32 %n)
  %a = call i32 @elem(ptr %fixed, i32 299)
  %b = call i32 @elem(ptr %p, i32 0)
  %r = add i32 %a, %b
  ret i32 %r
}

; a 3-byte byval argument after a dynamic alloca: the call's argument area
; is rounded up to 4 bytes, so SP stays aligned (c_s3 reports the alignment
; of its own local)
define i32 @dyn_byval(i32 %n) {
  %p = alloca i32, i32 %n
  call void @touch(ptr %p, i32 %n)
  %s = alloca %S3, align 4
  store i8 1, ptr %s
  %s1 = getelementptr i8, ptr %s, i32 1
  store i8 2, ptr %s1
  %s2 = getelementptr i8, ptr %s, i32 2
  store i8 3, ptr %s2
  %r = call i32 @c_s3(ptr byval(%S3) align 1 %s)
  %v = call i32 @elem(ptr %p, i32 0)
  %t = add i32 %r, %v
  ret i32 %t
}

; an over-aligned dynamic alloca (16 bytes)
define i32 @dyn_align(i32 %n) {
  %p = alloca i32, i32 %n, align 16
  %a = ptrtoint ptr %p to i32
  %m = and i32 %a, 15
  call void @touch(ptr %p, i32 %n)
  %v = call i32 @elem(ptr %p, i32 1)
  %r = add i32 %m, %v
  ret i32 %r
}

; --- stack realignment: a local aligned beyond the 4-byte stack alignment.
; R6 is SP after the push, the prologue rounds SP down (and #-align, r0),
; the locals are off SP and the incoming arguments off R6. c_mis reports
; the misalignment of the address, which LLVM itself folds to zero, and
; fp_main.c calls these at each SP mod 64 it can reach.
define i32 @al16(i32 %n) {
  %p = alloca [4 x i32], align 16
  %q = alloca i32
  store i32 %n, ptr %q
  call void @touch(ptr %p, i32 4)
  %m = call i32 @c_mis(ptr %p, i32 16)
  %v = call i32 @elem(ptr %p, i32 3)
  %w = load volatile i32, ptr %q
  %r = mul i32 %m, 1000
  %r1 = add i32 %r, %v
  %r2 = add i32 %r1, %w
  ret i32 %r2
}

; 64 bytes, with the incoming stack arguments e and f
define i32 @al64_args(i32 %a, i32 %b, i32 %c, i32 %d, i32 %e, i32 %f) {
  %p = alloca [4 x i32], align 64
  call void @touch(ptr %p, i32 4)
  %m = call i32 @c_mis(ptr %p, i32 64)
  %v = call i32 @elem(ptr %p, i32 1)
  %r = mul i32 %m, 1000
  %e10 = mul i32 %e, 10
  %f100 = mul i32 %f, 100
  %r1 = add i32 %r, %v
  %r2 = add i32 %r1, %e10
  %r3 = add i32 %r2, %f100
  %r4 = add i32 %r3, %a
  ret i32 %r4
}

; a frame over rtsd's 1020 bytes, with frame-pointer=all, and a call with
; stack arguments
define i32 @al64_big(i32 %n) #0 {
  %p = alloca [300 x i32], align 64
  call void @touch(ptr %p, i32 300)
  %m = call i32 @c_mis(ptr %p, i32 64)
  %s = call i32 @c_sum6(i32 1, i32 2, i32 3, i32 4, i32 5, i32 %n)
  %v = call i32 @elem(ptr %p, i32 %n)
  %r = mul i32 %m, 1000
  %r1 = add i32 %r, %v
  %r2 = add i32 %r1, %s
  ret i32 %r2
}

; realignment and a dynamic alloca: SP moves, so R7 (the base pointer)
; keeps the aligned frame
define i32 @al16_dyn(i32 %n) {
  %p = alloca [4 x i32], align 16
  %d = alloca i32, i32 %n
  call void @touch(ptr %p, i32 4)
  call void @touch(ptr %d, i32 %n)
  %s = call i32 @c_sum6(i32 1, i32 2, i32 3, i32 4, i32 5, i32 %n)
  %m = call i32 @c_mis(ptr %p, i32 16)
  %i = sub i32 %n, 1
  %v = call i32 @elem(ptr %d, i32 %i)
  %w = call i32 @elem(ptr %p, i32 2)
  %r = mul i32 %m, 1000
  %r1 = add i32 %r, %v
  %r2 = add i32 %r1, %w
  %r3 = add i32 %r2, %s
  ret i32 %r3
}

attributes #0 = { "frame-pointer"="all" }
