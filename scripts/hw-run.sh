#!/bin/bash
# Runs the tests built by hw-build.sh on an AP-RX72M-0A board, or with
# HW_BOARD=rx74n on an EK-RX74N, or HW_BOARD=rx74m on an EK-RX74M (the RX74N
# tests, build/hw-rx74n), through an E2 Lite. Runs on Windows (Git Bash), with e2 studio installed: starts
# e2 studio's e2-server-gdb, runs scripts/hw/run.py on the ELFs and stops
# the server.
#
#   hw-run.sh [ELF...]     default: every ELF in build/hw (insns, rom,
#                          codegen, rust, rt), or build/hw-rx74n
#
# An example (build/hw/examples, no _done) runs for HW_TIMEOUT seconds and,
# given last, is left running when the runner detaches:
#   HW_TIMEOUT=10 hw-run.sh build/hw/examples/blinky.elf
#
# The AP-RX72M-0A needs JSW3 = SGL and JSW2 = SCI, the EK-RX74N SW6 pin 1 ON
# and pin 2 OFF (docs/NOTES-hw.md); e2 studio must not be connected to the
# board. Environment: E2_DEBUGCOMP, E2_PYTHON, HW_PORT and HW_BOARD
# (scripts/hw/e2-server.sh), HW_TIMEOUT seconds per test (120).
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/hw/e2-server.sh"

elfs=("$@")
if [ ${#elfs[@]} -eq 0 ]; then
  dir=$root/build/hw
  [ "$board" = rx72m ] || dir=$dir-${board/rx74m/rx74n}
  for g in insns rom codegen rust rt; do
    for f in "$dir/$g"/*.elf; do [ -f "$f" ] && elfs+=("$f"); done
  done
fi
[ ${#elfs[@]} -gt 0 ] || { echo "no ELF files; run scripts/hw-build.sh first" >&2; exit 2; }
export HW_BOARD=$board
winelfs=()
for f in "${elfs[@]}"; do winelfs+=("$(cygpath -w "$f")"); done

start_server
"$py" "$(cygpath -w "$root/scripts/hw/run.py")" "$port" "${HW_TIMEOUT:-120}" "${winelfs[@]}"
rc=$?
sleep 2
stop_server
exit $rc
