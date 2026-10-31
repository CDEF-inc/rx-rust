#!/bin/bash
# Round-trip test of the LLVM RX disassembler.
#
#   mc-dis.sh [-v] [-m CPU] FILE.s
#                      assemble every line on its own with llvm-mc,
#                      disassemble the bytes with llvm-mc --disassemble,
#                      assemble that text again and compare the bytes
#
# The assembler side is checked against GNU as by mc-diff.sh, so the bytes
# llvm-mc makes are taken as right. Lines are read the way mc-diff.sh reads
# them ('#' / ';' comments, "# mcpu: rxv3e"); lines that do not assemble on
# their own (labels) or need a relocation (symbols, whose field reads back
# as 0) are skipped. A line may end with "  ; nodis" to
# record a known difference (reported, not counted). With -v every
# disassembled line is printed; -m CPU overrides the file's CPU (lines the
# CPU does not have are skipped).
set -u

root=$(cd "$(dirname "$0")/.." && pwd)
LLVM=$root/build/llvm/bin

verbose=0
mcpu=
while :; do
  case ${1:-} in
  -v) verbose=1; shift ;;
  -m) mcpu=$2; shift 2 ;;
  *) break ;;
  esac
done
src=$1
[ -n "$mcpu" ] || mcpu=$(sed -n 's/^# mcpu: *//p' "$src" | head -n 1)
mcpu=${mcpu:-rxv3}
MC="$LLVM/llvm-mc -triple=rx-none-elf -mcpu=$mcpu"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Assemble $1 (one or more instructions) and print the .text bytes as
# "0x.. 0x..".
asm_bytes() {
  printf '\t%s\n' "$1" > "$tmp/t.s"
  $MC -filetype=obj -o "$tmp/t.o" "$tmp/t.s" 2>/dev/null || return 1
  "$LLVM/llvm-objcopy" -O binary -j .text "$tmp/t.o" "$tmp/t.bin" &&
    od -An -tx1 -v "$tmp/t.bin" | tr -s ' \n' ' ' |
    sed 's/^ //; s/ $//; s/\([0-9a-f][0-9a-f]\)/0x\1/g'
}

pass=0
fail=0
skip=0
known=0
while IFS= read -r line || [ -n "$line" ]; do
  insn=${line%%;*}
  insn=$(echo "$insn" | sed 's/[[:space:]]*$//')
  case $insn in '' | \#*) continue ;; esac
  b=$(asm_bytes "$insn") || { skip=$((skip + 1)); continue; }
  if [ -z "$b" ] || "$LLVM/llvm-objdump" -r "$tmp/t.o" | grep -q R_RX_; then
    skip=$((skip + 1))
    continue
  fi
  d=$(echo "$b" | $MC --disassemble 2>&1 | grep -v -E '^[[:space:]]*\.text$')
  # One instruction per output line; join them for the second assembly.
  text=$(echo "$d" | sed 's/^[[:space:]]*//' | paste -sd ';')
  b2=$(asm_bytes "$(echo "$d" | sed 's/^[[:space:]]*//')" 2>/dev/null) ||
    b2="ERROR"
  [ $verbose = 1 ] && printf '%-40s %s\n' "$insn" "$text"
  if [ "$b" == "$b2" ] && ! echo "$d" | grep -q -E 'warning|error'; then
    pass=$((pass + 1))
    [[ $line == *"; nodis"* ]] && echo "XPASS  $insn"
  elif [[ $line == *"; nodis"* ]]; then
    known=$((known + 1))
  else
    fail=$((fail + 1))
    echo "FAIL   $insn"
    echo "  bytes: $b"
    echo "    dis: $text"
    echo "  again: $b2"
  fi
done < "$src"

echo "$(basename "$src"): $pass passed, $fail failed, $known known, $skip skipped"
[ $fail = 0 ]
