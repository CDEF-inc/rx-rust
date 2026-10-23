#!/bin/bash
# Builds a Rust-only rx-rt application (stage1 rustc, stage0 cargo, the
# crate's .cargo/config.toml), turns it into a 512 KB code-flash image and
# boots it on QEMU's RX62N from the reset vector. SCI0 output is printed; the
# result variables (`checks`, `failures`, `first_failure`, `finished`) are
# read through the QEMU monitor.
#
#   run-rust-rom.sh [CRATE_DIR]    (default test/rust/qemu-demo)
# PROFILE=dev|release (default release), RUN_SECONDS (default 3).
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
crate=$(realpath "${1:-$root/test/rust/qemu-demo}")
profile=${PROFILE:-release}
G=$root/gnu/install/bin
host=x86_64-unknown-linux-gnu
cargo=$root/rust/build/$host/stage0/bin/cargo
target_dir=$root/build/rust-rom

export RUSTC=$root/rust/build/$host/stage1/bin/rustc
export RUSTC_BOOTSTRAP=1
export PATH=$G:$PATH
(cd "$crate" && "$cargo" build --profile "$profile" --target-dir "$target_dir")

name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$crate/Cargo.toml" | head -1)
elf=$target_dir/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")/$name
"$G/rx-elf-size" -A "$elf" | awk '$2 != 0 && $1 !~ /^\.debug/'

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"$G/rx-elf-objcopy" -O binary "$elf" "$tmp/rom.bin"
size=$(stat -c %s "$tmp/rom.bin")
if [ "$size" != 524288 ]; then
  echo "ROM image is $size bytes, expected 524288" >&2
  exit 1
fi

declare -A A=()
for s in checks failures first_failure finished; do
  A[$s]=$("$G/rx-elf-nm" "$elf" | awk -v s="_$s" '$3 == s {print $1}')
done
{
  sleep "${RUN_SECONDS:-3}"
  for s in "${!A[@]}"; do echo "xp /1wd 0x${A[$s]}"; done
  echo "info registers"
  echo quit
} | timeout 60 qemu-system-rx -M gdbsim-r5f562n8 -bios "$tmp/rom.bin" \
    -display none -serial file:"$tmp/sci0.txt" -monitor stdio 2>&1 \
  | tr -d '\r' > "$tmp/monitor.log" || true

echo "--- SCI0"
tr -d '\r' < "$tmp/sci0.txt"
echo "---"
get() {
  local key
  key=$(printf '%016x:' $((16#${A[$1]})))
  awk -v k="$key" 'tolower($1) == k {print $2}' "$tmp/monitor.log"
}
label=$(basename "$crate")-$profile
if [ "$(get finished)" != "1" ]; then
  pc=$(grep -o 'pc=0x[0-9a-f]*' "$tmp/monitor.log" | head -1)
  echo "[$label] did not finish ($pc)"
  exit 1
fi
echo "[$label] checks=$(get checks) failures=$(get failures) first=$(get first_failure)"
[ "$(get failures)" = "0" ]
