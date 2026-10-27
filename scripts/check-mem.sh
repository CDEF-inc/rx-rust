#!/bin/bash
# Checks the memory access forms of code generation (stores of constants,
# and so on) by their instructions; mem_main.c checks what they do, through
# run-codegen-test.sh. Compiles test/codegen/mem.ll for each CPU, assembles
# the output with GNU as (and compares it with llc's own object file), and
# compares the MOV / MOVU / string instructions of every function at -O2
# with test/codegen/mem.expected (rxv3).
#
#   check-mem.sh [-u]   (-u rewrites mem.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
ll=$root/test/codegen/mem.ll
expected=$root/test/codegen/mem.expected
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
      echo "[mem $cpu -O$o] GNU as and llc -filetype=obj differ"
      status=1
    fi
  done
done

# MOV / MOVU / string instructions of each function at -O2 (rxv3).
$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=rxv3 -O2 "$ll" \
  -o "$tmp/m.s"
awk '/^_[A-Za-z0-9_]+:/ { sub(":", "", $1); print ""; print $1; next }
     /^\t(mov|movu)\.[bwl]\t/ ||
     /^\t(smov[bfu]|sstr\.[bwl]|scmpu|suntil\.[bwl]|swhile\.[bwl])$/ {
       sub(/^\t/, "  "); gsub(/\t/, " "); print }' \
  "$tmp/m.s" | sed '1d' > "$tmp/got"

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  echo "updated $expected"
elif diff -u "$expected" "$tmp/got"; then
  echo "[mem] $(grep -c '^_' "$expected") functions match mem.expected"
else
  status=1
fi
exit $status
