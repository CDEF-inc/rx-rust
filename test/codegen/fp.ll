; Frame pointer (R6, as in GCC) and dynamic alloca. Linked with fp_main.c,
; which checks the results and that R6-R13 survive the calls;
; scripts/check-fp.sh also checks the instructions and the CFI.
target triple = "rx-none-elf"

declare void @touch(ptr, i32)
declare i32 @c_sum6(i32, i32, i32, i32, i32, i32)
declare ptr @llvm.stacksave.p0()
declare void @llvm.stackrestore.p0(ptr)

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

attributes #0 = { "frame-pointer"="all" }
