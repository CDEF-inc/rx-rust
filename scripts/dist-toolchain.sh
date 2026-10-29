#!/bin/bash
# Packs the built toolchain into one archive, for machines that do not build
# it (CI): the stage1 rustc with the library sources for -Zbuild-std, the
# stage0 cargo, the rx-elf binutils and qemu-system-rx (CDEF-inc/qemu).
#
#   dist-toolchain.sh [NAME]   -> build/dist/NAME.tar.xz
#                                 (default rx-toolchain-x86_64-linux)
#
# The archive holds NAME/ with
#   bin/       rustc, cargo, rx-elf-* (binutils), qemu-system-rx
#   lib/       librustc_driver, rustlib/x86_64-unknown-linux-gnu (the host
#              std, for build scripts and proc macros) and
#              rustlib/src/rust/library (core etc. for -Zbuild-std)
#   VERSIONS   the commits it was built from
#
# To use it: RUSTC=NAME/bin/rustc RUSTC_BOOTSTRAP=1 PATH=NAME/bin:$PATH
# NAME/bin/cargo build ... (with -Zbuild-std=core). The binaries are built on
# Ubuntu 24.04 (glibc 2.39); qemu-system-rx needs QEMU's shared libraries
# (glib, pixman, ...: apt-get install qemu-system-misc brings them).
#
# QEMU picks the qemu-system-rx to pack (default ../qemu/build/qemu-system-rx).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
host=x86_64-unknown-linux-gnu
s1=$root/rust/build/$host/stage1
G=$root/gnu/install/bin
name=${1:-rx-toolchain-x86_64-linux}
QEMU=${QEMU:-$root/../qemu/build/qemu-system-rx}
[ -x "$s1/bin/rustc" ] || { echo "no stage1 rustc; run scripts/build-rustc.sh" >&2; exit 2; }
[ -x "$G/rx-elf-ld" ] || { echo "no rx-elf binutils; run scripts/build-gnu.sh" >&2; exit 2; }
[ -x "$QEMU" ] || { echo "no qemu-system-rx at $QEMU; set QEMU" >&2; exit 2; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
d=$tmp/$name
mkdir -p "$d/bin" "$d/lib/rustlib/src/rust"
cp "$s1/bin/rustc" "$root/rust/build/$host/stage0/bin/cargo" "$d/bin/"
cp "$s1"/lib/librustc_driver-*.so "$d/lib/"
cp -r "$s1/lib/rustlib/$host" "$d/lib/rustlib/"
tar -C "$root/rust" -cf - --exclude=.git library | tar -C "$d/lib/rustlib/src/rust" -xf -
for t in addr2line ar as ld ld.bfd nm objcopy objdump readelf size strings strip; do
  cp "$G/rx-elf-$t" "$d/bin/"
done
cp "$QEMU" "$d/bin/"

{
  echo "rx-rust        $(git -C "$root" rev-parse HEAD)"
  echo "rust           $(git -C "$root/rust" rev-parse HEAD)"
  echo "llvm-project   $(git -C "$root/rust/src/llvm-project" rev-parse HEAD)"
  echo "qemu           $(git -C "$(dirname "$QEMU")/.." rev-parse HEAD 2>/dev/null || echo unknown)"
  echo "rustc          $("$s1/bin/rustc" -V)"
  echo "binutils       $("$G/rx-elf-ld" --version | head -1)"
  echo "qemu-system-rx $("$QEMU" --version | head -1)"
} > "$d/VERSIONS"
cat "$d/VERSIONS"

mkdir -p "$root/build/dist"
out=$root/build/dist/$name.tar.xz
tar -C "$tmp" -cf - "$name" | xz -T0 -6 > "$out"
ls -l "$out"
