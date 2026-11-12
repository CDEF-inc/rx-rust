#!/bin/bash
# Builds the CC-RX side of scripts/run-ccrx-test.sh: test/ccrx's caller.c and
# callee.c compiled by CC-RX into OUT_DIR/caller_ccrx.o and callee_ccrx.o.
# Run it in Git Bash on Windows, where CC-RX is installed; then run
# run-ccrx-test.sh OUT_DIR in WSL.
#
#   build-ccrx-objs.sh OUT_DIR
#
# CCRX_DIR is CC-RX's installation (default: C:\Program Files (x86)\Renesas\
# RX\3_8_0, V3.08.00). The objects are RXv1 code with 64-bit doubles (clang's
# default), C99, at -optimize=2.
set -eu

out=${1:?usage: build-ccrx-objs.sh OUT_DIR}
root=$(cd "$(dirname "$0")/.." && pwd)
dir=${CCRX_DIR:-"/c/Program Files (x86)/Renesas/RX/3_8_0"}
export BIN_RX="$dir/bin" INC_RX="$dir/include"
mkdir -p "$out"
# CC-RX reads Windows paths; copy the sources next to the objects.
cp "$root"/test/ccrx/abi.h "$root"/test/ccrx/caller.c "$root"/test/ccrx/callee.c "$out/"
cd "$out"
"$dir/bin/ccrx.exe" -version 2>&1 | head -1 || true
for f in caller callee; do
  "$dir/bin/ccrx.exe" -isa=rxv1 -lang=c99 -dbl_size=8 -optimize=2 -nologo \
    -output=obj="${f}_ccrx.o" "$f.c"
done
