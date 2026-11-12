; Calls after_stack (after_stack.ll, lib.rs).
%s6 = type { [3 x i16] }
declare i32 @after_stack(ptr byval(%s6) align 2, i32)
define i32 @caller(ptr %s) {
  %r = call i32 @after_stack(ptr byval(%s6) align 2 %s, i32 7)
  ret i32 %r
}
