; A structure that is always on the stack and an int after it: the int is in
; R1 for CC-RX's calling convention, in R3 for GNURX's (scripts/check-abi-note.sh).
%s6 = type { [3 x i16] }
define i32 @after_stack(ptr byval(%s6) align 2 %s, i32 %x) {
  ret i32 %x
}
