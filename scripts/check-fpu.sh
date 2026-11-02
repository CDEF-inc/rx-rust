#!/bin/bash
# Checks the single-precision FPU code generation (+fpu) by its instructions
# (it is run by run-codegen-test.sh with fpu_main.c on each CPU): compiles
# test/codegen/fpu.ll for rxv1 + fpu, rxv2 and rxv3, assembles the output
# with GNU as (and compares it with llc's own object file), and compares the
# FPU / call instructions of every function at -O2 with
# test/codegen/fpu.expected: rxv1 has only the 2-operand forms, rxv2 and
# later also the 3-operand fadd / fsub / fmul; all of them take a memory
# source ([Rs], dsp:8, dsp:16).
#
#   check-fpu.sh [-u]   (-u rewrites fpu.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
ll=$root/test/codegen/fpu.ll
expected=$root/test/codegen/fpu.expected
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

status=0
: > "$tmp/got"
for cpu in rxv1 rxv2 rxv3; do
  case $cpu in
    rxv1) attrs=-mattr=+fpu,+64bit-doubles; asflags= ;;
    rxv2) attrs=-mattr=+64bit-doubles; asflags=-mcpu=rxv2 ;;
    rxv3) attrs=-mattr=+64bit-doubles; asflags=-mcpu=rxv3-dfpu ;;
  esac
  LLC="$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=$cpu $attrs"
  for o in 0 2; do
    $LLC -O$o -verify-machineinstrs "$ll" -o "$tmp/f.s"
    $LLC -O$o -filetype=obj "$ll" -o "$tmp/obj.o"
    "$G/rx-elf-as" $asflags -o "$tmp/as.o" "$tmp/f.s"
    for v in as obj; do
      "$G/rx-elf-objcopy" -O binary -j .text -j P "$tmp/$v.o" "$tmp/$v.bin"
    done
    if ! cmp -s "$tmp/as.bin" "$tmp/obj.bin"; then
      echo "[fpu $cpu -O$o] GNU as and llc -filetype=obj differ"
      status=1
    fi
  done
  # FPU instructions and calls of each function at -O2 (f.s is -O2 here).
  echo "== $cpu" >> "$tmp/got"
  awk '/^_[A-Za-z0-9_]+:/ { sub(":", "", $1); print ""; print $1; next }
       /^\t(f[a-z]+|itof|utof|round|bsr\.a)\t/ {
         sub(/^\t/, "  "); gsub(/\t/, " "); print }' "$tmp/f.s" >> "$tmp/got"
  echo >> "$tmp/got"
done

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  echo "updated $expected"
elif diff -u "$expected" "$tmp/got"; then
  echo "[fpu] $(grep -c '^_' "$expected") functions match fpu.expected"
else
  status=1
fi
exit $status
