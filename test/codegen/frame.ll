; Prologues and epilogues: the callee-saved registers are saved with one
; pushm (push.l for one register), and rtsd frees the frame, restores them
; and returns. Linked with frame_main.c, which keeps values live in R6..R13
; across the calls; scripts/check-frame.sh also checks the instructions
; against frame.expected.
target triple = "rx-none-elf"

declare void @touch(ptr, i32)

; --- which registers: r6 and r8 used, r7 saved with them (pushm r6-r8)
define i32 @clob_gap(i32 %a) {
  call void asm sideeffect "mov.l #0x5a5a, r6\0A\09mov.l #0xa5a5, r8", "~{r6},~{r8}"()
  %r = add i32 %a, 1
  ret i32 %r
}

; one register: push.l / rtsd #4, r10-r10
define i32 @clob_one(i32 %a) {
  call void asm sideeffect "mov.l #0x1234, r10", "~{r10}"()
  %r = add i32 %a, 2
  ret i32 %r
}

; every callee-saved register
define i32 @clob_all(i32 %a) {
  call void asm sideeffect "mov.l #1, r6\0A\09mov.l #2, r7\0A\09mov.l #3, r8\0A\09mov.l #4, r9\0A\09mov.l #5, r10\0A\09mov.l #6, r11\0A\09mov.l #7, r12\0A\09mov.l #8, r13", "~{r6},~{r7},~{r8},~{r9},~{r10},~{r11},~{r12},~{r13}"()
  %r = add i32 %a, 3
  ret i32 %r
}

; no callee-saved registers, locals only: rtsd #imm
define i32 @locals_only() {
  %p = alloca [16 x i32]
  call void @touch(ptr %p, i32 16)
  %q = getelementptr i32, ptr %p, i32 5
  %v = load i32, ptr %q
  ret i32 %v
}

; --- frame sizes around rtsd's limit of 1020 bytes (locals + registers;
; %n is kept in r8, so r6-r8 are saved)
; 1008 + 12 = 1020: rtsd #1020, r6-r8
define i32 @frame_1020(i32 %n) {
  %p = alloca [252 x i32]
  call void asm sideeffect "mov.l #0, r6\0A\09mov.l #0, r7", "~{r6},~{r7}"()
  call void @touch(ptr %p, i32 252)
  %q = getelementptr i32, ptr %p, i32 %n
  %v = load i32, ptr %q
  ret i32 %v
}

; 1012 + 12 = 1024: add #1012, r0 then rtsd #12, r6-r8
define i32 @frame_1024(i32 %n) {
  %p = alloca [253 x i32]
  call void asm sideeffect "mov.l #0, r6\0A\09mov.l #0, r7", "~{r6},~{r7}"()
  call void @touch(ptr %p, i32 253)
  %q = getelementptr i32, ptr %p, i32 %n
  %v = load i32, ptr %q
  ret i32 %v
}

; 1024 bytes and no registers: add #1024, r0 then rts
define i32 @frame_big() {
  %p = alloca [256 x i32]
  call void @touch(ptr %p, i32 256)
  %q = getelementptr i32, ptr %p, i32 255
  %v = load i32, ptr %q
  ret i32 %v
}

; --- incoming arguments on the stack, above the saved range (r6-r9, with
; r7 / r8 filled in) and the locals
define i32 @stack_args(i32 %a, i32 %b, i32 %c, i32 %d, i32 %e, i32 %f) {
  %p = alloca [8 x i32]
  call void asm sideeffect "mov.l #0, r6\0A\09mov.l #0, r9", "~{r6},~{r9}"()
  call void @touch(ptr %p, i32 8)
  %q = getelementptr i32, ptr %p, i32 3
  %v = load i32, ptr %q
  %s1 = mul i32 %e, 100
  %s2 = mul i32 %f, 10000
  %s3 = add i32 %s1, %s2
  %s4 = add i32 %s3, %a
  %s5 = add i32 %s4, %d
  %s6 = add i32 %s5, %v
  ret i32 %s6
}

; --- an i64 result in R1:R2 through rtsd (r11 and the registers %a and %n
; are kept in, with the ones between filled in)
define i64 @ret64(i64 %a, i32 %n) {
  %p = alloca [4 x i32]
  call void asm sideeffect "mov.l #0, r11", "~{r11}"()
  call void @touch(ptr %p, i32 4)
  %q = getelementptr i32, ptr %p, i32 %n
  %v = load i32, ptr %q
  %w = zext i32 %v to i64
  %r = add i64 %a, %w
  ret i64 %r
}
