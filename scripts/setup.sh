#!/bin/bash
# Fetches the rust submodule (CDEF-inc/rust, branch rx-none-elf) and the
# submodules of it that the build needs (the LLVM fork with the RX backend
# and library/backtrace), then writes rust/bootstrap.toml.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
git submodule update --init --filter=blob:none rust
cd rust
git submodule update --init --filter=blob:none src/llvm-project library/backtrace
sed "s#@ROOT@#$root#" "$root/scripts/bootstrap.toml.in" > bootstrap.toml
echo "wrote rust/bootstrap.toml"
