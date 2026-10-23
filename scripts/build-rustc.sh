#!/bin/bash
# Builds the stage1 rustc (with the RX backend from build/llvm) and the host
# standard library. Log: build/rustc.log
set -o pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$root/build"
cd "$root/rust"
./x build library --stage 1 > "$root/build/rustc.log" 2>&1
status=$?
tail -30 "$root/build/rustc.log"
exit $status
