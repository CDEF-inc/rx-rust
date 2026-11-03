#!/bin/bash
# Runs the tests built by hw-build.sh on an AP-RX72M-0A board through an
# E2 Lite. Runs on Windows (Git Bash), with e2 studio installed: starts
# e2 studio's e2-server-gdb, runs scripts/hw/run.py on the ELFs and stops
# the server.
#
#   hw-run.sh [ELF...]     default: every ELF in build/hw (insns, rom,
#                          codegen, rust, rt)
#
# An example (build/hw/examples, no _done) runs for HW_TIMEOUT seconds and,
# given last, is left running when the runner detaches:
#   HW_TIMEOUT=10 hw-run.sh build/hw/examples/blinky.elf
#
# The board needs JSW3 = SGL and JSW2 = SCI (docs/NOTES-hw.md); e2 studio
# must not be connected to it. Environment: E2_DEBUGCOMP, E2_PYTHON and
# HW_PORT (scripts/hw/e2-server.sh), HW_TIMEOUT seconds per test (120).
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/hw/e2-server.sh"

elfs=("$@")
if [ ${#elfs[@]} -eq 0 ]; then
  for g in insns rom codegen rust rt; do
    for f in "$root/build/hw/$g"/*.elf; do [ -f "$f" ] && elfs+=("$f"); done
  done
fi
[ ${#elfs[@]} -gt 0 ] || { echo "no ELF files; run scripts/hw-build.sh first" >&2; exit 2; }
winelfs=()
for f in "${elfs[@]}"; do winelfs+=("$(cygpath -w "$f")"); done

start_server
"$py" "$(cygpath -w "$root/scripts/hw/run.py")" "$port" "${HW_TIMEOUT:-120}" "${winelfs[@]}"
rc=$?
sleep 2
stop_server
exit $rc
