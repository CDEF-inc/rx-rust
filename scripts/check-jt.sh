#!/bin/bash
# Checks that dense switches use jump tables (and sparse ones do not), by
# their instructions; jt_main.c checks what they do, through
# run-codegen-test.sh. Compiles test/codegen/jt.ll for each CPU, assembles
# the output with GNU as and compares it with llc's own object file, and
# compares the indirect jumps and table addresses of every function at -O2
# with test/codegen/jt.expected (rxv3).
#
# The objects are compared after linking: GNU as keeps the addend of a
# reference to a table (.rodata + offset) in the section contents, llc in
# the relocation, so the unlinked bytes differ.
#
#   check-jt.sh [-u]   (-u rewrites jt.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
ll=$root/test/codegen/jt.ll
expected=$root/test/codegen/jt.expected
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
      "$G/rx-elf-ld" -e 0 -Ttext=0x1000 --unresolved-symbols=ignore-all \
        -o "$tmp/$v.elf" "$tmp/$v.o"
      "$G/rx-elf-objcopy" -O binary -j .text -j P -j .rodata -j C \
        "$tmp/$v.elf" "$tmp/$v.bin"
    done
    if ! cmp -s "$tmp/as.bin" "$tmp/obj.bin"; then
      echo "[jt $cpu -O$o] GNU as and llc -filetype=obj differ"
      status=1
    fi
  done
done

# Indirect jumps and table addresses of each function at -O2 (rxv3).
$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=rxv3 -O2 "$ll" \
  -o "$tmp/m.s"
awk '/^_[A-Za-z0-9_]+:/ { sub(":", "", $1); print ""; print $1; next }
     /^\t(jmp|bra\.l)\t/ || /^\tmov\.l\t#LJTI/ {
       sub(/^\t/, "  "); gsub(/\t/, " "); print }' \
  "$tmp/m.s" | sed '1d' > "$tmp/got"

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  echo "updated $expected"
elif diff -u "$expected" "$tmp/got"; then
  echo "[jt] $(grep -c '^_' "$expected") functions match jt.expected"
else
  status=1
fi
exit $status
