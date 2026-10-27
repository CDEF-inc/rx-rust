#!/bin/bash
# Runs a test ELF on QEMU (gdbsim-r5f562n8) and prints the result
# variables the test driver defines (`checks`, `failures`, `first_failure`,
# `got_value`, `want_value`); _start sets `finished` when main returns.
# Results are read through the QEMU monitor.
#
#   qemu-run.sh ELF LABEL     exit status 0 iff the test finished with no
#                             failures
#
# QEMU is the emulator to run (default: ../qemu/build/qemu-system-rx, the
# CDEF-inc/qemu build next to this repository, else qemu-system-rx on
# PATH), QEMU_CPU the -cpu model (default: the board's, rx62n).
set -u
elf=$1
label=$2
G=$(cd "$(dirname "$0")/.." && pwd)/gnu/install/bin
if [ -z "${QEMU:-}" ]; then
  QEMU=$(cd "$(dirname "$0")/.." && pwd)/../qemu/build/qemu-system-rx
  [ -x "$QEMU" ] || QEMU=qemu-system-rx
fi
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

declare -A A=()
for s in checks failures first_failure got_value want_value exit_code finished; do
  A[$s]=$("$G/rx-elf-nm" "$elf" | awk -v s="_$s" '$3 == s {print $1}')
done
head -c 16 /dev/zero > "$tmp/bios.bin"
{
  sleep "${RUN_SECONDS:-3}"
  for s in "${!A[@]}"; do echo "xp /1wd 0x${A[$s]}"; done
  echo "info registers"
  echo quit
} | timeout 60 "$QEMU" -M gdbsim-r5f562n8 \
    ${QEMU_CPU:+-cpu "$QEMU_CPU"} -bios "$tmp/bios.bin" \
    -display none -serial none -device loader,file="$elf",cpu-num=0 \
    -monitor stdio 2>&1 | tr -d '\r' > "$tmp/monitor.log" || true

# The monitor prints addresses as 16 (QEMU 8) or 8 (QEMU 11) hex digits.
get() {
  local key
  key=$(printf '%x' $((16#${A[$1]})))
  awk -v k="$key" '{a = tolower($1); sub(/:$/, "", a); sub(/^0+/, "", a)
                    if (a == k) print $2}' "$tmp/monitor.log"
}
if [ "$(get finished)" != "1" ]; then
  pc=$(grep -o 'pc=0x[0-9a-f]*' "$tmp/monitor.log" | head -1)
  echo "[$label] did not finish ($pc)"
  exit 1
fi
echo "[$label] checks=$(get checks) failures=$(get failures)" \
     "first=$(get first_failure) got=$(get got_value) want=$(get want_value)"
[ "$(get failures)" = "0" ]
