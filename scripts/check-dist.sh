#!/bin/bash
# Tests a dist-toolchain.sh archive on this machine, which needs nothing
# else from this repository's builds (no GNU tools either): unpacks it,
# builds test/rust/qemu-demo (release and dev) with its rustc, cargo and
# rust-lld and runs it on its qemu-system-rx (run-rust-rom.sh with
# TOOLCHAIN), then runs its ld.lld and lldb, and looks for its SBOM.
#
#   check-dist.sh ARCHIVE.tar.xz|ARCHIVE.zip
#
# The CI runs it on Ubuntu 22.04 and Debian 12. cargo fetches the crates.io
# index for -Zbuild-std, so it needs the network.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
# Through scripts\msvc.bat: Git Bash's /usr/bin/link comes before MSVC's
# link.exe, which rustc runs to link build scripts.
if command -v cl > /dev/null; then PATH=$(dirname "$(command -v cl)"):$PATH; fi
[ $# = 1 ] || { echo "usage: $0 ARCHIVE.tar.xz|ARCHIVE.zip" >&2; exit 2; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
case $1 in
*.zip) (cd "$tmp" && "$SYSTEMROOT/System32/tar.exe" -xf "$(cygpath -w "$(realpath "$OLDPWD/$1" 2>/dev/null || realpath "$1")")") ;;
*) tar -C "$tmp" -xJf "$1" ;;
esac
T=$(ls -d "$tmp"/*/ | head -1)
T=${T%/}
cat "$T/VERSIONS"
status=0
for p in release dev; do
  if TOOLCHAIN=$T PROFILE=$p bash "$root/scripts/run-rust-rom.sh" > "$tmp/$p.log" 2>&1; then
    echo "ok    qemu-demo $p: $(tail -1 "$tmp/$p.log")"
  else
    echo "FAIL  qemu-demo $p"; tail -20 "$tmp/$p.log"; status=1
  fi
done
for t in ld.lld lldb; do
  [ -e "$T/bin/$t" ] || continue
  if v=$("$T/bin/$t" --version 2>&1); then
    echo "ok    $t: $(echo "$v" | head -1)"
  else
    echo "FAIL  $t"; echo "$v"; status=1
  fi
done
# The SBOMs (scripts/sbom.py): SPDX 2.3 and CycloneDX 1.6 documents that
# name the forks.
s=$T/SBOM.spdx.json
if [ -f "$s" ] && grep -q '"spdxVersion": "SPDX-2.3"' "$s" &&
   grep -q '"name": "llvm-project"' "$s" && grep -q '"name": "qemu"' "$s"; then
  echo "ok    SBOM (SPDX): $(grep -c '"SPDXID"' "$s") SPDX ids"
else
  echo "FAIL  SBOM: no SPDX 2.3 SBOM.spdx.json"; status=1
fi
s=$T/SBOM.cdx.json
if [ -f "$s" ] && grep -q '"bomFormat": "CycloneDX"' "$s" && grep -q '"specVersion": "1.6"' "$s" &&
   grep -q '"name": "llvm-project"' "$s" && grep -q '"name": "qemu"' "$s"; then
  echo "ok    SBOM (CycloneDX): $(grep -c '"bom-ref"' "$s") bom-refs"
else
  echo "FAIL  SBOM: no CycloneDX 1.6 SBOM.cdx.json"; status=1
fi
exit $status
