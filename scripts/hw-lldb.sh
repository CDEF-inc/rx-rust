#!/bin/bash
# Debugs a program on the AP-RX72M-0A, the EK-RX74N or the EK-RX74M with
# LLDB, through an E2 Lite and e2 studio's e2-server-gdb. Runs on Windows
# (Git Bash): starts the server and scripts/hw/lldb-bridge.py, which
# downloads ELF, resets the board and connects to an LLDB waiting on
# localhost (in WSL; Windows reaches WSL's localhost, not the other way
# round). In either order:
#
#   lldb ELF -s scripts/hw/hw.lldb -o hw-connect     # in WSL
#   scripts/hw-lldb.sh ELF                           # in Git Bash on Windows
#
# (hw.lldb defines hw-connect: `process connect --plugin gdb-remote
# listen://127.0.0.1:61235`.) LLDB then stops at the CPU's first
# instruction. With --listen the bridge listens on localhost instead, for an
# LLDB on Windows (`gdb-remote 61235`).
#
# The bridge and the server stay up across LLDB sessions (#86): after
# `kill` (or when LLDB quits), `hw-connect` in LLDB starts the next session:
# the bridge then downloads ELF again, read from the file at that moment,
# and resets the board.
# After a rebuild LLDB needs a new target for the new symbols (`target
# delete`, `target create ELF`, then `hw-connect`). After `detach` the
# program keeps running, and the next session stops it where it is. ^C
# here ends it all; --sessions N ends after N sessions.
#
# The board needs JSW3 = SGL and JSW2 = SCI (AP-RX72M-0A); e2 studio must
# not be connected to it. Environment: HW_BOARD, E2_DEBUGCOMP, E2_PYTHON and
# HW_PORT (scripts/hw/e2-server.sh), LLDB_PORT the port for LLDB (61235).
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
mode=connect:
sessions=0
while [ $# -gt 1 ]; do
  case $1 in
  --listen) mode=; shift ;;
  --sessions) sessions=$2; shift 2 ;;
  *) break ;;
  esac
done
[ $# -eq 1 ] && [ -f "$1" ] ||
  { echo "usage: hw-lldb.sh [--listen] [--sessions N] ELF" >&2; exit 2; }
. "$root/scripts/hw/e2-server.sh"
start_server
trap stop_server EXIT
# ^C reaches the bridge, which then stops the program (or leaves a detached
# one running) and ends. The bridge also ends when the server goes away
# while it waits for LLDB (hw-lldb-check.sh stops a stuck one that way).
"$py" "$(cygpath -w "$root/scripts/hw/lldb-bridge.py")" --sessions "$sessions" \
  "$port" "$mode${LLDB_PORT:-61235}" "$(cygpath -w "$1")"
