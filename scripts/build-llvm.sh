# Configures LLVM (X86 + the experimental RX target) in build/llvm and builds
# llc and llvm-mc.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
src=$root/rust/src/llvm-project/llvm
out=$root/build/llvm
mkdir -p "$out"
if [ ! -f "$out/build.ninja" ]; then
  cmake -G Ninja -S "$src" -B "$out" \
    -DCMAKE_BUILD_TYPE=Release \
    -DLLVM_ENABLE_ASSERTIONS=ON \
    -DLLVM_TARGETS_TO_BUILD=X86 \
    -DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD=RX \
    -DLLVM_CCACHE_BUILD=ON \
    -DLLVM_PARALLEL_COMPILE_JOBS=6 \
    -DLLVM_PARALLEL_LINK_JOBS=1 \
    -DLLVM_INCLUDE_BENCHMARKS=OFF \
    -DLLVM_INCLUDE_EXAMPLES=OFF \
    -DLLVM_ENABLE_ZSTD=OFF \
    > "$out/cmake.log" 2>&1 || { tail -30 "$out/cmake.log"; exit 1; }
fi
ninja -C "$out" llc llvm-mc > "$out/ninja.log" 2>&1 || { grep -E -B2 -A8 'error|FAILED' "$out/ninja.log" | head -80; exit 1; }
"$out/bin/llc" --version | sed -n '1,4p;/Registered/,$p' | grep -iE 'llvm version|registered|rx|x86'
"$out/bin/llc" -march=rx -mcpu=help 2>&1 | head -20
