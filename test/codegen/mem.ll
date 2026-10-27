; Memory access forms in code generation: stores of constants (mov.sz #imm,
; dsp[Rd]). Linked with mem_main.c; scripts/check-mem.sh also checks the
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
