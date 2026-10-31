#!/bin/bash
# Checks the prologues and epilogues of code generation (pushm / rtsd) by
# their instructions; frame_main.c checks what they do, through
# run-codegen-test.sh. Compiles test/codegen/frame.ll for each CPU, assembles
# the output with GNU as (and compares it with llc's own object file), and
# compares the stack instructions (push / pop, SP adjustments, returns) of
# every function at -O2 with test/codegen/frame.expected (rxv3).
#
#   check-frame.sh [-u]   (-u rewrites frame.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
ll=$root/test/codegen/frame.ll
expected=$root/test/codegen/frame.expected
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
    $LLC -O$o -verify-machineinstrs "$ll" -o "$tmp/f.s"
    $LLC -O$o -filetype=obj "$ll" -o "$tmp/obj.o"
    "$G/rx-elf-as" $asflags -o "$tmp/as.o" "$tmp/f.s"
    for v in as obj; do
      "$G/rx-elf-objcopy" -O binary -j .text -j P "$tmp/$v.o" "$tmp/$v.bin"
    done
    if ! cmp -s "$tmp/as.bin" "$tmp/obj.bin"; then
      echo "[frame $cpu -O$o] GNU as and llc -filetype=obj differ"
      status=1
    fi
  done
done

# The stack instructions of each function at -O2 (rxv3).
$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=rxv3 -O2 "$ll" \
  -o "$tmp/f.s"
awk '/^_[A-Za-z0-9_]+:/ { sub(":", "", $1); print ""; print $1; next }
     /^\t(pushm|popm|push\.l|pop|rtsd)\t/ || /^\trts$/ ||
     /^\tadd\t#-?[0-9]+, r0, r0$/ {
       sub(/^\t/, "  "); gsub(/\t/, " "); print }' \
  "$tmp/f.s" | sed '1d' > "$tmp/got"

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  echo "updated $expected"
elif diff -u "$expected" "$tmp/got"; then
  echo "[frame] $(grep -c '^_' "$expected") functions match frame.expected"
else
  status=1
fi
exit $status
