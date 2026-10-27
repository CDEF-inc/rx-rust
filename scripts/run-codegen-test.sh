#!/bin/bash
# Compile an LLVM IR test with llc, link it with a GCC-compiled C driver and
# run it on QEMU.
#
#   run-codegen-test.sh [TEST.ll [DRIVER.c]]
#
# The program is built twice: llc -> .s -> GNU as, and llc -> .o directly.
# The driver must define `checks`, `failures`, `first_failure`, `got_value`
# and `want_value` (see qemu-run.sh). Set KEEP_DIR=dir to keep the build
# files there, OPT=0..3 for the llc optimization level (default 2), and
# ATTRS for extra llc features (e.g. ATTRS=+fpu; QEMU's RX62N has the FPU).
# MCPU sets llc's and GNU as's -mcpu (default rxv1), QEMU_CPU the QEMU core
# (default rx62n; the GCC driver uses the FPU, so the core needs one), e.g.
# MCPU=rxv2 QEMU_CPU=rxv2.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
ll=$(realpath "${1:-$root/test/codegen/i32.ll}")
drv=$(realpath "${2:-$root/test/codegen/main.c}")
G=$root/gnu/install/bin
# LLVM's double is always 64 bits, so everything is built for GCC's
# -m64bit-doubles ABI (and linked with that libgcc multilib).
LLC="$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=${MCPU:-rxv1}"
LLC="$LLC -mattr=+64bit-doubles${ATTRS:+,$ATTRS} -O${OPT:-2}"
GCCFLAGS="-mcpu=rx600 -m64bit-doubles"
# GNU as accepts RXv2 / RXv3 / DFPU instructions only with -mcpu.
case ${MCPU:-rxv1} in
  rxv2) ASFLAGS=-mcpu=rxv2 ;;
  rxv3*) ASFLAGS=-mcpu=rxv3-dfpu ;;
  *) ASFLAGS= ;;
esac
libgcc=$("$G/rx-elf-gcc" $GCCFLAGS -print-libgcc-file-name)

if [ -n "${KEEP_DIR:-}" ]; then
  tmp=$KEEP_DIR
  mkdir -p "$tmp"
else
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
fi
cd "$tmp"

"$G/rx-elf-as" -o start.o "$root/scripts/rx-start.s"
"$G/rx-elf-gcc" $GCCFLAGS -O2 -c "$drv" -o main.o
$LLC "$ll" -o test.s
"$G/rx-elf-as" $ASFLAGS -o test_as.o test.s
$LLC -filetype=obj "$ll" -o test_obj.o

status=0
for variant in as obj; do
  "$G/rx-elf-ld" -T "$root/scripts/rx-qemu.ld" -o "t_$variant.elf" start.o \
    main.o "test_$variant.o" "$libgcc" 2>/dev/null
  bash "$root/scripts/qemu-run.sh" "t_$variant.elf" "$variant" || status=1
done
exit $status
