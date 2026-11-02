#!/bin/bash
# Checks ld.lld (build/llvm, the RX port) against GNU ld: links every codegen
# test (test/codegen/*.ll with its GCC-compiled driver, the start-up code and
# libgcc) with both, for rxv1 and rxv3, from llc's own object file and from
# GNU as's assembly of llc's output, and compares the allocated sections
# (address, size and contents) and the entry point and e_flags.
#
# The two linkers take archive members in different orders (lld extracts a
# member's dependencies at once, GNU ld goes round the archive), which only
# moves code around. So the libgcc members GNU ld used, in its order (from
# its map), are given to both as object files. The ELFs that lld links from
# the archive itself are run on QEMU by run-codegen-test.sh (lld is its
# default linker).
#
#   check-lld.sh
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
LLD=$root/build/llvm/bin/ld.lld
C=$root/test/codegen
GCCFLAGS="-mcpu=rx600 -m64bit-doubles"
libgcc=$("$G/rx-elf-gcc" $GCCFLAGS -print-libgcc-file-name)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/libgcc"
(cd "$tmp/libgcc" && "$root/build/llvm/bin/llvm-ar" x "$libgcc")

# name address size of each allocated section with contents
sections() {
  "$G/rx-elf-readelf" -SW "$1" | awk '{ if (!sub(/^ *\[ *[0-9]+\] /, "")) next }
    $3 ~ /^[0-9a-f]+$/ && $5 ~ /^[0-9a-f]+$/ && $7 ~ /A/ && $2 != "NOBITS" {
      print $1, $3, $5 }'
}

status=0
n=0
nsec=0
"$G/rx-elf-as" -o "$tmp/start.o" "$root/scripts/rx-start.s"
for t in "i32 main" "m3 m3_main" "struct struct_main" "demote demote_main" \
         "atomic atomic_main" "mem mem_main" "mulh mulh_main" "frame frame_main" \
         "fp fp_main" "va va_main" "jt jt_main" "fpu fpu_main" "dfpu dfpu_main" \
         "fpu_ieee fpu_ieee_main"; do
  set -- $t
  "$G/rx-elf-gcc" $GCCFLAGS -O2 -c "$C/$2.c" -o "$tmp/drv.o"
  for cpu in rxv1 rxv3; do
    case $cpu in
      rxv1) attrs=+64bit-doubles,+fpu; asflags= ;;
      rxv3) attrs=+64bit-doubles; asflags=-mcpu=rxv3-dfpu ;;
    esac
    [ "$1" = dfpu ] && [ $cpu = rxv1 ] && continue
    LLC="$root/build/llvm/bin/llc -mtriple=rx-none-elf -mcpu=$cpu -mattr=$attrs -O2"
    $LLC -filetype=obj "$C/$1.ll" -o "$tmp/obj.o"
    $LLC "$C/$1.ll" -o "$tmp/t.s"
    awk -f "$root/scripts/gas-dfpu.awk" "$tmp/t.s" > "$tmp/gas.s"
    "$G/rx-elf-as" $asflags -o "$tmp/as.o" "$tmp/gas.s"
    for v in obj as; do
      objs=("$tmp/start.o" "$tmp/drv.o" "$tmp/$v.o")
      "$G/rx-elf-ld" -T "$root/scripts/rx-qemu.ld" -Map "$tmp/gnu.map" \
        -o "$tmp/gnu0.elf" "${objs[@]}" "$libgcc" 2>/dev/null
      # the libgcc members GNU ld took, in its order
      members=$(awk '/^Archive member included/ { next }
                     match($0, /libgcc\.a\(([^)]*)\)/, m) && !seen[m[1]]++ {
                       print m[1] }' "$tmp/gnu.map" | tr '\n' ' ')
      for m in $members; do objs+=("$tmp/libgcc/$m"); done
      "$G/rx-elf-ld" -T "$root/scripts/rx-qemu.ld" -o "$tmp/gnu.elf" "${objs[@]}" 2>/dev/null
      if ! "$LLD" -T "$root/scripts/rx-qemu.ld" -o "$tmp/lld.elf" "${objs[@]}" \
           2> "$tmp/err"; then
        echo "[lld $1 $cpu $v] ld.lld failed:"; head -5 "$tmp/err"; status=1; continue
      fi
      n=$((n + 1))
      sections "$tmp/gnu.elf" > "$tmp/gnu.sec"
      sections "$tmp/lld.elf" > "$tmp/lld.sec"
      if ! cmp -s "$tmp/gnu.sec" "$tmp/lld.sec"; then
        echo "[lld $1 $cpu $v] section layout differs"
        diff "$tmp/gnu.sec" "$tmp/lld.sec" | head -6
        status=1
        continue
      fi
      [ -s "$tmp/gnu.sec" ] || { echo "[lld $1 $cpu $v] no sections"; status=1; }
      while read -r s _ _; do
        nsec=$((nsec + 1))
        for e in gnu lld; do
          "$G/rx-elf-objcopy" -O binary -j "$s" "$tmp/$e.elf" "$tmp/$e.bin"
        done
        if ! cmp -s "$tmp/gnu.bin" "$tmp/lld.bin"; then
          echo "[lld $1 $cpu $v] $s differs: $(cmp "$tmp/gnu.bin" "$tmp/lld.bin" | head -1)"
          status=1
        fi
      done < "$tmp/gnu.sec"
      for f in Entry Flags; do
        a=$("$G/rx-elf-readelf" -h "$tmp/gnu.elf" | grep "$f")
        b=$("$G/rx-elf-readelf" -h "$tmp/lld.elf" | grep "$f")
        [ "$a" = "$b" ] || { echo "[lld $1 $cpu $v] $f: $a / $b"; status=1; }
      done
    done
  done
done
echo "[lld] $n links ($nsec sections) compared with GNU ld"
exit $status
