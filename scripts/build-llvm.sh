# Configures LLVM (X86 + the experimental RX target, and lld) in build/llvm
# and builds llc, llvm-mc, llvm-objdump, llvm-objcopy and lld (ld.lld).
# Without the optional libraries CMake would pick up from the machine
# (libxml2, zstd): the tools and the rustc linked against them need only
# glibc, libstdc++ and zlib, wherever they were built (dist-toolchain.sh).
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
    -DLLVM_ENABLE_PROJECTS=lld \
    -DLLVM_CCACHE_BUILD=ON \
    -DLLVM_PARALLEL_COMPILE_JOBS=6 \
    -DLLVM_PARALLEL_LINK_JOBS=1 \
    -DLLVM_INCLUDE_BENCHMARKS=OFF \
    -DLLVM_INCLUDE_EXAMPLES=OFF \
    -DLLVM_ENABLE_ZSTD=OFF \
    -DLLVM_ENABLE_LIBXML2=OFF \
    > "$out/cmake.log" 2>&1 || { tail -30 "$out/cmake.log"; exit 1; }
fi
# An older build/llvm without lld gets it added.
grep -q '^LLVM_ENABLE_PROJECTS:STRING=.*lld' "$out/CMakeCache.txt" ||
  cmake -DLLVM_ENABLE_PROJECTS=lld "$out" >> "$out/cmake.log" 2>&1
ninja -C "$out" llc llvm-mc llvm-objdump llvm-objcopy lld \
  FileCheck not count split-file yaml2obj llvm-readobj llvm-readelf llvm-dwarfdump llvm-ar \
  llvm-nm llvm-size llvm-strip llvm-strings llvm-symbolizer llvm-addr2line > "$out/ninja.log" 2>&1 || { grep -E -B2 -A8 'error|FAILED' "$out/ninja.log" | head -80; exit 1; }
"$out/bin/llc" --version | sed -n '1,4p;/Registered/,$p' | grep -iE 'llvm version|registered|rx|x86'
"$out/bin/llc" -march=rx -mcpu=help 2>&1 | head -20
