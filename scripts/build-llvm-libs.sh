#!/bin/bash
# Builds everything rustc_llvm links against (all LLVM component libraries
# and llvm-config) in the tree configured by build-llvm.sh.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
out=$root/build/llvm
ninja -C "$out" llvm-config llvm-libraries llc llvm-mc > "$out/ninja-libs.log" 2>&1 || {
  grep -E -A12 'error:|FAILED' "$out/ninja-libs.log" | grep -v '^\[' | head -80; exit 1; }
tail -1 "$out/ninja-libs.log"
"$out/bin/llvm-config" --version --targets-built
