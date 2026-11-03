#!/bin/bash
# Packs the built toolchain into one archive, for machines that do not build
# it (CI): the stage1 rustc with the library sources for -Zbuild-std, the
# stage0 cargo, lld (the RX port: rustc's rust-lld, and bin/ld.lld), the
# LLVM binary tools (objcopy, objdump, nm, ...; also as rx-elf-*) and
# qemu-system-rx (CDEF-inc/qemu), and LLDB with RX support if
# scripts/build-lldb.sh built it. No GNU binutils: Rust programs do not
# need them (C code built with rx-elf-gcc brings its own).
#
#   dist-toolchain.sh [NAME]   -> build/dist/NAME.tar.xz
#                                 (default rx-toolchain-x86_64-linux)
#
# The archive holds NAME/ with
#   bin/       rustc, cargo, ld.lld, llvm-{objcopy,objdump,nm,size,readelf,
#              readobj,ar,strip,strings,addr2line,symbolizer} and the same
#              as rx-elf-* (symbolic links), qemu-system-rx, lldb
#   lib/       librustc_driver, liblldb, rustlib/x86_64-unknown-linux-gnu (the host
#              std, for build scripts and proc macros; bin/rust-lld) and
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
L=$root/build/llvm/bin
name=${1:-rx-toolchain-x86_64-linux}
QEMU=${QEMU:-$root/../qemu/build/qemu-system-rx}
[ -x "$s1/bin/rustc" ] || { echo "no stage1 rustc; run scripts/build-rustc.sh" >&2; exit 2; }
[ -x "$s1/lib/rustlib/$host/bin/rust-lld" ] || { echo "no rust-lld in the sysroot; run scripts/build-rustc.sh" >&2; exit 2; }
for t in lld llvm-objcopy llvm-objdump llvm-nm llvm-size llvm-readobj llvm-ar llvm-strings llvm-symbolizer; do
  [ -x "$L/$t" ] || { echo "no $L/$t; run scripts/build-llvm.sh" >&2; exit 2; }
done
[ -x "$QEMU" ] || { echo "no qemu-system-rx at $QEMU; set QEMU" >&2; exit 2; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
d=$tmp/$name
mkdir -p "$d/bin" "$d/lib/rustlib/src/rust"
cp "$s1/bin/rustc" "$root/rust/build/$host/stage0/bin/cargo" "$d/bin/"
cp "$s1"/lib/librustc_driver-*.so "$d/lib/"
cp -r "$s1/lib/rustlib/$host" "$d/lib/rustlib/"
tar -C "$root/rust" -cf - --exclude=.git library | tar -C "$d/lib/rustlib/src/rust" -xf -
cp "$QEMU" "$d/bin/"
# One lld: rustc's rust-lld (in the sysroot, from build-rustc.sh), and
# ld.lld for linking by hand. lld picks its flavour from its name.
ln -s "../lib/rustlib/$host/bin/rust-lld" "$d/bin/ld.lld"
# The LLVM tools read RX ELF files (objdump disassembles them). The ones
# that are one program under several names are links, as LLVM installs them.
for t in objcopy objdump nm size readobj ar strings symbolizer; do
  cp "$L/llvm-$t" "$d/bin/"
done
ln -s llvm-objcopy "$d/bin/llvm-strip"
ln -s llvm-readobj "$d/bin/llvm-readelf"
ln -s llvm-symbolizer "$d/bin/llvm-addr2line"
for t in objcopy objdump nm size readelf ar strip strings addr2line; do
  ln -s "llvm-$t" "$d/bin/rx-elf-$t"
done
# LLDB (build/lldb): lldb finds liblldb through its RUNPATH, $ORIGIN/../lib.
lldb=$root/build/lldb
if [ -x "$lldb/bin/lldb" ]; then
  cp "$lldb/bin/lldb" "$d/bin/"
  so=$(readlink "$lldb/lib/liblldb.so")              # liblldb.so.23.1
  strip -o "$d/lib/$(readlink -f "$lldb/lib/liblldb.so" | xargs basename)" \
    "$(readlink -f "$lldb/lib/liblldb.so")"
  ln -s "$(readlink -f "$lldb/lib/liblldb.so" | xargs basename)" "$d/lib/$so"
else
  echo "no build/lldb/bin/lldb (scripts/build-lldb.sh): the archive has no LLDB" >&2
fi

{
  echo "rx-rust        $(git -C "$root" rev-parse HEAD)"
  echo "rust           $(git -C "$root/rust" rev-parse HEAD)"
  echo "llvm-project   $(git -C "$root/rust/src/llvm-project" rev-parse HEAD)"
  echo "qemu           $(git -C "$(dirname "$QEMU")/.." rev-parse HEAD 2>/dev/null || echo unknown)"
  echo "rustc          $("$s1/bin/rustc" -V)"
  echo "llvm tools     $("$L/llvm-objcopy" --version | grep -i 'llvm version' | head -1 | sed 's/^ *//')"
  echo "qemu-system-rx $("$QEMU" --version | head -1)"
  [ -x "$d/bin/lldb" ] && echo "lldb           $("$lldb/bin/lldb" --version | head -1)"
} > "$d/VERSIONS"
cat "$d/VERSIONS"

mkdir -p "$root/build/dist"
out=$root/build/dist/$name.tar.xz
tar -C "$tmp" -cf - "$name" | xz -T0 -6 > "$out"
ls -l "$out"
