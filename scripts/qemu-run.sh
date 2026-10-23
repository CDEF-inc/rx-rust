#!/bin/bash
# Runs a test ELF on QEMU (RX62N, gdbsim-r5f562n8) and prints the result
# variables the test driver defines (`checks`, `failures`, `first_failure`,
# `got_value`, `want_value`); _start sets `finished` when main returns.
# Results are read through the QEMU monitor.
#
#   qemu-run.sh ELF LABEL     exit status 0 iff the test finished with no
#                             failures
set -u
elf=$1
label=$2
G=$(cd "$(dirname "$0")/.." && pwd)/gnu/install/bin
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
} | timeout 60 qemu-system-rx -M gdbsim-r5f562n8 -bios "$tmp/bios.bin" \
    -display none -serial none -device loader,file="$elf",cpu-num=0 \
    -monitor stdio 2>&1 | tr -d '\r' > "$tmp/monitor.log" || true

# The monitor prints addresses as 16 hex digits.
get() {
  local key
  key=$(printf '%016x:' $((16#${A[$1]})))
  awk -v k="$key" 'tolower($1) == k {print $2}' "$tmp/monitor.log"
}
if [ "$(get finished)" != "1" ]; then
  pc=$(grep -o 'pc=0x[0-9a-f]*' "$tmp/monitor.log" | head -1)
  echo "[$label] did not finish ($pc)"
  exit 1
fi
echo "[$label] checks=$(get checks) failures=$(get failures)" \
     "first=$(get first_failure) got=$(get got_value) want=$(get want_value)"
[ "$(get failures)" = "0" ]
