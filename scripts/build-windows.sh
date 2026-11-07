#!/bin/bash
# Builds the parts of the Windows toolchain (host x86_64-pc-windows-msvc),
# which scripts/dist-toolchain-windows.sh packs. Run scripts/setup.sh first.
#
#   scripts\msvc.bat scripts/build-windows.sh llvm    build/llvm
#   scripts\msvc.bat scripts/build-windows.sh rustc   the stage1 rustc
#   scripts\msvc.bat scripts/build-windows.sh lldb    build/lldb
#   scripts/build-windows.sh qemu                     build/qemu-windows
#
# llvm, rustc and lldb need Visual Studio's C++ tools (CMake and Ninja come
# with them) and Python: `python`, else the one in e2 studio. The sources
# must be checked out with core.autocrlf=false, in a short path (C:\rxw).
#
# qemu is built with MinGW, as QEMU does not build with MSVC: in the UCRT64
# shell of MSYS2, with the packages .github/workflows/windows.yml lists.
#
# rustc links its C runtime statically on MSVC (bootstrap's crt_static), so
# LLVM has to as well; it also makes the programs run without the Visual C++
# redistributable. -utf-8: cl reads sources in the system's code page
# otherwise. In code page 932 that can lose the line after a comment, and it
# breaks clang's Lexer. JOBS is the compile parallelism (default all cores).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
src=$root/rust/src/llvm-project/llvm
host=x86_64-pc-windows-msvc
jobs=${JOBS:+-j $JOBS}

msvc() {
  command -v cl > /dev/null || { echo "run this through scripts\\msvc.bat" >&2; exit 2; }
  # Git Bash's /usr/bin/link is not the linker.
  PATH=$(dirname "$(command -v cl)"):$PATH
  # -utf-8, not /utf-8: Git Bash turns what looks like a Unix path into a
  # Windows one, in the environment and in arguments, also after an `=`.
  export CL=-utf-8 VSLANG=1033
  python --version > /dev/null 2>&1 ||
    PATH=$(ls -d /c/Renesas/e2_studio/eclipse/plugins/com.renesas.python3.win32.*/bin | tail -1):$PATH
  python --version
}

# What build/llvm and build/lldb share: the options of build-llvm.sh and
# build-lldb.sh, without ccache.
llvm_flags=(
  -G Ninja -S "$src"
  -DCMAKE_BUILD_TYPE=Release
  -DCMAKE_C_COMPILER=cl -DCMAKE_CXX_COMPILER=cl
  -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded
  -DLLVM_ENABLE_ASSERTIONS=ON
  -DLLVM_TARGETS_TO_BUILD=X86
  -DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD=RX
  -DLLVM_PARALLEL_LINK_JOBS=2
  -DLLVM_INCLUDE_BENCHMARKS=OFF
  -DLLVM_INCLUDE_EXAMPLES=OFF
  -DLLVM_INCLUDE_TESTS=OFF
  -DLLVM_ENABLE_ZSTD=OFF
  -DLLVM_ENABLE_ZLIB=OFF
  -DLLVM_ENABLE_LIBXML2=OFF
  -DLLVM_ENABLE_DIA_SDK=OFF
)

llvm() {
  msvc
  local out=$root/build/llvm
  mkdir -p "$out"
  [ -f "$out/build.ninja" ] || cmake "${llvm_flags[@]}" -B "$out" -DLLVM_ENABLE_PROJECTS=lld \
    > "$out/cmake.log" 2>&1 || { tail -30 "$out/cmake.log"; exit 1; }
  ninja -C "$out" $jobs llvm-config llvm-libraries lld llc llvm-mc llvm-objdump llvm-objcopy \
    llvm-readobj llvm-ar llvm-nm llvm-size llvm-strings llvm-symbolizer > "$out/ninja.log" 2>&1 ||
    { grep -E -A8 'FAILED|error' "$out/ninja.log" | head -60; exit 1; }
  "$out/bin/llc" --version | grep -iE 'llvm version| rx '
}

rustc() {
  msvc
  cd "$root/rust"
  python x.py build library --stage 1 > "$root/build/rustc.log" 2>&1 ||
    { tail -30 "$root/build/rustc.log"; exit 1; }
  local bin=build/$host/stage1/lib/rustlib/$host/bin
  mkdir -p "$bin"
  cp "$root/build/llvm/bin/lld.exe" "$bin/rust-lld.exe"
  "build/$host/stage1/bin/rustc" -vV
}

