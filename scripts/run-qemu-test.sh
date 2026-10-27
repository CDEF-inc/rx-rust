#!/bin/bash
# Assembles an instruction test (test/qemu/NAME.s, with test/qemu/harness.s)
# with llvm-mc, links it with scripts/rx-start.s and runs it on QEMU once
# per CPU model.
#
#   run-qemu-test.sh NAME [CPU...]     CPU defaults to rx62n
#
# The test is assembled for each CPU with the symbols V2, V3, V3E, FPU and
# DFPU set to 1 if that core has the ISA or unit, 0 otherwise. QEMU is the
# qemu-system-rx to run (see qemu-run.sh).
set -u
name=$1; shift
cpus=("$@"); [ ${#cpus[@]} -gt 0 ] || cpus=(rx62n)
R=$(cd "$(dirname "$0")/.." && pwd)
G=$R/gnu/install/bin
MC=$R/build/llvm/bin/llvm-mc
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Features of the QEMU CPU models (target/rx/cpu.c in CDEF-inc/qemu).
declare -A FEATURES=(
  [rx62n]="FPU"
  [rxv1]=""
  [rxv2]="V2 FPU"
  [rxv3]="V2 V3 FPU DFPU"
  [rxv3e]="V2 V3 V3E FPU DFPU"
)

"$G/rx-elf-as" "$R/scripts/rx-start.s" -o "$tmp/start.o" || exit 1
status=0
for c in "${cpus[@]}"; do
  [ -n "${FEATURES[$c]+x}" ] || { echo "unknown CPU $c"; exit 2; }
  defs=()
  for f in V2 V3 V3E FPU DFPU; do
    v=0; [[ " ${FEATURES[$c]} " == *" $f "* ]] && v=1
    defs+=(--defsym "$f=$v")
  done
  for s in harness "$name"; do
    "$MC" -triple=rx -mcpu=rxv3e -filetype=obj "${defs[@]}" \
      "$R/test/qemu/$s.s" -o "$tmp/$s.o" || exit 1
  done
  "$G/rx-elf-ld" --no-warn-rwx-segments -T "$R/scripts/rx-qemu.ld" \
    "$tmp/start.o" "$tmp/harness.o" "$tmp/$name.o" -o "$tmp/$c.elf" || exit 1
  QEMU_CPU=$c "$R/scripts/qemu-run.sh" "$tmp/$c.elf" "$name/$c" || status=1
done
exit $status
