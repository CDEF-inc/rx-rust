#!/bin/bash
# Fetches the rust submodule (CDEF-inc/rust, branch rx-none-elf) and the
# submodules of it that the build needs (the LLVM fork with the RX backend
# and library/backtrace), then writes rust/bootstrap.toml. With
# SETUP_DEPTH=N (CI) only the last N commits of each are fetched.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
fetch=--filter=blob:none
[ -n "${SETUP_DEPTH:-}" ] && fetch=--depth=$SETUP_DEPTH
git submodule update --init $fetch rust
cd rust
git submodule update --init $fetch src/llvm-project library/backtrace
sed "s#@ROOT@#$root#" "$root/scripts/bootstrap.toml.in" > bootstrap.toml
echo "wrote rust/bootstrap.toml"
