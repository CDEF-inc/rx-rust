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

echo "== assembler and DFPU instruction sequences"
for t in basic scc system fpu dsp v2 dfpu v3 v3e mov; do
  run "mc $t" bash "$S/mc-diff.sh" lines "$root/test/mc/$t.s"
done
for t in branch symbol string; do
  run "mc $t" bash "$S/mc-diff.sh" file "$root/test/mc/$t.s"
done
run "check-dfpu" bash "$S/check-dfpu.sh"
run "check-mem" bash "$S/check-mem.sh"

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

C=$root/test/codegen
for cpu in "${cpus[@]}"; do
  echo "== $cpu"
  export RX_CPU=$cpu
  unset QEMU_CPU
  for o in 0 2; do
    run "$cpu codegen i32 -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/i32.ll" "$C/main.c"
    run "$cpu codegen m3 -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/m3.ll" "$C/m3_main.c"
    run "$cpu codegen struct -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/struct.ll" "$C/struct_main.c"
    run "$cpu codegen atomic -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/atomic.ll" "$C/atomic_main.c"
    run "$cpu codegen mem -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/mem.ll" "$C/mem_main.c"
  done
  attrs=; [ "$cpu" = rx62n ] && attrs=+fpu
  for o in 0 1 2 3; do
    run "$cpu codegen fpu -O$o" env OPT=$o ATTRS=$attrs bash "$S/run-codegen-test.sh" "$C/fpu.ll" "$C/fpu_main.c"
    if [ "$cpu" = rxv3 ] || [ "$cpu" = rxv3e ]; then
      run "$cpu codegen dfpu -O$o" env OPT=$o bash "$S/run-codegen-test.sh" "$C/dfpu.ll" "$C/dfpu_main.c"
    fi
  done
  for p in release dev; do
    run "$cpu rust core-smoke $p" env PROFILE=$p bash "$S/run-rust-test.sh"
    run "$cpu rust qemu-demo $p" env PROFILE=$p bash "$S/run-rust-rom.sh"
    run "$cpu rust isr-save $p" env PROFILE=$p bash "$S/run-rust-rom.sh" "$root/test/rust/isr-save"
  done
done

echo "== $pass passed, ${#fail[@]} failed"
for f in "${fail[@]}"; do echo "   $f"; done
[ ${#fail[@]} -eq 0 ]
