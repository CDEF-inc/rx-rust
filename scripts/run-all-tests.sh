#!/bin/bash
# Runs every test: the CPU-independent ones once (assembler, DFPU
# instruction sequences, QEMU instruction tests on their CPU models), then
# the codegen and Rust tests once per CPU model (RX_CPU, see cpu.sh) on
# CDEF-inc/qemu.
#
#   run-all-tests.sh [-j N] [--shard K/N] [CPU...]
#                                 CPU defaults to rx62n rxv2 rxv3 rxv3e
#
# The tests run in parallel, N at a time (-j, default: the number of CPUs;
# -j 1 runs them one after another). Tests that share a cargo target
# directory and profile, or that check what another one built, form a group
# and run in order. --shard K/N runs the K-th of N parts (K = 1..N; the
# groups are dealt out in turn), for CI jobs that split the tests.
#
# Prints one line per test as it finishes and a summary; the exit status is
# 0 iff all passed. Logs go to build/test-logs.
set -u
jobs=$(nproc)
shard=1/1
while :; do
  case ${1:-} in
  -j) jobs=$2; shift 2 ;;
  --shard) shard=$2; shift 2 ;;
  *) break ;;
  esac
done
shard_k=${shard%/*} shard_n=${shard#*/}
[ "$shard_k" -ge 1 ] && [ "$shard_k" -le "$shard_n" ] && [ "$jobs" -ge 1 ] || {
  echo "usage: $0 [-j N] [--shard K/N] [CPU...]" >&2; exit 2; }
