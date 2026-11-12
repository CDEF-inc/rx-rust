#!/bin/bash
# Checks that clang's C code interoperates with CC-RX's on QEMU: test/ccrx's
# caller.c and callee.c (abi.h) are compiled by clang (its default -mabi=ccrx)
# and by CC-RX and linked all four ways, so that each compiler is the caller
# and the callee of the other (and of itself).
#
#   run-ccrx-test.sh CCRX_DIR
#
# CC-RX runs on Windows only, so its side comes prebuilt: CCRX_DIR holds
# caller_ccrx.o and callee_ccrx.o, made by scripts/build-ccrx-objs.sh in Git
# Bash. They are RXv1 code (-isa=rxv1 -dbl_size=8), which runs on every QEMU
# core. RX_CPU picks the QEMU core and clang's -mcpu (see cpu.sh). OPT is
# clang's optimization level (default 2), CLANG the clang to test (default
# build/lldb/bin/clang, scripts/build-lldb.sh). The objects are linked by
# build/llvm's ld.lld with scripts/rx-qemu.ld, which places CC-RX's P / C /
# D / B sections. KEEP_DIR=dir keeps the build files there.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/cpu.sh"
ccrx=${1:?usage: run-ccrx-test.sh CCRX_DIR (from scripts/build-ccrx-objs.sh)}
ccrx=$(cd "$ccrx" && pwd)
CLANG=${CLANG:-$root/build/lldb/bin/clang}
G=$root/gnu/install/bin
T=$root/test/ccrx
case ${MCPU:-rxv1} in
  rxv1) ccpu=rx600 ;;
  *) ccpu=$MCPU ;;
esac
# -fwrapv: the test values overflow int on purpose.
CC="$CLANG --target=rx-none-elf -mcpu=$ccpu -O${OPT:-2} -std=c99 -fwrapv -Wall
  ${CLANG_FLAGS:-}"
LINK="$root/build/llvm/bin/ld.lld -T $root/scripts/rx-qemu.ld"

if [ -n "${KEEP_DIR:-}" ]; then
  tmp=$KEEP_DIR
  mkdir -p "$tmp"
else
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
fi
cd "$tmp"

$CLANG --target=rx-none-elf -mcpu=$ccpu -c "$root/scripts/rx-start.s" -o start.o
cp "$ccrx/caller_ccrx.o" "$ccrx/callee_ccrx.o" .
$CC -I"$T" -c "$T/caller.c" -o caller_clang.o
$CC -I"$T" -c "$T/callee.c" -o callee_clang.o

status=0
for caller in ccrx clang; do
  for callee in ccrx clang; do
    elf=$caller-$callee.elf
    $LINK start.o "caller_$caller.o" "callee_$callee.o" -o "$elf"
    bash "$root/scripts/qemu-run.sh" "$elf" "caller $caller, callee $callee" || status=1
  done
done
exit $status
