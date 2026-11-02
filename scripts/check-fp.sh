#!/bin/bash
# Checks the frame pointer, dynamic alloca and variadic code generation by
# its instructions; fp_main.c and va_main.c check what it does, through
# run-codegen-test.sh. Compiles test/codegen/fp.ll and va.ll for each CPU
# at -O0 / -O2 and
#   - checks -verify-machineinstrs and that GNU as and llc -filetype=obj
#     give the same code, with and without CFI (-force-dwarf-frame-section),
#     and that the CFI does not change the code;
#   - checks that inline assembly clobbering R6 in a function with a frame
#     pointer gets the reserved-register warning;
#   - compares the stack and frame instructions of every function at -O2
#     (rxv3) with test/codegen/fp.expected, and the .debug_frame of fp.ll
#     (as rx-elf-readelf prints it) with fp-cfi.expected.
#
#   check-fp.sh [-u]   (-u rewrites fp.expected and fp-cfi.expected)
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
C=$root/test/codegen
expected=$C/fp.expected
expected_cfi=$C/fp-cfi.expected
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
  for t in fp va; do
    for o in 0 2; do
      for cfi in "" -force-dwarf-frame-section; do
        $LLC -O$o -verify-machineinstrs $cfi "$C/$t.ll" -o "$tmp/f.s"
        $LLC -O$o $cfi -filetype=obj "$C/$t.ll" -o "$tmp/obj${cfi:+-cfi}.o"
        "$G/rx-elf-as" $asflags -o "$tmp/as.o" "$tmp/f.s" 2> /dev/null ||
          [ -n "$cfi" ] # GNU as has no .cfi_* for RX
        if [ -z "$cfi" ]; then
          for v in as obj; do
            "$G/rx-elf-objcopy" -O binary -j .text -j P "$tmp/$v.o" "$tmp/$v.bin"
          done
          if ! cmp -s "$tmp/as.bin" "$tmp/obj.bin"; then
            echo "[fp $t $cpu -O$o] GNU as and llc -filetype=obj differ"
            status=1
          fi
        fi
      done
      "$G/rx-elf-objcopy" -O binary -j .text "$tmp/obj-cfi.o" "$tmp/cfi.bin"
      if ! cmp -s "$tmp/obj.bin" "$tmp/cfi.bin"; then
        echo "[fp $t $cpu -O$o] the code changes with CFI"
        status=1
      fi
    done
  done
done

# Inline assembly clobbering the frame pointer.
cat > "$tmp/clob.ll" <<'EOF'
target triple = "rx-none-elf"
define void @f() "frame-pointer"="all" {
  call void asm sideeffect "nop", "~{r6}"()
  ret void
}
EOF
if ! "$root/build/llvm/bin/llc" -mtriple=rx-none-elf "$tmp/clob.ll" \
     -o /dev/null 2>&1 | grep -q "reserved registers"; then
  echo "[fp] no warning for an asm clobber of the frame pointer"
  status=1
fi

# Stack, frame pointer and call instructions of each function at -O2.
for t in fp va; do
  $root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=rxv3 -O2 "$C/$t.ll" \
    -o "$tmp/$t.s"
done
awk '/^_[A-Za-z0-9_]+:/ { sub(":", "", $1); print ""; print $1; next }
     /^\t(pushm|popm|push\.l|pop|rtsd|bsr\.a)\t/ || /^\trts$/ ||
     /^\t(add|sub)\t.*, r0(, r[0-9]+)?$/ || /^\tadd\t#-?[0-9]+, r6, / ||
     /^\tmov\.l\tr(0|6), r(0|6)$/ || /^\tand\t#-[0-9]+, / {
       sub(/^\t/, "  "); gsub(/\t/, " "); print }' \
  "$tmp/fp.s" "$tmp/va.s" | sed '1d' > "$tmp/got"

# The .debug_frame of fp.ll at -O2 (rxv3).
$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=rxv3 -O2 \
  -force-dwarf-frame-section -filetype=obj "$C/fp.ll" -o "$tmp/cfi.o"
"$G/rx-elf-readelf" --debug-dump=frames "$tmp/cfi.o" | sed '1,/^$/d' \
  > "$tmp/got-cfi"

if [ "${1:-}" = -u ]; then
  cp "$tmp/got" "$expected"
  cp "$tmp/got-cfi" "$expected_cfi"
  echo "updated $expected and $expected_cfi"
  exit $status
fi
if diff -u "$expected" "$tmp/got"; then
  echo "[fp] $(grep -c '^_' "$expected") functions match fp.expected"
else
  status=1
fi
if diff -u "$expected_cfi" "$tmp/got-cfi"; then
  echo "[fp] $(grep -c ' FDE ' "$expected_cfi") FDEs match fp-cfi.expected"
else
  status=1
fi
exit $status
