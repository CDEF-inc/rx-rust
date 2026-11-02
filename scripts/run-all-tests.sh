#!/bin/bash
# Runs every test: the CPU-independent ones once (assembler, DFPU
# instruction sequences, QEMU instruction tests on their CPU models), then
# the codegen and Rust tests once per CPU model (RX_CPU, see cpu.sh) on
# CDEF-inc/qemu.
#
#   run-all-tests.sh [CPU...]     CPU defaults to rx62n rxv2 rxv3 rxv3e
#
# Prints one line per test and a summary; the exit status is 0 iff all
# passed. Logs go to build/test-logs.
set -u
cpus=("$@"); [ ${#cpus[@]} -gt 0 ] || cpus=(rx62n rxv2 rxv3 rxv3e)
root=$(cd "$(dirname "$0")/.." && pwd)
S=$root/scripts
logs=$root/build/test-logs
rm -rf "$logs"
mkdir -p "$logs"
pass=0
fail=()

# run NAME COMMAND...: run a test, keep its output in build/test-logs
run() {
  local name=$1; shift
  local log=$logs/$(echo "$name" | tr ' /=' '___').log
  if "$@" > "$log" 2>&1; then
    pass=$((pass + 1))
    printf 'ok    %s\n' "$name"
  else
    fail+=("$name")
    printf 'FAIL  %s  (%s)\n' "$name" "$log"
  fi
}

echo "== assembler, disassembler and DFPU instruction sequences"
for t in basic scc system fpu dsp v2 dfpu v3 v3e mov memex; do
  run "mc $t" bash "$S/mc-diff.sh" lines "$root/test/mc/$t.s"
done
for t in branch symbol string sizeless; do
  run "mc $t" bash "$S/mc-diff.sh" file "$root/test/mc/$t.s"
done
for t in basic scc system fpu dsp v2 dfpu v3 v3e mov memex branch symbol \
         string sizeless brdisp; do
  run "dis $t" bash "$S/mc-dis.sh" "$root/test/mc/$t.s"
done
# The RXv1 DSP forms (A0, no shift), decoded first only for RXv1.
run "dis dsp rxv1" bash "$S/mc-dis.sh" -m rxv1 "$root/test/mc/dsp.s"
run "check-fpu" bash "$S/check-fpu.sh"
run "check-dfpu" bash "$S/check-dfpu.sh"
run "check-jt" bash "$S/check-jt.sh"
run "check-mem" bash "$S/check-mem.sh"
run "check-mulh" bash "$S/check-mulh.sh"
run "check-frame" bash "$S/check-frame.sh"
run "check-fp" bash "$S/check-fp.sh"
run "check-lld" bash "$S/check-lld.sh"
run "lld lit" "$root/build/llvm/bin/llvm-lit" -q \
  "$root"/rust/src/llvm-project/lld/test/ELF/rx-*

echo "== QEMU instruction tests"
run "qemu cpu-features" "$S/run-qemu-test.sh" cpu-features rx62n rxv1 rxv2 rxv3 rxv3e
run "qemu fcmp-flags" "$S/run-qemu-test.sh" fcmp-flags rx62n rxv2 rxv3 rxv3e
run "qemu fpu-status" "$S/run-qemu-test.sh" fpu-status rx62n rxv2 rxv3 rxv3e
run "qemu rxv2-fpu" "$S/run-qemu-test.sh" rxv2-fpu rxv2 rxv3 rxv3e
run "qemu rxv2-insns" "$S/run-qemu-test.sh" rxv2-insns rxv2 rxv3 rxv3e
run "qemu dsp-v1" "$S/run-qemu-test.sh" dsp-v1 rx62n rxv1
run "qemu dsp-v2" "$S/run-qemu-test.sh" dsp-v2 rxv2 rxv3 rxv3e
run "qemu rxv3-insns" "$S/run-qemu-test.sh" rxv3-insns rxv3 rxv3e
run "qemu dfpu-insns" "$S/run-qemu-test.sh" dfpu-insns rxv3 rxv3e
run "qemu rxv3e-insns" "$S/run-qemu-test.sh" rxv3e-insns rxv3e
run "qemu mem-insns" "$S/run-qemu-test.sh" mem-insns rx62n rxv1 rxv2 rxv3 rxv3e
run "qemu cmt" "$S/run-qemu-test.sh" cmt rx62n rxv1 rxv2 rxv3 rxv3e
run "qemu memex-insns" "$S/run-qemu-test.sh" memex-insns rx62n rxv1 rxv2 rxv3 rxv3e

C=$root/test/codegen
for cpu in "${cpus[@]}"; do
  echo "== $cpu"
  export RX_CPU=$cpu
  unset QEMU_CPU
  for o in 0 2; do
    run "$cpu codegen i32 -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/i32.ll" "$C/main.c"
    run "$cpu codegen m3 -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/m3.ll" "$C/m3_main.c"
    run "$cpu codegen struct -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/struct.ll" "$C/struct_main.c"
    run "$cpu codegen demote -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/demote.ll" "$C/demote_main.c"
    run "$cpu codegen atomic -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/atomic.ll" "$C/atomic_main.c"
    run "$cpu codegen mem -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/mem.ll" "$C/mem_main.c"
    run "$cpu codegen mulh -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/mulh.ll" "$C/mulh_main.c"
    run "$cpu codegen frame -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/frame.ll" "$C/frame_main.c"
    run "$cpu codegen fp -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/fp.ll" "$C/fp_main.c"
    run "$cpu codegen va -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/va.ll" "$C/va_main.c"
    run "$cpu codegen jt -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/jt.ll" "$C/jt_main.c"
  done
  attrs=; [ "$cpu" = rx62n ] && attrs=+fpu
  for o in 0 1 2 3; do
    run "$cpu codegen fpu -O$o" env OPT=$o ATTRS=$attrs bash "$S/run-codegen-test.sh" "$C/fpu.ll" "$C/fpu_main.c"
    if [ "$cpu" = rxv3 ] || [ "$cpu" = rxv3e ]; then
      run "$cpu codegen dfpu -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/dfpu.ll" "$C/dfpu_main.c"
    fi
  done
  # The same float / double code with the FPU / DFPU (flush-denormals) and
  # in soft-float: both must give the IEEE results.
  for o in 0 2; do
    run "$cpu codegen fpu_ieee -O$o" env OPT=$o ATTRS=${attrs:+$attrs,}+flush-denormals \
      bash "$S/run-codegen-test.sh" "$C/fpu_ieee.ll" "$C/fpu_ieee_main.c"
  done
  run "$cpu codegen fpu_ieee soft" env OPT=2 bash "$S/run-codegen-test.sh" "$C/fpu_ieee.ll" "$C/fpu_ieee_main.c"
  for p in release dev; do
    run "$cpu rust core-smoke $p" env PROFILE=$p bash "$S/run-rust-test.sh"
    run "$cpu rust qemu-demo $p" env PROFILE=$p bash "$S/run-rust-rom.sh"
    run "$cpu rust isr-save $p" env PROFILE=$p bash "$S/run-rust-rom.sh" "$root/test/rust/isr-save"
    run "$cpu rust alloc-smoke $p" env PROFILE=$p bash "$S/run-rust-rom.sh" "$root/test/rust/alloc-smoke"
    run "$cpu rust float-smoke $p" env PROFILE=$p bash "$S/run-rust-rom.sh" "$root/test/rust/float-smoke"
    run "$cpu rust float-smoke $p flush-denormals" env PROFILE=$p \
      RUST_EXTRA_FLAGS="-C target-feature=+flush-denormals" \
      bash "$S/run-rust-rom.sh" "$root/test/rust/float-smoke"
    run "$cpu rust isr-cfi $p" "$S/check-isr-cfi.py" \
      "$root/build/rust-rom/$cpu/rx-none-elf/$([ $p = dev ] && echo debug || echo $p)/isr-save"
    run "$cpu rust hw-irq $p" env PROFILE=$p RX_MEMORY_X="$root/test/rust/qemu-demo/memory.x" \
      bash "$S/run-rust-rom.sh" "$root/test/rust/hw-irq"
  done
done

# The codegen tests and the Rust applications linked with ld.lld (the RX
# port in build/llvm) instead of GNU ld.
LLD=$root/build/llvm/bin/ld.lld
for cpu in rx62n rxv3; do
  echo "== $cpu, linked with ld.lld"
  export RX_CPU=$cpu
  unset QEMU_CPU
  for t in "i32 main" "m3 m3_main" "struct struct_main" "demote demote_main" \
           "atomic atomic_main" "mem mem_main" "mulh mulh_main" "frame frame_main" \
           "fp fp_main" "va va_main" "jt jt_main"; do
    set -- $t
    run "$cpu lld codegen $1" env OPT=2 RX_LD=$LLD bash "$S/run-codegen-test.sh" "$C/$1.ll" "$C/$2.c"
  done
  attrs=; [ "$cpu" = rx62n ] && attrs=+fpu
  run "$cpu lld codegen fpu" env OPT=2 ATTRS=$attrs RX_LD=$LLD \
    bash "$S/run-codegen-test.sh" "$C/fpu.ll" "$C/fpu_main.c"
  run "$cpu lld codegen fpu_ieee" env OPT=2 ATTRS=${attrs:+$attrs,}+flush-denormals RX_LD=$LLD \
    bash "$S/run-codegen-test.sh" "$C/fpu_ieee.ll" "$C/fpu_ieee_main.c"
  [ "$cpu" = rxv3 ] && run "$cpu lld codegen dfpu" env OPT=2 RX_LD=$LLD \
    bash "$S/run-codegen-test.sh" "$C/dfpu.ll" "$C/dfpu_main.c"
  for c in qemu-demo isr-save alloc-smoke float-smoke; do
    run "$cpu lld rust $c" env PROFILE=release RUST_EXTRA_FLAGS="-C linker=$LLD" \
      bash "$S/run-rust-rom.sh" "$root/test/rust/$c"
  done
  run "$cpu lld rust hw-irq" env PROFILE=release RUST_EXTRA_FLAGS="-C linker=$LLD" \
    RX_MEMORY_X="$root/test/rust/qemu-demo/memory.x" bash "$S/run-rust-rom.sh" "$root/test/rust/hw-irq"
done

echo "== $pass passed, ${#fail[@]} failed"
for f in "${fail[@]}"; do echo "   $f"; done
[ ${#fail[@]} -eq 0 ]
