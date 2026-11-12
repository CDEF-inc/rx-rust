#!/bin/bash
# Builds llvm-test-suite with clang for rx-none-elf and runs it on QEMU:
# newlib and libstdc++ from GNURX (the C and C++ libraries), the start-up
# code and system calls of test/test-suite, and scripts/test-suite-run.py as
# TEST_SUITE_RUN_UNDER (the program's SCI0 output is its stdout). The
# benchmarks use their small problem sizes (SMALL_PROBLEM_SIZE): QEMU and
# the soft-float double are slow.
#
#   run-test-suite.sh [SUBDIR...]     default: SingleSource
#
# Environment:
#   TEST_SUITE  the llvm-test-suite checkout (default ../ext/llvm-test-suite)
#   CLANG       clang (default build/lldb/bin/clang); clang++ next to it
#   GNURX       GNURX's rx-elf tree (the directory with bin/rx-elf-gcc and
#               lib/gcc/rx-elf), for --gcc-toolchain
#   RX_CPU      the QEMU core and clang's -mcpu (scripts/cpu.sh; default
#               rx62n, for which clang gets GCC's -mcpu=rx600)
#   OPT         the optimization flags (default -O2)
#   OUT         the build directory (default build/test-suite-$RX_CPU-O2)
#   JOBS        parallel jobs (default nproc)
#
# The results are in $OUT/results.json and $OUT/lit.log; a summary is printed.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/cpu.sh"
TEST_SUITE=${TEST_SUITE:-$root/../ext/llvm-test-suite}
CLANG=${CLANG:-$root/build/lldb/bin/clang}
GNURX=${GNURX:?set GNURX to the rx-elf tree of GNURX}
OPT=${OPT:--O2}
case ${MCPU:-rxv1} in
  rxv1) ccpu=rx600 ;;
  *) ccpu=$MCPU ;;
esac
OUT=${OUT:-$root/build/test-suite-${RX_CPU:-rx62n}$(echo "$OPT" | tr -d ' ')}
JOBS=${JOBS:-$(nproc)}
bin=$(dirname "$CLANG")
lit=${LIT:-$bin/llvm-lit}
T=$root/test/test-suite
subdirs=("$@"); [ ${#subdirs[@]} -gt 0 ] || subdirs=(SingleSource)

mkdir -p "$OUT/rt"
target="--target=rx-none-elf -mcpu=$ccpu --gcc-toolchain=$GNURX"
# Rebuilt when changed: every program depends on them (host-tools.cmake).
[ "$T/start.s" -nt "$OUT/rt/start.o" ] &&
  "$CLANG" $target -c "$T/start.s" -o "$OUT/rt/start.o"
[ "$T/syscalls.c" -nt "$OUT/rt/syscalls.o" ] &&
  "$CLANG" $target -O2 -Wall -c "$T/syscalls.c" -o "$OUT/rt/syscalls.o"

ldflags="--ld-path=$bin/ld.lld -nostartfiles -T $T/rx-qemu-libc.ld
  $OUT/rt/start.o $OUT/rt/syscalls.o"
cmake -G Ninja -S "$TEST_SUITE" -B "$OUT" \
  -DCMAKE_SYSTEM_NAME=Generic \
  -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY \
  -DCMAKE_C_COMPILER="$CLANG" -DCMAKE_CXX_COMPILER="${CLANG}++" \
  -DCMAKE_C_COMPILER_TARGET=rx-none-elf -DCMAKE_CXX_COMPILER_TARGET=rx-none-elf \
  -DCMAKE_C_FLAGS="$target $OPT" -DCMAKE_CXX_FLAGS="$target $OPT" \
  -DCMAKE_EXE_LINKER_FLAGS="$(echo $ldflags)" \
  -DCMAKE_C_STANDARD_LIBRARIES=-lm -DCMAKE_CXX_STANDARD_LIBRARIES=-lm \
  -DTEST_SUITE_SUBDIRS="$(IFS=';'; echo "${subdirs[*]}")" \
  -DTEST_SUITE_USER_MODE_EMULATION=ON \
  -DTEST_SUITE_RUN_UNDER="python3 $root/scripts/test-suite-run.py" \
  -DTEST_SUITE_HOST_CC=cc \
  -DTEST_SUITE_LIT="$lit" \
  -DTEST_SUITE_COLLECT_CODE_SIZE=OFF \
  -DSMALL_PROBLEM_SIZE=ON \
  -DCMAKE_PROJECT_INCLUDE="$T/host-tools.cmake" \
  -DRX_TS_LINK_DEPENDS="$OUT/rt/start.o;$OUT/rt/syscalls.o;$T/rx-qemu-libc.ld" \
  > "$OUT/cmake.log"

# Keep going past programs that do not build (they count as failures).
ninja -C "$OUT" -k 0 -j "$JOBS" > "$OUT/build.log" 2>&1 || true
echo "built $(grep -c 'FAILED: ' "$OUT/build.log" || true) failures; build log $OUT/build.log"

QEMU_CPU=${QEMU_CPU:-} "$lit" -j "$JOBS" -o "$OUT/results.json" "$OUT" \
  > "$OUT/lit.log" 2>&1 || true
grep -E '^(Total Discovered|  (Passed|Failed|Executable Missing|Unsupported|Timed Out|Unresolved))' "$OUT/lit.log" || tail -20 "$OUT/lit.log"
