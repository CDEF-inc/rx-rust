#!/bin/bash
# Differential test of the LLVM RX assembler against GNU as (rx-elf).
#
#   mc-diff.sh lines FILE.s   assemble every line on its own with both
#                             assemblers and compare the .text bytes
#   mc-diff.sh file  FILE.s   assemble the whole file with both and compare
#                             .text bytes and the relocation list
#
# Lines starting with '#' or ';' and empty lines are skipped. A line may end
# with "  ; xfail" to record a known difference (reported, not counted).
set -u

root=$(cd "$(dirname "$0")/.." && pwd)
GNU=$root/gnu/install/bin
LLVM=$root/build/llvm/bin
# LLVM's rxv3 includes the DFPU; GNU as needs rxv3-dfpu for the DFPU
# instructions (same e_flags as rxv3).
GAS="$GNU/rx-elf-as -mcpu=rxv3-dfpu"
MC="$LLVM/llvm-mc -triple=rx-none-elf -mcpu=rxv3 -filetype=obj"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# GNU as names the default code section "P" (Renesas convention); LLVM uses
# ".text".
text_hex() {
  "$GNU/rx-elf-objcopy" -O binary -j .text -j P "$1" "$1.bin" 2>/dev/null &&
    od -An -tx1 -v "$1.bin" | tr -s ' \n' ' ' | sed 's/^ //; s/ $//'
}

relocs() {
  "$GNU/rx-elf-readelf" -rW "$1" | awk '/^[0-9a-f]+ /{print $1, $3, $5, $6, $7}'
}

mode=$1
src=$2
pass=0
fail=0
xfail=0

case $mode in
lines)
  while IFS= read -r line || [ -n "$line" ]; do
    insn=${line%%;*}
    insn=$(echo "$insn" | sed 's/[[:space:]]*$//')
    case $insn in '' | \#*) continue ;; esac
    expect_fail=0
    [[ $line == *"; xfail"* ]] && expect_fail=1
    printf '\t%s\n' "$insn" > "$tmp/t.s"
    g=$($GAS -o "$tmp/g.o" "$tmp/t.s" 2>&1) && g=$(text_hex "$tmp/g.o") || g="ERROR: $g"
    l=$($MC -o "$tmp/l.o" "$tmp/t.s" 2>&1) && l=$(text_hex "$tmp/l.o") || l="ERROR: $l"
    if [ "$g" == "$l" ]; then
      pass=$((pass + 1))
      [ $expect_fail = 1 ] && echo "XPASS  $insn"
    elif [ $expect_fail = 1 ]; then
      xfail=$((xfail + 1))
    else
      fail=$((fail + 1))
      echo "FAIL   $insn"
      echo "   gas: $g"
      echo "  llvm: $l"
    fi
  done < "$src"
  ;;
file)
  $GAS -o "$tmp/g.o" "$src" || exit 2
  $MC -o "$tmp/l.o" "$src" || exit 2
  g=$(text_hex "$tmp/g.o")
  l=$(text_hex "$tmp/l.o")
  if [ "$g" == "$l" ]; then pass=$((pass + 1)); else
    fail=$((fail + 1))
    echo "FAIL   .text differs"
    diff <("$GNU/rx-elf-objdump" -d "$tmp/g.o" | tail -n +7) \
         <("$GNU/rx-elf-objdump" -d "$tmp/l.o" | tail -n +7)
  fi
  gf=$("$GNU/rx-elf-readelf" -h "$tmp/g.o" | grep Flags)
  lf=$("$GNU/rx-elf-readelf" -h "$tmp/l.o" | grep Flags)
  if [ "$gf" == "$lf" ]; then pass=$((pass + 1)); else
    fail=$((fail + 1))
    echo "FAIL   e_flags differ"
    echo "   gas: $gf"
    echo "  llvm: $lf"
  fi
  if diff <(relocs "$tmp/g.o") <(relocs "$tmp/l.o") > "$tmp/r.diff"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL   relocations differ (< gas, > llvm)"
    cat "$tmp/r.diff"
  fi
  ;;
*)
  echo "usage: $0 lines|file FILE.s" >&2
  exit 2
  ;;
esac

echo "$(basename "$src"): $pass passed, $fail failed, $xfail expected failures"
[ $fail = 0 ]
