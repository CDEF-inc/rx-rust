#!/bin/bash
# Checks that clang's C code interoperates with GCC's on QEMU: test/clang's
# caller.c and callee.c (abi.h) are compiled by clang and by rx-elf-gcc and
# linked all four ways, so that each compiler is the caller and the callee
# of the other (and of itself).
#
#   run-clang-test.sh
#
# RX_CPU picks the QEMU core and clang's -mcpu (see cpu.sh; for rx62n clang
# gets GCC's default -mcpu=rx600, RXv1 with the FPU). GCC has no RXv2 /
# RXv3, so its objects are rx600 code with -m64bit-doubles on every core.
# OPT is the optimization level of both compilers (default 2). CLANG is the
# clang to test (default build/lldb/bin/clang, scripts/build-lldb.sh). The
# clang driver links (with build/llvm's ld.lld, or GNU ld with RX_LD=bfd)
# against the libgcc of gnu/install, which it finds with --gcc-toolchain.
# The start-up code (scripts/rx-start.s) goes through clang's integrated
# assembler. CLANG_FLAGS adds clang options (e.g. -mno-ms-bitfields, which
# must fail). GCC_OBJS=dir takes the GCC side (caller_gcc.o, callee_gcc.o)
# from dir instead, e.g. built by GNURX on Windows for RXv2 / RXv3 and the
# DFPU (docs/NOTES-clang.md). KEEP_DIR=dir keeps the build files there.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/cpu.sh"
CLANG=${CLANG:-$root/build/lldb/bin/clang}
G=$root/gnu/install/bin
T=$root/test/clang
case ${MCPU:-rxv1} in
  rxv1) ccpu=rx600 ;;
  *) ccpu=$MCPU ;;
esac
# -fwrapv: the test values overflow int on purpose.
opt="-O${OPT:-2} -std=gnu11 -fwrapv -Wall"
GCC="$G/rx-elf-gcc -mcpu=rx600 -m64bit-doubles $opt"
CC="$CLANG --target=rx-none-elf -mcpu=$ccpu $opt ${CLANG_FLAGS:-}"
case ${RX_LD:-lld} in
  lld) ld=--ld-path=$root/build/llvm/bin/ld.lld ;;
  bfd) ld=-fuse-ld=bfd ;;
  *) ld=--ld-path=$RX_LD ;;
esac
LINK="$CLANG --target=rx-none-elf -mcpu=$ccpu --gcc-toolchain=$root/gnu/install
  $ld -nostartfiles -nolibc -T $root/scripts/rx-qemu.ld"

if [ -n "${KEEP_DIR:-}" ]; then
  tmp=$KEEP_DIR
  mkdir -p "$tmp"
else
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
fi
cd "$tmp"

$CLANG --target=rx-none-elf -mcpu=$ccpu -c "$root/scripts/rx-start.s" -o start.o
$GCC -ffreestanding -fno-tree-loop-distribute-patterns -c "$T/support.c" -o support.o
for c in gcc clang; do
  if [ $c = gcc ] && [ -n "${GCC_OBJS:-}" ]; then
    cp "$GCC_OBJS/caller_gcc.o" "$GCC_OBJS/callee_gcc.o" .
    continue
  fi
  cc=$GCC; [ $c = clang ] && cc=$CC
  $cc -I"$T" -c "$T/caller.c" -o "caller_$c.o"
  $cc -I"$T" -c "$T/callee.c" -o "callee_$c.o"
done

status=0
for caller in gcc clang; do
  for callee in gcc clang; do
    elf=$caller-$callee.elf
    $LINK start.o "caller_$caller.o" "callee_$callee.o" support.o -o "$elf"
    bash "$root/scripts/qemu-run.sh" "$elf" "caller $caller, callee $callee" || status=1
  done
done
exit $status
