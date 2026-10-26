; Compare-and-swap and read-modify-write atomics: compiled with llc, linked
; with atomic_main.c and run under QEMU by scripts/run-codegen-test.sh. QEMU
; is RXv1, so this exercises the interrupt-masking expansion of CMPXCHG32;
; the RXv2 MOVLI/MOVCO loop is checked by assembling `llc -mcpu=rxv2` output.
target triple = "rx-none-elf"

; --- 32-bit compare-and-swap: old value and success flag
define i32 @cas32(ptr %p, i32 %cmp, i32 %new) {
  %r = cmpxchg ptr %p, i32 %cmp, i32 %new seq_cst seq_cst
  %v = extractvalue { i32, i1 } %r, 0
  ret i32 %v
}
define i32 @cas32_ok(ptr %p, i32 %cmp, i32 %new) {
  %r = cmpxchg weak ptr %p, i32 %cmp, i32 %new acq_rel monotonic
  %ok = extractvalue { i32, i1 } %r, 1
  %z = zext i1 %ok to i32
  ret i32 %z
}

; --- 8- and 16-bit compare-and-swap (expanded to a 32-bit one on the
; containing word)
define i32 @cas16(ptr %p, i16 %cmp, i16 %new) {
  %r = cmpxchg ptr %p, i16 %cmp, i16 %new seq_cst seq_cst
  %v = extractvalue { i16, i1 } %r, 0
  %ok = extractvalue { i16, i1 } %r, 1
  %z = zext i16 %v to i32
  %s = select i1 %ok, i32 65536, i32 0
  %o = or i32 %z, %s
  ret i32 %o
}
define i32 @cas8(ptr %p, i8 %cmp, i8 %new) {
  %r = cmpxchg ptr %p, i8 %cmp, i8 %new seq_cst seq_cst
  %v = extractvalue { i8, i1 } %r, 0
  %ok = extractvalue { i8, i1 } %r, 1
  %z = zext i8 %v to i32
  %s = select i1 %ok, i32 256, i32 0
  %o = or i32 %z, %s
  ret i32 %o
}

; --- 32-bit read-modify-write: each returns the old value
define i32 @rmw_add(ptr %p, i32 %v) { %r = atomicrmw add ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_sub(ptr %p, i32 %v) { %r = atomicrmw sub ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_and(ptr %p, i32 %v) { %r = atomicrmw and ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_or(ptr %p, i32 %v) { %r = atomicrmw or ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_xor(ptr %p, i32 %v) { %r = atomicrmw xor ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_nand(ptr %p, i32 %v) { %r = atomicrmw nand ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_xchg(ptr %p, i32 %v) { %r = atomicrmw xchg ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_max(ptr %p, i32 %v) { %r = atomicrmw max ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_min(ptr %p, i32 %v) { %r = atomicrmw min ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_umax(ptr %p, i32 %v) { %r = atomicrmw umax ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_umin(ptr %p, i32 %v) { %r = atomicrmw umin ptr %p, i32 %v seq_cst
  ret i32 %r }
define i32 @rmw_uinc(ptr %p, i32 %v) {
  %r = atomicrmw uinc_wrap ptr %p, i32 %v seq_cst
  ret i32 %r }

; --- 8- and 16-bit read-modify-write
define i32 @rmw8_add(ptr %p, i8 %v) { %r = atomicrmw add ptr %p, i8 %v seq_cst
  %z = zext i8 %r to i32
  ret i32 %z }
define i32 @rmw8_min(ptr %p, i8 %v) { %r = atomicrmw min ptr %p, i8 %v seq_cst
  %s = sext i8 %r to i32
  ret i32 %s }
define i32 @rmw16_xchg(ptr %p, i16 %v) {
  %r = atomicrmw xchg ptr %p, i16 %v seq_cst
  %z = zext i16 %r to i32
  ret i32 %z }
define i32 @rmw16_umax(ptr %p, i16 %v) {
  %r = atomicrmw umax ptr %p, i16 %v seq_cst
  %z = zext i16 %r to i32
  ret i32 %z }

; --- a CAS loop with values live across it (register pressure)
define i32 @cas_loop(ptr %p, i32 %n, i32 %a, i32 %b, i32 %c) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %i1, %loop ]
  %acc = phi i32 [ 0, %entry ], [ %acc1, %loop ]
  %old = load atomic i32, ptr %p monotonic, align 4
  %new = add i32 %old, %a
  %r = cmpxchg ptr %p, i32 %old, i32 %new seq_cst seq_cst
  %v = extractvalue { i32, i1 } %r, 0
  %t = mul i32 %v, %b
  %u = xor i32 %t, %c
  %acc1 = add i32 %acc, %u
  %i1 = add i32 %i, 1
  %done = icmp eq i32 %i1, %n
  br i1 %done, label %exit, label %loop
exit:
  ret i32 %acc1
}
