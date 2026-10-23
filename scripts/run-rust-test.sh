#!/bin/bash
# Builds a no_std Rust staticlib for rx-none-elf with -Zbuild-std=core (stage1
# rustc, stage0 cargo), links it with a GCC-compiled C driver and libgcc, and
# runs it on QEMU.
#
#   run-rust-test.sh [CRATE_DIR]   (default test/rust/core-smoke; the crate
#                                   has main.c next to Cargo.toml)
# PROFILE=dev|release (default release).
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
crate=$(realpath "${1:-$root/test/rust/core-smoke}")
profile=${PROFILE:-release}
G=$root/gnu/install/bin
host=x86_64-unknown-linux-gnu
stage1=$root/rust/build/$host/stage1
cargo=$root/rust/build/$host/stage0/bin/cargo
target_dir=$root/build/rust-test
GCCFLAGS="-mcpu=rx600 -m64bit-doubles"
libgcc=$("$G/rx-elf-gcc" $GCCFLAGS -print-libgcc-file-name)

# -Zbuild-std reads the library sources of the stage1 sysroot.
export RUSTC=$stage1/bin/rustc
export RUSTC_BOOTSTRAP=1
export PATH=$G:$PATH
(cd "$crate" && "$cargo" build --profile "$profile" --target rx-none-elf \
  -Zbuild-std=core -Zbuild-std-features=compiler-builtins-mem \
  --target-dir "$target_dir")

dir=$target_dir/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")
lib=$(ls "$dir"/lib*.a | head -1)

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"$G/rx-elf-as" -o "$tmp/start.o" "$root/scripts/rx-start.s"
"$G/rx-elf-gcc" $GCCFLAGS -O2 -c "$crate/main.c" -o "$tmp/main.o"
"$G/rx-elf-ld" -T "$root/scripts/rx-qemu.ld" --gc-sections -o "$tmp/t.elf" \
  "$tmp/start.o" "$tmp/main.o" "$lib" "$libgcc" 2>/dev/null
"$G/rx-elf-size" "$tmp/t.elf"
bash "$root/scripts/qemu-run.sh" "$tmp/t.elf" "rust-$profile"
