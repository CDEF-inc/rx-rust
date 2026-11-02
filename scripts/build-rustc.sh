#!/bin/bash
# Builds the stage1 rustc (with the RX backend from build/llvm) and the host
# standard library, and puts build/llvm's lld (the RX port) into the sysroot
# as rust-lld, rx-none-elf's default linker. Log: build/rustc.log
set -o pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$root/build"
cd "$root/rust"
./x build library --stage 1 > "$root/build/rustc.log" 2>&1
status=$?
tail -30 "$root/build/rustc.log"
if [ $status -eq 0 ]; then
  bin=$root/rust/build/x86_64-unknown-linux-gnu/stage1/lib/rustlib/x86_64-unknown-linux-gnu/bin
  { mkdir -p "$bin" && cp "$root/build/llvm/bin/lld" "$bin/rust-lld"; } || {
    echo "cannot install rust-lld (rx-none-elf's linker) from build/llvm/bin/lld; run scripts/build-llvm.sh" >&2
    status=1
  }
fi
exit $status
