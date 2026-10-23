target triple = "rx-none-elf"

define i32 @add(i32 %a, i32 %b) {
  %r = add i32 %a, %b
  ret i32 %r
}

define i32 @addi(i32 %a) {
  %r = add i32 %a, 1000
  ret i32 %r
}

define i32 @max(i32 %a, i32 %b) {
  %c = icmp sgt i32 %a, %b
  %r = select i1 %c, i32 %a, i32 %b
  ret i32 %r
}

define i32 @sum(i32 %n) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %i1, %loop ]
  %s = phi i32 [ 0, %entry ], [ %s1, %loop ]
  %s1 = add i32 %s, %i
  %i1 = add i32 %i, 1
  %c = icmp slt i32 %i1, %n
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %s1
}

declare i32 @ext(i32, i32, i32, i32, i32, i32)

define i32 @call6(i32 %x) {
  %r = call i32 @ext(i32 1, i32 2, i32 3, i32 4, i32 %x, i32 6)
  %s = add i32 %r, %x
  ret i32 %s
}
