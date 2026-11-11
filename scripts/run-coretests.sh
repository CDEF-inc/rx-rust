#!/bin/bash
# Builds library/coretests, the tests of `core`, for rx-none-elf
# (test/rust/coretests: stage1 rustc, stage0 cargo, -Zbuild-std=core,alloc)
# and runs them on QEMU (gdbsim-r5f562n8, loaded into the SDRAM). The test
# output on SCI0 is in libtest's format; it is printed with -v, and kept in
# the target directory as coretests-PROFILE.log (the build's warnings in
# build-PROFILE.log).
#
#   run-coretests.sh [-v]
#
# The failing tests are compared with test/rust/coretests/expected-failures
# (lines "CPU TEST  # why", CPU `*` for every core): the exit status is 0
# iff the tests finished and exactly those failed. PROFILE=release|dev
# (default release). RX_CPU picks the CPU model (see cpu.sh; default RXv1
# code on QEMU's rx62n). RUN_TIMEOUT is the limit in seconds (default 900).
# PART=+MOD,... or -MOD,... runs only those test modules, or all but those
# (coretests-part.sh; the board runs PART=+num and PART=-num). QEMU and
# QEMU_FLAGS as in qemu-run.sh.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/cpu.sh"
verbose=0
[ "${1:-}" = -v ] && verbose=1
crate=$root/test/rust/coretests
profile=${PROFILE:-release}
G=$root/gnu/install/bin
host=x86_64-unknown-linux-gnu
cargo=$root/rust/build/$host/stage0/bin/cargo
target_dir=$root/build/coretests/${RX_CPU:-default}
part=
if [ -n "${PART:-}" ]; then
  case $PART in +*) part=-${PART#+} ;; *) part=-no-${PART#-} ;; esac
  part=${part//,/-}
  target_dir=$target_dir$part
  crate=$target_dir/src
  bash "$root/scripts/coretests-part.sh" "$PART" "$crate" > /dev/null
fi
if [ -z "${QEMU:-}" ]; then
  QEMU=$root/../qemu/build/qemu-system-rx
  [ -x "$QEMU" ] || QEMU=qemu-system-rx
fi

export RUSTC=$root/rust/build/$host/stage1/bin/rustc
export RUSTC_BOOTSTRAP=1
export PATH=$G:$PATH
if [ -n "${RUST_CPU_FLAGS:-}" ]; then
  # RUSTFLAGS replaces the crate's rustflags, so give the linker script too.
  export RUSTFLAGS="-C link-arg=-Tcoretests.x $RUST_CPU_FLAGS"
fi
mkdir -p "$target_dir"
# coretests has many warnings for this target; they go to the build log.
build_log=$target_dir/build-$profile.log
elf=$(cd "$crate" && "$cargo" test --no-run --profile "$profile" \
        --target-dir "$target_dir" --message-format=json-render-diagnostics \
        2> "$build_log" | sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -1)
if [ -z "$elf" ]; then
  grep -E '^error' -A12 "$build_log" | head -80
  echo "no test executable (see $build_log)" >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
log=$target_dir/coretests-$profile.log
head -c 16 /dev/zero > "$tmp/bios.bin"
"$QEMU" -M gdbsim-r5f562n8 ${QEMU_CPU:+-cpu "$QEMU_CPU"} ${QEMU_FLAGS:-} \
  -bios "$tmp/bios.bin" -display none -monitor none \
  -serial file:"$tmp/sci0.txt" -device loader,file="$elf",cpu-num=0 &
qemu=$!
# The runner prints "test result: ..." at the end, or "panic outside a
# test" if the harness itself panics.
end='^test result: (ok|FAILED)\.|^panic outside a test'
for ((t = 0; t < ${RUN_TIMEOUT:-900}; t++)); do
  tr -d '\r' 2> /dev/null < "$tmp/sci0.txt" | grep -qE "$end" && break
  kill -0 $qemu 2> /dev/null || break
  sleep 1
done
kill $qemu 2> /dev/null || true
wait $qemu 2> /dev/null || true
tr -d '\r' < "$tmp/sci0.txt" > "$log"
[ $verbose = 1 ] && cat "$log"

label=coretests$part-$profile${RX_CPU:+-$RX_CPU}
if ! grep -qE "$end" "$log"; then
  echo "[$label] did not finish; the last test started:"
  grep '^test .* \.\.\.$' "$log" | tail -1
  exit 1
fi
grep -E '^---- |^panic outside' -A6 "$log" || true
summary=$(grep -E "$end" "$log")
# The names in the "failures:" list.
sed -n '/^failures:$/,/^$/s/^    //p' "$log" | sort > "$tmp/failed"
cpu=${RX_CPU:-rx62n}
# Of the expected failures, those that ran (all but with PART).
sed -n 's/^test \(.*\) \.\.\.$/\1/p' "$log" | sort > "$tmp/ran"
awk -v c="$cpu" '$1 == c || $1 == "*" {print $2}' \
  "$root/test/rust/coretests/expected-failures" | sort | comm -12 - "$tmp/ran" \
  > "$tmp/expected"
status=0
if grep -q '^panic outside' "$log"; then status=1; fi
while read -r t; do
  echo "FAIL (unexpected)  $t"; status=1
done < <(comm -23 "$tmp/failed" "$tmp/expected")
while read -r t; do
  echo "XPASS  $t"; status=1
done < <(comm -13 "$tmp/failed" "$tmp/expected")
echo "[$label] ${summary#test result: } ($(wc -l < "$tmp/expected") expected failures)"
exit $status