lldb() {
  msvc
  local out=$root/build/lldb xml_version=2.13.9
  local xml_sha256=a2c9ae7b770da34860050c309f903221c67830c86e4a7e760692b803df95143a
  local xml=$out/libxml2 xml_src=$out/libxml2-src
  mkdir -p "$xml_src"
  if ! ls "$xml"/lib/libxml2*.lib > /dev/null 2>&1; then
    local tarball=$xml_src/libxml2-$xml_version.tar.xz
    curl -fsSL -o "$tarball" \
      "https://download.gnome.org/sources/libxml2/${xml_version%.*}/libxml2-$xml_version.tar.xz"
    echo "$xml_sha256  $tarball" | sha256sum -c --quiet
    tar --force-local -C "$xml_src" -xJf "$tarball"   # C: is not a host
    cmake -G Ninja -S "$xml_src/libxml2-$xml_version" -B "$xml_src/build" \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_C_COMPILER=cl \
      -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded \
      -DCMAKE_POLICY_DEFAULT_CMP0091=NEW \
      -DCMAKE_INSTALL_PREFIX="$xml" \
      -DCMAKE_INSTALL_LIBDIR=lib \
      -DBUILD_SHARED_LIBS=OFF \
      -DLIBXML2_WITH_ICU=OFF \
      -DLIBXML2_WITH_ICONV=OFF \
      -DLIBXML2_WITH_LZMA=OFF \
      -DLIBXML2_WITH_ZLIB=OFF \
      -DLIBXML2_WITH_HTTP=OFF \
      -DLIBXML2_WITH_MODULES=OFF \
      -DLIBXML2_WITH_PYTHON=OFF \
      -DLIBXML2_WITH_PROGRAMS=OFF \
      -DLIBXML2_WITH_TESTS=OFF \
      > "$xml_src/cmake.log" 2>&1 || { tail -30 "$xml_src/cmake.log"; exit 1; }
    ninja -C "$xml_src/build" install > "$xml_src/ninja.log" 2>&1 ||
      { tail -30 "$xml_src/ninja.log"; exit 1; }
  fi
  # The static libxml2 needs LIBXML_STATIC to compile against and bcrypt to
  # link. No lldb-server: it is for debugging the host.
  [ -f "$out/build.ninja" ] || cmake "${llvm_flags[@]}" -B "$out" \
    "-DLLVM_ENABLE_PROJECTS=clang;lldb" \
    -DCMAKE_C_FLAGS=-DLIBXML_STATIC -DCMAKE_CXX_FLAGS=-DLIBXML_STATIC \
    -DCMAKE_SHARED_LINKER_FLAGS=bcrypt.lib \
    -DCLANG_INCLUDE_TESTS=OFF \
    -DLLDB_INCLUDE_TESTS=OFF \
    -DLLDB_ENABLE_PYTHON=OFF \
    -DLLDB_ENABLE_LUA=OFF \
    -DLLDB_ENABLE_LIBEDIT=OFF \
    -DLLDB_ENABLE_CURSES=OFF \
    -DLLDB_ENABLE_LZMA=OFF \
    -DLLDB_ENABLE_LIBXML2=ON \
    -DLIBXML2_INCLUDE_DIR="$xml/include/libxml2" \
    -DLIBXML2_LIBRARY="$(ls "$xml"/lib/libxml2*.lib | head -1)" \
    > "$out/cmake.log" 2>&1 || { tail -30 "$out/cmake.log"; exit 1; }
  ninja -C "$out" $jobs lldb > "$out/ninja.log" 2>&1 ||
    { grep -E -A8 'FAILED|error' "$out/ninja.log" | head -60; exit 1; }
  "$out/bin/lldb" --version
}

qemu() {
  local q=${QEMU_SRC:-$root/../qemu} commit
  commit=$(grep -v '^#' "$root/scripts/qemu.commit")
  if [ ! -d "$q/.git" ]; then
    git init -q "$q"
    git -C "$q" config core.autocrlf false
    git -C "$q" remote add origin https://github.com/CDEF-inc/qemu.git
    git -C "$q" fetch -q --depth 1 origin "$commit"
    git -C "$q" checkout -q FETCH_HEAD
  fi
  # Making the qemu-bundle links needs Developer Mode or an administrator,
  # and qemu-system-rx runs without the bundle.
  sed -i 's/^            raise e$/            continue/' "$q/scripts/symlink-install-tree.py"
  mkdir -p "$q/build"
  cd "$q/build"
  [ -f build.ninja ] || ../configure --target-list=rx-softmmu --disable-docs --disable-werror \
    --disable-guest-agent --disable-tools > configure.log 2>&1 || { tail -30 configure.log; exit 1; }
  ninja $jobs qemu-system-rx.exe > ninja.log 2>&1 ||
    { grep -E -B2 -A8 'error|FAILED' ninja.log | head -60; exit 1; }
  ./qemu-system-rx.exe --version | head -1
  # The program and the MinGW DLLs it needs, for the zip.
  local out=$root/build/qemu-windows
  rm -rf "$out"
  mkdir -p "$out"
  cp qemu-system-rx.exe "$out/"
  ldd qemu-system-rx.exe | awk '$3 ~ /^\/ucrt64\// {print $3}' | xargs -I{} cp {} "$out/"
}

case ${1:-} in
llvm|rustc|lldb|qemu) "$1" ;;
*) sed -n '2,/^set/p' "$0" | sed '$d; s/^# \{0,1\}//' >&2; exit 2 ;;
esac
