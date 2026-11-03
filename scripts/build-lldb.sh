#!/bin/bash
# Configures LLDB for X86 + the experimental RX target in build/lldb and
# builds lldb. LLDB needs clang, so this is a separate build directory: the
# LLVM tools in build/llvm (which every test run rebuilds) stay without it.
# No Python or Lua scripting. JOBS is the compile parallelism (default 6).
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
src=$root/rust/src/llvm-project/llvm
out=$root/build/lldb
mkdir -p "$out"
if [ ! -f "$out/build.ninja" ]; then
  cmake -G Ninja -S "$src" -B "$out" \
    -DCMAKE_BUILD_TYPE=Release \
    -DLLVM_ENABLE_ASSERTIONS=ON \
    -DLLVM_TARGETS_TO_BUILD=X86 \
    -DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD=RX \
    -DLLVM_ENABLE_PROJECTS="clang;lldb" \
    -DLLDB_ENABLE_PYTHON=OFF \
    -DLLDB_ENABLE_LUA=OFF \
    -DLLVM_CCACHE_BUILD=ON \
    -DLLVM_PARALLEL_COMPILE_JOBS=${JOBS:-6} \
    -DLLVM_PARALLEL_LINK_JOBS=1 \
    -DLLVM_INCLUDE_BENCHMARKS=OFF \
    -DLLVM_INCLUDE_EXAMPLES=OFF \
    -DLLVM_ENABLE_ZSTD=OFF \
    > "$out/cmake.log" 2>&1 || { tail -30 "$out/cmake.log"; exit 1; }
fi
# clang, lldb-test and the LLVM test tools are for the lit tests that
# run-all-tests.sh runs.
ninja -C "$out" lldb clang lldb-test FileCheck count not yaml2obj > "$out/ninja.log" 2>&1 || { grep -E -B2 -A8 'error|FAILED' "$out/ninja.log" | head -80; exit 1; }
"$out/bin/lldb" --version
