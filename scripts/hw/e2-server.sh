# Starting and stopping e2 studio's e2-server-gdb for the AP-RX72M-0A, for
# scripts/hw-run.sh and scripts/hw-lldb.sh (sourced; Git Bash on Windows).
#
# Environment:
#   E2_DEBUGCOMP  e2 studio's DebugComp/RX directory (default: the one under
#                 ~/.eclipse/com.renesas.platform_*)
#   E2_PYTHON     Python 3 (default: the one bundled with e2 studio under
#                 C:/Renesas/e2_studio, else python3)
#   HW_PORT       the server's GDB port (61234)
dc=${E2_DEBUGCOMP:-$(ls -d "$HOME"/.eclipse/com.renesas.platform_*/DebugComp/RX 2>/dev/null | head -1)}
py=${E2_PYTHON:-$(ls /c/Renesas/e2_studio/eclipse/plugins/com.renesas.python3.*/bin/python.exe 2>/dev/null | head -1)}
py=${py:-python3}
port=${HW_PORT:-61234}

# start_server: start the server on $port and wait until it listens.
start_server() {
  [ -x "$dc/e2-server-gdb.exe" ] || { echo "e2-server-gdb.exe not found; set E2_DEBUGCOMP" >&2; exit 2; }
  if tasklist | grep -q -i e2-server-gdb; then
    echo "e2-server-gdb is already running (is e2 studio connected?)" >&2
    exit 2
  fi
  server_log=$(mktemp)
  # The options e2 studio passes for the R5F572MN on the AP-RX72M-0A: E2
  # Lite, JTAG at 6 MHz, EXTAL 12 MHz, register setting 1 (the board's app
  # note AN1526), ID code all F.
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
    -uFirstGDB= main --gdbVersion= 16.2 -p "$port") > "$server_log" 2>&1 &
  for i in $(seq 1 60); do
    grep -q "GDB: $port" "$server_log" && return 0
    if grep -q -i failed "$server_log" || [ "$i" = 60 ]; then
      cat "$server_log"; stop_server; exit 1
    fi
    sleep 1
  done
}

stop_server() {
  taskkill //F //IM e2-server-gdb.exe > /dev/null 2>&1
  rm -f "${server_log:-}"
}
