#!/bin/bash
# Debugs a program on the AP-RX72M-0A with LLDB, through an E2 Lite and e2
# studio's e2-server-gdb. Runs on Windows (Git Bash): starts the server and
# scripts/hw/lldb-bridge.py, which downloads ELF, resets the board and
# connects to an LLDB waiting on localhost (in WSL; Windows reaches WSL's
# localhost, not the other way round). In either order:
#
#   lldb ELF -o 'process connect --plugin gdb-remote listen://127.0.0.1:61235'
#                                           # in WSL
#   scripts/hw-lldb.sh ELF                  # in Git Bash on Windows
#
# LLDB then stops at the CPU's first instruction. With --listen the bridge
# listens on localhost instead, for an LLDB on Windows (`gdb-remote 61235`).
#
# The bridge serves one LLDB session; when LLDB quits, the bridge and the
# server stop (after `detach` the program keeps running). The board needs
# JSW3 = SGL and JSW2 = SCI; e2 studio must not be connected to it.
# Environment: E2_DEBUGCOMP, E2_PYTHON and HW_PORT (scripts/hw/e2-server.sh),
# LLDB_PORT the port for LLDB (61235).
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
mode=connect:
[ "${1:-}" = --listen ] && { mode=; shift; }
[ $# -eq 1 ] && [ -f "$1" ] || { echo "usage: hw-lldb.sh [--listen] ELF" >&2; exit 2; }
. "$root/scripts/hw/e2-server.sh"
start_server
trap stop_server EXIT
"$py" "$(cygpath -w "$root/scripts/hw/lldb-bridge.py")" "$port" \
  "$mode${LLDB_PORT:-61235}" "$(cygpath -w "$1")"
