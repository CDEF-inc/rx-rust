#!/bin/bash
# Configures LLDB for X86 + the experimental RX target in build/lldb and
# builds lldb. LLDB needs clang, so this is a separate build directory: the
# LLVM tools in build/llvm (which every test run rebuilds) stay without it.
# No Python or Lua scripting, and none of the optional libraries CMake would
# pick up from the machine (libedit, curses, lzma, zstd), so that lldb needs
# only glibc, libstdc++ and zlib (dist-toolchain.sh). libxml2, which LLDB
# needs to read a GDB server's target.xml and memory map (#82), is built
# here from source as a static library without its own optional
# dependencies (ICU, lzma, zlib). JOBS is the compile parallelism
# (default 6).
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
src=$root/rust/src/llvm-project/llvm
out=$root/build/lldb
mkdir -p "$out"

xml_version=2.13.9
xml_sha256=a2c9ae7b770da34860050c309f903221c67830c86e4a7e760692b803df95143a
xml=$out/libxml2
if [ ! -f "$xml/lib/libxml2.a" ]; then
  xml_src=$out/libxml2-src
  mkdir -p "$xml_src"
  tarball=$xml_src/libxml2-$xml_version.tar.xz
  # Download again when the archive is missing or does not match (an
  # interrupted earlier download), and keep it only once it matches.
  if ! echo "$xml_sha256  $tarball" | sha256sum -c --quiet > /dev/null 2>&1; then
    curl -fsSL -o "$tarball.part" \
      "https://download.gnome.org/sources/libxml2/${xml_version%.*}/libxml2-$xml_version.tar.xz"
    echo "$xml_sha256  $tarball.part" | sha256sum -c --quiet
    mv "$tarball.part" "$tarball"
  fi
  rm -rf "$xml_src/libxml2-$xml_version"
  tar -C "$xml_src" -xJf "$tarball"
  cmake -G Ninja -S "$xml_src/libxml2-$xml_version" -B "$xml_src/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$xml" \
    -DCMAKE_INSTALL_LIBDIR=lib \
    -DBUILD_SHARED_LIBS=OFF \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DLIBXML2_WITH_ICU=OFF \
    -DLIBXML2_WITH_LZMA=OFF \
    -DLIBXML2_WITH_ZLIB=OFF \
    -DLIBXML2_WITH_HTTP=OFF \
    -DLIBXML2_WITH_MODULES=OFF \
    -DLIBXML2_WITH_PYTHON=OFF \
    -DLIBXML2_WITH_PROGRAMS=OFF \
    -DLIBXML2_WITH_TESTS=OFF \
    > "$xml_src/cmake.log" 2>&1 || { tail -30 "$xml_src/cmake.log"; exit 1; }
  ninja -C "$xml_src/build" install > "$xml_src/ninja.log" 2>&1 ||
    { grep -E -B2 -A8 'error|FAILED' "$xml_src/ninja.log" | head -80; exit 1; }
fi

# Configure again when this script has changed (e.g. a build directory
# configured before libxml2 was turned on).
if [ ! -f "$out/build.ninja" ] || [ "$0" -nt "$out/build.ninja" ]; then
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
    -DLLVM_ENABLE_LIBXML2=OFF \
    -DLLDB_ENABLE_LIBEDIT=OFF \
    -DLLDB_ENABLE_CURSES=OFF \
    -DLLDB_ENABLE_LIBXML2=ON \
    -DLIBXML2_INCLUDE_DIR="$xml/include/libxml2" \
    -DLIBXML2_LIBRARY="$xml/lib/libxml2.a" \
    -DLLDB_ENABLE_LZMA=OFF \
    > "$out/cmake.log" 2>&1 || { tail -30 "$out/cmake.log"; exit 1; }
fi
# clang, lldb-test, the LLVM test tools and RXInstEmulationTests are for the
# tests that run-all-tests.sh runs.
ninja -C "$out" lldb clang lldb-test FileCheck count not yaml2obj llvm-config RXInstEmulationTests > "$out/ninja.log" 2>&1 || { grep -E -B2 -A8 'error|FAILED' "$out/ninja.log" | head -80; exit 1; }
"$out/bin/lldb" --version
