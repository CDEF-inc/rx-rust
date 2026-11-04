#!/bin/bash
# Tests a dist-toolchain.sh archive on this machine, which needs nothing
# else from this repository's builds (no GNU tools either): unpacks it,
# builds test/rust/qemu-demo (release and dev) with its rustc, cargo and
# rust-lld and runs it on its qemu-system-rx (run-rust-rom.sh with
# TOOLCHAIN), then runs its ld.lld and lldb.
#
#   check-dist.sh ARCHIVE.tar.xz
#
# The CI runs it on Ubuntu 22.04 and Debian 12. cargo fetches the crates.io
# index for -Zbuild-std, so it needs the network.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
[ $# = 1 ] || { echo "usage: $0 ARCHIVE.tar.xz" >&2; exit 2; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
tar -C "$tmp" -xJf "$1"
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
exit $status
