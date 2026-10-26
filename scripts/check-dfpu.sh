#!/bin/bash
# Checks the DFPU code generation (+dfpu, rxv3), which QEMU cannot run:
# compiles test/codegen/dfpu.ll, assembles the output with GNU as (and
# compares it with llc's own object file), and compares the DFPU / call
# instructions of every function with test/codegen/dfpu.expected, whose
# operand order was checked against the RXv3 manual (DSUB src, src2, dest:
# dest = src2 - src; DCMPcm src, src2: RES = src2 cm src; MVFDR: Z = RES).
#
#   check-dfpu.sh [-u]   (-u rewrites dfpu.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
LLC="$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=rxv3 -mattr=+64bit-doubles"
ll=$root/test/codegen/dfpu.ll
expected=$root/test/codegen/dfpu.expected
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

status=0
for o in 0 2; do
  $LLC -O$o -verify-machineinstrs "$ll" -o "$tmp/d.s"
  $LLC -O$o -filetype=obj "$ll" -o "$tmp/obj.o"
  "$G/rx-elf-as" -mcpu=rxv3-dfpu -o "$tmp/as.o" "$tmp/d.s"
  for v in as obj; do
    "$G/rx-elf-objcopy" -O binary -j .text -j P "$tmp/$v.o" "$tmp/$v.bin"
  done
  if ! cmp -s "$tmp/as.bin" "$tmp/obj.bin"; then
    echo "[dfpu -O$o] GNU as and llc -filetype=obj differ"
    status=1
  fi
done

# DFPU instructions, DR transfers and calls of each function at -O2.
awk '/^_[A-Za-z0-9_]+:/ { sub(":", "", $1); print ""; print $1; next }
     /^\t(d[a-z.]+|itod|utod|mvfdr|bsr\.a)\t/ ||
     /^\tmvfdr$/ { sub(/^\t/, "  "); gsub(/\t/, " "); print }' \
  "$tmp/d.s" | sed '1d' > "$tmp/got"

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  echo "updated $expected"
elif diff -u "$expected" "$tmp/got"; then
  echo "[dfpu] $(grep -c '^_' "$expected") functions match dfpu.expected"
else
  status=1
fi
exit $status
