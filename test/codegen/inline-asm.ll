; Inline assembly: operands, control registers, clobbers and module asm.
target triple = "rx-none-elf"

module asm "\09.global\09_boot"
module asm "_boot:"
module asm "\09mov.l\09#0x1000, r0"
module asm "\09mvtc\09#_vectors, intb"
module asm "\09rts"

@vectors = global [4 x i32] zeroinitializer

define i32 @f(i32 %x) {
  %r = call i32 asm sideeffect "add #1, $1, $0", "=r,r"(i32 %x)
  ret i32 %r
}

define void @g() {
  call void asm sideeffect "setpsw i\0A\09mvtipl #3", ""()
  ret void
}

define i32 @h() {
  %v = call i32 asm sideeffect "mvfc psw, $0", "=r"()
  ret i32 %v
}

define void @k(i32 %x) {
  call void asm sideeffect "mov.l $0, r1", "r,~{r1}"(i32 %x)
  ret void
}

define i32 @tied(i32 %x, i32 %y) {
  %r = call i32 asm "add $2, $0", "=r,0,r"(i32 %x, i32 %y)
  ret i32 %r
}