cpus=("$@"); [ ${#cpus[@]} -gt 0 ] || cpus=(rx62n rxv2 rxv3 rxv3e)
root=$(cd "$(dirname "$0")/.." && pwd)
S=$root/scripts
logs=$root/build/test-logs
# QEMU's timers (CMT) follow the host's clock, so on a busy machine a test
# can see more time pass between two instructions than the board would.
# With -icount the guest's time follows the instructions it ran (8 ns each,
# and no waiting when it is idle), whatever else runs.
export QEMU_FLAGS=${QEMU_FLAGS-"-icount shift=3,sleep=off"}
rm -rf "$logs"
mkdir -p "$logs/status"

# The queue: test i is names[i], run as cmds[i] (a shell word list, with
# RX_CPU and QEMU_CPU as they were when it was queued); groups[g] holds the
# tests of group g, and order the groups as they first appeared.
names=() cmds=() order=()
declare -A groups=()

# run [-g GROUP] NAME COMMAND...: queue a test (its own group unless -g);
# its output goes to build/test-logs.
run() {
  local g=
  [ "$1" = -g ] && { g=$2; shift 2; }
  local name=$1; shift
  g=${g:-$name}
  local i=${#names[@]} pre="env -u QEMU_CPU"
  [ -n "${QEMU_CPU+x}" ] && pre="env QEMU_CPU=$(printf %q "$QEMU_CPU")"
  names+=("$name")
  cmds+=("$pre RX_CPU=$(printf %q "${RX_CPU:-}") $(printf '%q ' "$@")")
  [ -n "${groups[$g]+x}" ] || order+=("$g")
  groups[$g]+=" $i"
}

# run_group G: run the tests of group G in order, each within
# $TEST_TIMEOUT seconds (default 900; the longest take a few minutes), so
# that a test that hangs fails with its log instead of holding the run.
run_group() {
  local i log
  for i in ${groups[$1]}; do
    log=$logs/$(echo "${names[$i]}" | tr ' /=' '___').log
    if timeout -k 10 "${TEST_TIMEOUT:-900}" bash -c "${cmds[$i]}" > "$log" 2>&1 < /dev/null; then
      echo ok > "$logs/status/$i"
      printf 'ok    %s\n' "${names[$i]}"
    else
      echo FAIL > "$logs/status/$i"
      printf 'FAIL  %s  (%s)\n' "${names[$i]}" "$log"
    fi
  done
}

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
# LLDB (build/lldb, from scripts/build-lldb.sh; skipped without it): a debug
# session on QEMU, and the RX lit test.
if [ -x "$root/build/lldb/bin/lldb" ]; then
  run "check-lldb" bash "$S/check-lldb.sh"
  run "lldb lit" "$root/build/lldb/bin/llvm-lit" -q \
    "$root"/rust/src/llvm-project/lldb/test/Shell/Disassemble/TestDisassembleRX.test \
    "$root"/rust/src/llvm-project/clang/test/Preprocessor/init-rx.c \
    "$root"/rust/src/llvm-project/clang/test/CodeGen/rx-mangle.c
else
  echo "(no build/lldb: LLDB not tested)"
fi

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
    # build/rust-rom/$cpu; isr-cfi checks what isr-save built
    g="rom $cpu $p"
    run -g "$g" "$cpu rust qemu-demo $p" env PROFILE=$p bash "$S/run-rust-rom.sh"
    run -g "$g" "$cpu rust isr-save $p" env PROFILE=$p bash "$S/run-rust-rom.sh" "$root/test/rust/isr-save"
    run -g "$g" "$cpu rust alloc-smoke $p" env PROFILE=$p bash "$S/run-rust-rom.sh" "$root/test/rust/alloc-smoke"
    run -g "$g" "$cpu rust float-smoke $p" env PROFILE=$p bash "$S/run-rust-rom.sh" "$root/test/rust/float-smoke"
    run "$cpu rust float-smoke $p flush-denormals" env PROFILE=$p \
      RUST_EXTRA_FLAGS="-C target-feature=+flush-denormals" \
      bash "$S/run-rust-rom.sh" "$root/test/rust/float-smoke"
    run -g "$g" "$cpu rust isr-cfi $p" "$S/check-isr-cfi.py" \
      "$root/build/rust-rom/$cpu/rx-none-elf/$([ $p = dev ] && echo debug || echo $p)/isr-save"
    # register bank interrupt entries (save / rstr, RXv3)
    if [ "$cpu" = rxv3 ] || [ "$cpu" = rxv3e ]; then
      run -g "$g" "$cpu rust isr-bank $p" env PROFILE=$p bash "$S/run-rust-rom.sh" "$root/test/rust/isr-bank"
      run -g "$g" "$cpu rust isr-bank-cfi $p" "$S/check-isr-cfi.py" \
        "$root/build/rust-rom/$cpu/rx-none-elf/$([ $p = dev ] && echo debug || echo $p)/isr-bank"
    fi
    run -g "$g" "$cpu rust hw-irq $p" env PROFILE=$p RX_MEMORY_X="$root/test/rust/qemu-demo/memory.x" \
      bash "$S/run-rust-rom.sh" "$root/test/rust/hw-irq"
  done
done

# The codegen tests and the Rust applications linked with GNU ld instead of
# lld (the default).
GLD=$root/gnu/install/bin/rx-elf-ld
for cpu in rx62n rxv3; do
  export RX_CPU=$cpu
  unset QEMU_CPU
  for t in "i32 main" "m3 m3_main" "struct struct_main" "demote demote_main" \
           "atomic atomic_main" "mem mem_main" "mulh mulh_main" "frame frame_main" \
           "fp fp_main" "va va_main" "jt jt_main"; do
    set -- $t
    run "$cpu gnu-ld codegen $1" env OPT=2 RX_LD=$GLD bash "$S/run-codegen-test.sh" "$C/$1.ll" "$C/$2.c"
  done
  attrs=; [ "$cpu" = rx62n ] && attrs=+fpu
  run "$cpu gnu-ld codegen fpu" env OPT=2 ATTRS=$attrs RX_LD=$GLD \
    bash "$S/run-codegen-test.sh" "$C/fpu.ll" "$C/fpu_main.c"
  run "$cpu gnu-ld codegen fpu_ieee" env OPT=2 ATTRS=${attrs:+$attrs,}+flush-denormals RX_LD=$GLD \
    bash "$S/run-codegen-test.sh" "$C/fpu_ieee.ll" "$C/fpu_ieee_main.c"
  [ "$cpu" = rxv3 ] && run "$cpu gnu-ld codegen dfpu" env OPT=2 RX_LD=$GLD \
    bash "$S/run-codegen-test.sh" "$C/dfpu.ll" "$C/dfpu_main.c"
  for c in qemu-demo isr-save alloc-smoke float-smoke; do
    run -g "rom $cpu gnu-ld" "$cpu gnu-ld rust $c" env PROFILE=release RUST_EXTRA_FLAGS="-C linker=$GLD" \
      bash "$S/run-rust-rom.sh" "$root/test/rust/$c"
  done
  run -g "rom $cpu gnu-ld" "$cpu gnu-ld rust hw-irq" env PROFILE=release RUST_EXTRA_FLAGS="-C linker=$GLD" \
    RX_MEMORY_X="$root/test/rust/qemu-demo/memory.x" bash "$S/run-rust-rom.sh" "$root/test/rust/hw-irq"
done

# With linker relaxation (-mattr=+relax / -C target-feature=+relax: lld
# shortens calls and symbol addresses): the codegen tests, the Rust
# applications (isr-save's CFI checked again) and a debug session.
RELAX="-C target-feature=+relax"
for cpu in rx62n rxv3; do
  export RX_CPU=$cpu
  unset QEMU_CPU
  for t in "i32 main" "m3 m3_main" "struct struct_main" "demote demote_main" \
           "atomic atomic_main" "mem mem_main" "mulh mulh_main" "frame frame_main" \
           "fp fp_main" "va va_main" "jt jt_main"; do
    set -- $t
    run "$cpu relax codegen $1" env OPT=2 ATTRS=+relax bash "$S/run-codegen-test.sh" "$C/$1.ll" "$C/$2.c"
  done
  attrs=; [ "$cpu" = rx62n ] && attrs=+fpu
  run "$cpu relax codegen fpu" env OPT=2 ATTRS=${attrs:+$attrs,}+relax \
    bash "$S/run-codegen-test.sh" "$C/fpu.ll" "$C/fpu_main.c"
  run "$cpu relax codegen fpu_ieee" env OPT=2 ATTRS=${attrs:+$attrs,}+flush-denormals,+relax \
    bash "$S/run-codegen-test.sh" "$C/fpu_ieee.ll" "$C/fpu_ieee_main.c"
  [ "$cpu" = rxv3 ] && run "$cpu relax codegen dfpu" env OPT=2 ATTRS=+relax \
    bash "$S/run-codegen-test.sh" "$C/dfpu.ll" "$C/dfpu_main.c"
  g="rom $cpu relax"
  for c in qemu-demo isr-save alloc-smoke float-smoke; do
    run -g "$g" "$cpu relax rust $c" env PROFILE=release RUST_EXTRA_FLAGS="$RELAX" \
      bash "$S/run-rust-rom.sh" "$root/test/rust/$c"
  done
  run -g "$g" "$cpu relax rust isr-cfi" "$S/check-isr-cfi.py" \
    "$root/build/rust-rom/$cpu-$(echo "$RELAX" | md5sum | cut -c1-8)/rx-none-elf/release/isr-save"
  if [ "$cpu" = rxv3 ]; then
    run -g "$g" "$cpu relax rust isr-bank" env PROFILE=release RUST_EXTRA_FLAGS="$RELAX" \
      bash "$S/run-rust-rom.sh" "$root/test/rust/isr-bank"
    run -g "$g" "$cpu relax rust isr-bank-cfi" "$S/check-isr-cfi.py" \
      "$root/build/rust-rom/$cpu-$(echo "$RELAX" | md5sum | cut -c1-8)/rx-none-elf/release/isr-bank"
  fi
  run -g "$g" "$cpu relax rust hw-irq" env PROFILE=release RUST_EXTRA_FLAGS="$RELAX" \
    RX_MEMORY_X="$root/test/rust/qemu-demo/memory.x" bash "$S/run-rust-rom.sh" "$root/test/rust/hw-irq"
done
unset RX_CPU
[ -x "$root/build/lldb/bin/lldb" ] &&
  run "check-lldb relax" env RUST_EXTRA_FLAGS="$RELAX" bash "$S/check-lldb.sh"

# Run this shard's groups, at most $jobs at a time.
n=0
for g in "${order[@]}"; do
  if [ $((n++ % shard_n + 1)) = "$shard_k" ]; then
    while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do wait -n; done
    run_group "$g" &
  fi
done
wait

pass=0 fail=()
for g in "${order[@]}"; do
  for i in ${groups[$g]}; do
    [ -f "$logs/status/$i" ] || continue
    if [ "$(cat "$logs/status/$i")" = ok ]; then
      pass=$((pass + 1))
    else
      fail+=("${names[$i]}")
    fi
  done
done
echo "== $pass passed, ${#fail[@]} failed"
for f in "${fail[@]}"; do echo "   $f"; done
[ ${#fail[@]} -eq 0 ]
