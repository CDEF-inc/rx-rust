#!/bin/bash
# Builds the reference GNU toolchain for rx-elf into gnu/install: binutils
# 2.43 (assembler/linker, used to check llvm-mc and to link) and upstream GCC
# 14.2 with libgcc (C side of the ABI tests). Sources are downloaded into
# gnu/src. Do not run it together with an LLVM build (memory).
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
gnu=$root/gnu
jobs=${JOBS:-8}
mkdir -p "$gnu/src" "$gnu/build/bu" "$gnu/build/gcc"
export PATH=$gnu/install/bin:$PATH

fetch() {
  [ -d "$gnu/src/$1" ] && return
  wget -q -O "$gnu/src/$1.tar.xz" "$2"
  tar -C "$gnu/src" -xf "$gnu/src/$1.tar.xz"
}
fetch binutils-2.43 https://ftp.gnu.org/gnu/binutils/binutils-2.43.tar.xz
fetch gcc-14.2.0 https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.xz

# Local fixes in scripts/patches, applied once (skipped when already applied).
#   binutils-2.43-rx-isa-flags.patch: ld kept E_FLAG_RX_V2/V3 only when every
#     input had identical e_flags, so a RXv3 program lost V3 as soon as one
#     object used string instructions and another did not.
for p in "$root"/scripts/patches/binutils-2.43-*.patch; do
  patch -d "$gnu/src/binutils-2.43" -p1 -R -s -f --dry-run < "$p" > /dev/null 2>&1 ||
    patch -d "$gnu/src/binutils-2.43" -p1 -s < "$p"
done

cd "$gnu/build/bu"
[ -f Makefile ] || ../../src/binutils-2.43/configure --target=rx-elf \
  --prefix="$gnu/install" --disable-nls --disable-werror > configure.log 2>&1
make -j"$jobs" > build.log 2>&1
make install >> build.log 2>&1

cd "$gnu/build/gcc"
[ -f Makefile ] || ../../src/gcc-14.2.0/configure --target=rx-elf \
  --prefix="$gnu/install" --enable-languages=c --without-headers --with-newlib \
  --disable-nls --disable-libssp --disable-shared --disable-threads \
  > configure.log 2>&1
make -j"$jobs" all-gcc all-target-libgcc > build.log 2>&1
make install-gcc install-target-libgcc >> build.log 2>&1

rx-elf-as --version | head -1
rx-elf-gcc --version | head -1
rx-elf-gcc -print-multi-lib
