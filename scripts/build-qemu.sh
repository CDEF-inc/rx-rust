#!/bin/bash
# Builds qemu-system-rx (CDEF-inc/qemu, the RXv2 / RXv3 / RXv3e models the
# tests run on) in ../qemu/build. Without ../qemu it is cloned there at the
# commit in scripts/qemu.commit; an existing checkout is built as it is.
# QEMU_SRC overrides ../qemu.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
src=${QEMU_SRC:-$root/../qemu}
commit=$(grep -v '^#' "$root/scripts/qemu.commit")
if [ ! -d "$src/.git" ]; then
  git init -q "$src"
  git -C "$src" remote add origin https://github.com/CDEF-inc/qemu.git
  git -C "$src" fetch -q --depth 1 origin "$commit"
  git -C "$src" checkout -q FETCH_HEAD
elif [ "$(git -C "$src" rev-parse HEAD)" != "$commit" ]; then
  echo "note: $src is at $(git -C "$src" rev-parse --short HEAD), not scripts/qemu.commit's ${commit:0:10}" >&2
fi
mkdir -p "$src/build"
cd "$src/build"
[ -f build.ninja ] || ../configure --target-list=rx-softmmu --disable-docs \
  --disable-werror > configure.log 2>&1 || { tail -30 configure.log; exit 1; }
ninja qemu-system-rx > ninja.log 2>&1 || { grep -E -B2 -A8 'error|FAILED' ninja.log | head -80; exit 1; }
./qemu-system-rx --version | head -1
