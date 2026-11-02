#!/bin/bash
# Checks that multiplies and divisions by constants use emul / emulu, by
# their instructions; mulh_main.c checks what they do, through
# run-codegen-test.sh. Compiles test/codegen/mulh.ll for each CPU, assembles
# the output with GNU as (and compares it with llc's own object file), and
# compares the multiply, divide and call instructions of every function at
# -O2 with test/codegen/mulh.expected (rxv3).
#
#   check-mulh.sh [-u]   (-u rewrites mulh.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
ll=$root/test/codegen/mulh.ll
expected=$root/test/codegen/mulh.expected
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

status=0
for cpu in rxv1 rxv2 rxv3; do
  case $cpu in
    rxv1) asflags= ;;
    rxv2) asflags=-mcpu=rxv2 ;;
    rxv3) asflags=-mcpu=rxv3-dfpu ;;
  esac
  LLC="$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=$cpu"
  for o in 0 2; do
    $LLC -O$o -verify-machineinstrs "$ll" -o "$tmp/m.s"
    $LLC -O$o -filetype=obj "$ll" -o "$tmp/obj.o"
    "$G/rx-elf-as" $asflags -o "$tmp/as.o" "$tmp/m.s"
    for v in as obj; do
      "$G/rx-elf-objcopy" -O binary -j .text -j P "$tmp/$v.o" "$tmp/$v.bin"
    done
    if ! cmp -s "$tmp/as.bin" "$tmp/obj.bin"; then
      echo "[mulh $cpu -O$o] GNU as and llc -filetype=obj differ"
      status=1
    fi
  done
done

# Multiply / divide instructions and calls of each function at -O2 (rxv3).
$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=rxv3 -O2 "$ll" \
  -o "$tmp/m.s"
awk '/^_[A-Za-z0-9_]+:/ { sub(":", "", $1); print ""; print $1; next }
     /^\t(mul|emul|emulu|div|divu|bsr\.a)\t/ {
       sub(/^\t/, "  "); gsub(/\t/, " "); print }' \
  "$tmp/m.s" | sed '1d' > "$tmp/got"

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  echo "updated $expected"
elif diff -u "$expected" "$tmp/got"; then
  echo "[mulh] $(grep -c '^_' "$expected") functions match mulh.expected"
else
  status=1
fi
exit $status
