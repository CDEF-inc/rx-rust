#!/bin/bash
# dist-toolchain.sh for Windows: packs what scripts/build-windows.sh built
# into one zip. Runs in Git Bash.
#
#   dist-toolchain-windows.sh [NAME]   -> build/dist/NAME.zip
#                                         (default rx-toolchain-x86_64-windows)
#
# The zip holds NAME/ with
#   bin/       rustc, cargo, ld.lld, llvm-{objcopy,objdump,nm,size,readelf,
#              readobj,ar,strip,strings,addr2line,symbolizer} and the same as
#              rx-elf-* (copies: a zip has no links), qemu-system-rx and the
#              MinGW DLLs it needs, lldb, rustc_driver and liblldb
#   lib/       rustlib/x86_64-pc-windows-msvc (the host std, for build scripts
#              and proc macros; bin/rust-lld), rustlib/src/rust/library (core
#              etc. for -Zbuild-std)
#   VERSIONS   the commits it was built from
#   SBOM.spdx.json  the software bill of materials (SPDX 2.3, scripts/sbom.py),
#              also written next to the zip as build/dist/NAME.spdx.json
#
# To use it: put NAME\bin on PATH and set RUSTC=NAME\bin\rustc.exe and
# RUSTC_BOOTSTRAP=1. Linking build scripts needs the MSVC linker (Visual
# Studio Build Tools), as with any x86_64-pc-windows-msvc Rust; in Git Bash
# its directory has to come before /usr/bin, whose link is not the linker.
# The rest needs nothing installed: the MSVC runtime is linked statically.
# scripts/check-dist.sh tests a zip (in Git Bash).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
host=x86_64-pc-windows-msvc
s1=$root/rust/build/$host/stage1
L=$root/build/llvm/bin
Q=$root/build/qemu-windows
name=${1:-rx-toolchain-x86_64-windows}
[ -f "$s1/bin/rustc.exe" ] || { echo "no stage1 rustc; run scripts/build-windows.sh rustc" >&2; exit 2; }
[ -f "$s1/lib/rustlib/$host/bin/rust-lld.exe" ] || { echo "no rust-lld in the sysroot; run scripts/build-windows.sh rustc" >&2; exit 2; }
for t in lld llvm-objcopy llvm-objdump llvm-nm llvm-size llvm-readobj llvm-ar llvm-strings llvm-symbolizer; do
  [ -f "$L/$t.exe" ] || { echo "no $L/$t.exe; run scripts/build-windows.sh llvm" >&2; exit 2; }
done
[ -f "$Q/qemu-system-rx.exe" ] || { echo "no $Q/qemu-system-rx.exe; run scripts/build-windows.sh qemu" >&2; exit 2; }

tmp=$root/build/dist/tmp
rm -rf "$tmp"
trap 'rm -rf "$tmp"' EXIT
d=$tmp/$name
mkdir -p "$d/bin" "$d/lib/rustlib/src/rust"
cp "$s1/bin/rustc.exe" "$s1"/bin/rustc_driver-*.dll "$root/rust/build/$host/stage0/bin/cargo.exe" "$d/bin/"
cp -r "$s1/lib/rustlib/$host" "$d/lib/rustlib/"
tar -C "$root/rust" -cf - --exclude=.git library | tar -C "$d/lib/rustlib/src/rust" -xf -
cp "$Q"/* "$d/bin/"
# The MSYS2 packages of QEMU's DLLs, which the CI's qemu build records
# (windows.yml); without them the SBOM names only the files.
: > "$tmp/libs.txt"
for dll in "$Q"/*.dll; do
  f=$(basename "$dll")
  line=$(grep -F "$f	" "$root/build/qemu-windows.packages" 2> /dev/null | head -1 || true)
  [ -n "$line" ] && echo "$line" >> "$tmp/libs.txt" || printf '%s\t\t\n' "$f" >> "$tmp/libs.txt"
done
cp "$s1/lib/rustlib/$host/bin/rust-lld.exe" "$d/bin/ld.lld.exe"
for t in objcopy objdump nm size readobj ar strings symbolizer; do
  cp "$L/llvm-$t.exe" "$d/bin/"
done
cp "$d/bin/llvm-objcopy.exe" "$d/bin/llvm-strip.exe"
cp "$d/bin/llvm-readobj.exe" "$d/bin/llvm-readelf.exe"
cp "$d/bin/llvm-symbolizer.exe" "$d/bin/llvm-addr2line.exe"
for t in objcopy objdump nm size readelf ar strip strings addr2line; do
  cp "$d/bin/llvm-$t.exe" "$d/bin/rx-elf-$t.exe"
done
lldb=$root/build/lldb/bin
lldb_xml=
if [ -f "$lldb/lldb.exe" ]; then
  cp "$lldb/lldb.exe" "$lldb/liblldb.dll" "$d/bin/"
  # liblldb links libxml2 statically (build-windows.sh lldb), for the SBOM.
  lldb_xml=$(grep -o 'xml_version=[0-9.]*' "$root/scripts/build-windows.sh" | head -1 | cut -d= -f2)
else
  echo "no build/lldb/bin/lldb.exe (build-windows.sh lldb): the zip has no LLDB" >&2
fi

{
  echo "rx-rust        $(git -C "$root" rev-parse HEAD)"
  echo "rust           $(git -C "$root/rust" rev-parse HEAD)"
  echo "llvm-project   $(git -C "$root/rust/src/llvm-project" rev-parse HEAD)"
  echo "qemu           $(grep -v '^#' "$root/scripts/qemu.commit") (scripts/qemu.commit)"
  echo "rustc          $("$d/bin/rustc" -V)"
  echo "llvm tools     $("$d/bin/llvm-objcopy" --version | grep -i 'llvm version' | head -1 | sed 's/^ *//')"
  echo "qemu-system-rx $("$d/bin/qemu-system-rx" --version | head -1 | tr -d '\r')"
  [ -f "$d/bin/lldb.exe" ] && echo "lldb           $("$d/bin/lldb" --version | head -1)"
} > "$d/VERSIONS"
cat "$d/VERSIONS"

# Windows's python (python3 may be missing, or the Store's stub that fails);
# the paths go to it in Windows form.
py=python3
python3 -c '' 2> /dev/null || py=python
"$py" "$(cygpath -m "$root/scripts/sbom.py")" --out "$(cygpath -m "$d/SBOM.spdx.json")" \
  --name "$name" --root "$(cygpath -m "$root")" \
  --rx-rust "$(git -C "$root" rev-parse HEAD)" --rust "$(git -C "$root/rust" rev-parse HEAD)" \
  --llvm "$(git -C "$root/rust/src/llvm-project" rev-parse HEAD)" \
  --qemu "$(grep -v '^#' "$root/scripts/qemu.commit")" \
  --qemu-version "$("$d/bin/qemu-system-rx" --version | head -1 | tr -d '\r')" \
  --rustc-version "$("$d/bin/rustc" -V)" --cargo-version "$("$d/bin/cargo" -V)" \
  ${lldb_xml:+--libxml2 "$lldb_xml"} --libs "$(cygpath -m "$tmp/libs.txt")" --libs-kind msys2

out=$root/build/dist/$name.zip
rm -f "$out"
# Windows's own tar (bsdtar); Git Bash's cannot write a zip.
(cd "$tmp" && "$SYSTEMROOT/System32/tar.exe" -a -cf "$(cygpath -w "$out")" "$name")
cp "$d/SBOM.spdx.json" "$root/build/dist/$name.spdx.json"
ls -l "$out" "$root/build/dist/$name.spdx.json"
