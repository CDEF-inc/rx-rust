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
# must not be connected to it. Environment:
#   E2_DEBUGCOMP  e2 studio's DebugComp/RX directory (default: the one under
#                 ~/.eclipse/com.renesas.platform_*)
#   E2_PYTHON     Python 3 (default: the one bundled with e2 studio under
#                 C:/Renesas/e2_studio, else python3)
#   HW_PORT       GDB port (61234), HW_TIMEOUT seconds per test (120)
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
dc=${E2_DEBUGCOMP:-$(ls -d "$HOME"/.eclipse/com.renesas.platform_*/DebugComp/RX 2>/dev/null | head -1)}
py=${E2_PYTHON:-$(ls /c/Renesas/e2_studio/eclipse/plugins/com.renesas.python3.*/bin/python.exe 2>/dev/null | head -1)}
py=${py:-python3}
port=${HW_PORT:-61234}
[ -x "$dc/e2-server-gdb.exe" ] || { echo "e2-server-gdb.exe not found; set E2_DEBUGCOMP" >&2; exit 2; }
if tasklist | grep -q -i e2-server-gdb; then
  echo "e2-server-gdb is already running (is e2 studio connected?)" >&2
  exit 2
fi

elfs=("$@")
if [ ${#elfs[@]} -eq 0 ]; then
  for g in insns rom codegen rust rt; do
    for f in "$root/build/hw/$g"/*.elf; do [ -f "$f" ] && elfs+=("$f"); done
  done
fi
[ ${#elfs[@]} -gt 0 ] || { echo "no ELF files; run scripts/hw-build.sh first" >&2; exit 2; }
winelfs=()
for f in "${elfs[@]}"; do winelfs+=("$(cygpath -w "$f")"); done

log=$(mktemp)
# The options e2 studio passes for the R5F572MN on the AP-RX72M-0A: E2 Lite,
# JTAG at 6 MHz, EXTAL 12 MHz, register setting 1 (the board's app note
# AN1526), ID code all F.
(cd "$dc" && exec ./e2-server-gdb.exe -g E2LITE -t R5F572MN -uConnectionTimeout= 30 \
  -uClockSrcHoco= 0 -uInputClock= 12.0000 -uAllowClockSourceInternal= 1 \
  -uUseFine= 0 -uJTagClockFreq= 6.00 -uFineBaudRate= 1.50 \
  -w 0 -z 0 -uRegisterSetting= 1 -uModePin= 0 \
  -uChangeStartupBank= 0 -uStartupBank= 0 -uDebugMode= 0 -uExecuteProgram= 0 \
  -uIdCode= FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF -uresetOnReload= 1 -n 0 \
  -uWorkRamAddress= 1000 -uverifyOnWritingMemory= 0 \
  -uProgReWriteIRom= 0 -uProgReWriteDFlash= 0 \
  -uhookWorkRamAddr= 0x7fb40 -uhookWorkRamSize= 0x4c0 -uOSRestriction= 0 \
  -l "-uCore= SINGLE_CORE|enabled|1|main" -uSyncMode= async \
  -uFirstGDB= main --gdbVersion= 16.2 -p "$port") > "$log" 2>&1 &
for i in $(seq 1 60); do
  grep -q "GDB: $port" "$log" && break
  if grep -q -i failed "$log" || [ "$i" = 60 ]; then
    cat "$log"; taskkill //F //IM e2-server-gdb.exe > /dev/null 2>&1; exit 1
  fi
  sleep 1
done

"$py" "$(cygpath -w "$root/scripts/hw/run.py")" "$port" "${HW_TIMEOUT:-120}" "${winelfs[@]}"
rc=$?
sleep 2
taskkill //F //IM e2-server-gdb.exe > /dev/null 2>&1
rm -f "$log"
exit $rc
