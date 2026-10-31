#!/bin/bash
# Checks the prologues and epilogues of code generation (pushm / rtsd) and
# their CFI by their instructions; frame_main.c checks what they do, through
# run-codegen-test.sh. Compiles test/codegen/frame.ll for each CPU and
#   - assembles the output with GNU as and compares it with llc's own object
#     file;
#   - compiles it again with CFI (-force-dwarf-frame-section, what debug info
#     turns on) and checks that the code stays the same, and that the CFI
#     follows the prologue: the CFA offset after each push and SP adjustment,
#     and the slot of each saved register;
#   - compares the stack instructions (push / pop, SP adjustments, returns) of
#     every function at -O2 with test/codegen/frame.expected, and the
#     .debug_frame (as rx-elf-readelf prints it) with frame-cfi.expected
#     (rxv3).
#
#   check-frame.sh [-u]   (-u rewrites frame.expected and frame-cfi.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
ll=$root/test/codegen/frame.ll
expected=$root/test/codegen/frame.expected
expected_cfi=$root/test/codegen/frame-cfi.expected
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Reads llc assembly with CFI and prints a line for each mismatch between
# the prologue and its CFI. The CFA is SP + 4 on entry; a push of lo..hi
# adds 4 bytes per register and puts lo at the lowest address.
check_cfi() {
  awk '
    function fail(msg) { print "  " fn ": " msg; bad = 1 }
    /^_[A-Za-z0-9_]+:/ { fn = $1; sub(":", "", fn) }
    /\.cfi_startproc/ { cfa = 4; pend = 0; lo = -1; nreg = 0; noff = 0 }
    /^\t(pushm|push\.l)\t/ {
      n = split($2, r, "-")
      lo = substr(r[1], 2) + 0; hi = substr(r[n], 2) + 0
      nreg = hi - lo + 1; cfa += 4 * nreg; pend = cfa; pushcfa = cfa
    }
    /^\tadd\t#-[0-9]+, r0, r0$/ {
      a = $2; sub("#-", "", a); sub(",", "", a)
      cfa += a; pend = cfa
    }
    /\.cfi_def_cfa_offset/ {
      if (!pend) fail("unexpected .cfi_def_cfa_offset " $2)
      else if ($2 != pend) fail(".cfi_def_cfa_offset " $2 ", want " pend)
      pend = 0
    }
    /\.cfi_offset/ {
      reg = $2; sub(",", "", reg); reg += 0; noff++
      want = -pushcfa + 4 * (reg - lo)
      if (lo < 0 || reg < lo || reg > hi) fail("r" reg " was not pushed")
      else if ($3 != want) fail("r" reg " at " $3 ", want " want)
    }
    /\.cfi_endproc/ {
      if (pend) fail("no .cfi_def_cfa_offset " pend)
      if (noff != nreg) fail(noff " .cfi_offset for " nreg " registers")
      funcs++
    }
    END {
      if (!funcs) print "  no .cfi_startproc / .cfi_endproc"
      exit bad || !funcs
    }' "$1"
}

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
    $LLC -O$o -verify-machineinstrs -force-dwarf-frame-section "$ll" \
      -o "$tmp/cfi.s"
    $LLC -O$o -force-dwarf-frame-section -filetype=obj "$ll" -o "$tmp/cfi.o"
    for v in as obj cfi; do
      "$G/rx-elf-objcopy" -O binary -j .text -j P "$tmp/$v.o" "$tmp/$v.bin"
    done
    if ! cmp -s "$tmp/as.bin" "$tmp/obj.bin"; then
      echo "[frame $cpu -O$o] GNU as and llc -filetype=obj differ"
      status=1
    fi
    if ! cmp -s "$tmp/obj.bin" "$tmp/cfi.bin"; then
      echo "[frame $cpu -O$o] the code changes with CFI"
      status=1
    fi
    if ! check_cfi "$tmp/cfi.s"; then
      echo "[frame $cpu -O$o] the CFI does not follow the prologue"
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

# The .debug_frame at -O2 (rxv3).
$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=rxv3 -O2 \
  -force-dwarf-frame-section -filetype=obj "$ll" -o "$tmp/cfi.o"
"$G/rx-elf-readelf" --debug-dump=frames "$tmp/cfi.o" | sed '1,/^$/d' \
  > "$tmp/got-cfi"

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  cp "$tmp/got-cfi" "$expected_cfi"
  echo "updated $expected and $expected_cfi"
  exit $status
fi
if diff -u "$expected" "$tmp/got"; then
  echo "[frame] $(grep -c '^_' "$expected") functions match frame.expected"
else
  status=1
fi
if diff -u "$expected_cfi" "$tmp/got-cfi"; then
  echo "[frame] $(grep -c ' FDE ' "$expected_cfi") FDEs match frame-cfi.expected"
else
  status=1
fi
exit $status
