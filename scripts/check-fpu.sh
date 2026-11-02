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
# It does the same for test/codegen/fpu_ieee.ll (float and double without
# fast-math flags) with -mattr=+flush-denormals, for rxv1 + fpu and rxv3,
# against fpu-ieee.expected: the compares, branches, selects and
# conversions there (NaN-correct conditions such as scn / scge / sco,
# dcmpun) and the libcalls that remain (ueq / one).
#
#   check-fpu.sh [-u]   (-u rewrites fpu.expected and fpu-ieee.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
ll=$root/test/codegen/fpu.ll
expected=$root/test/codegen/fpu.expected
ieee_ll=$root/test/codegen/fpu_ieee.ll
ieee_expected=$root/test/codegen/fpu-ieee.expected
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

# fpu_ieee.ll with +flush-denormals: FPU / DFPU instructions, conditions
# and calls.
: > "$tmp/got-ieee"
for cpu in rxv1 rxv3; do
  case $cpu in
    rxv1) attrs=-mattr=+fpu,+64bit-doubles,+flush-denormals; asflags= ;;
    rxv3) attrs=-mattr=+64bit-doubles,+flush-denormals; asflags=-mcpu=rxv3-dfpu ;;
  esac
  LLC="$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=$cpu $attrs"
  for o in 0 2; do
    $LLC -O$o -verify-machineinstrs "$ieee_ll" -o "$tmp/f.s"
    $LLC -O$o -filetype=obj "$ieee_ll" -o "$tmp/obj.o"
    awk -f "$root/scripts/gas-dfpu.awk" "$tmp/f.s" > "$tmp/gas.s"
    "$G/rx-elf-as" $asflags -o "$tmp/as.o" "$tmp/gas.s"
    for v in as obj; do
      "$G/rx-elf-objcopy" -O binary -j .text -j P "$tmp/$v.o" "$tmp/$v.bin"
    done
    if ! cmp -s "$tmp/as.bin" "$tmp/obj.bin"; then
      echo "[fpu_ieee $cpu -O$o] GNU as and llc -filetype=obj differ"
      status=1
    fi
  done
  echo "== $cpu" >> "$tmp/got-ieee"
  awk '/^_[A-Za-z0-9_]+:/ { sub(":", "", $1); print ""; print $1; next }
       /^\t(f[a-z]+|d[a-z.]+|itof|utof|itod|utod|ftod|mvfdr|bsr\.a)\t/ ||
       /^\tmvfdr$/ || /^\tsc[a-z]+\.l\t/ || /^\tb(n|pz|o|no|lt|ge|gt|le|eq|ne)\.[bw]\t/ {
         sub(/^\t/, "  "); gsub(/\t/, " "); sub(/ \.LBB[0-9_]+$/, " .L"); print }' \
    "$tmp/f.s" >> "$tmp/got-ieee"
  echo >> "$tmp/got-ieee"
done

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  cp "$tmp/got-ieee" "$ieee_expected"
  echo "updated $expected and $ieee_expected"
  exit $status
fi
if diff -u "$expected" "$tmp/got"; then
  echo "[fpu] $(grep -c '^_' "$expected") functions match fpu.expected"
else
  status=1
fi
if diff -u "$ieee_expected" "$tmp/got-ieee"; then
  echo "[fpu] $(grep -c '^_' "$ieee_expected") functions match fpu-ieee.expected"
else
  status=1
fi
exit $status
