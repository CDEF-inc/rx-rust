#!/bin/bash
# Debugs test/rust/lldb-demo on the board with LLDB and checks what LLDB
# shows, as check-lldb.sh does on QEMU (#87): a batch LLDB session (in WSL)
# against scripts/hw-lldb.sh's bridge. It checks the backtrace through the
# Rust frames, arguments, rustc's Rust formatters (when LLDB has Python),
# the unwinding from an interrupt handler into code that ran in user mode,
# a single step into `int #64` (the bridge runs it to the handler, #98),
# an interrupt (^C) of the running program, and the next sessions after
# `kill` (the bridge loads the ELF again) and after `detach` (#86).
#
#   hw-lldb-check.sh [ELF]   default: build/hw/lldb/lldb-demo.elf (hw-build.sh
#                            lldb), or build/hw-rx74n/lldb/lldb-demo.elf
#
# Runs on Windows (Git Bash) with the repository in WSL, where LLDB
# (build/lldb) runs: LLDB_DISTRO is the WSL distribution (Ubuntu). HW_BOARD
# and the board settings as for hw-lldb.sh.
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
distro=${LLDB_DISTRO:-Ubuntu}
board=${HW_BOARD:-rx72m}
case $root in
  //wsl.localhost/"$distro"/* | //wsl\$/"$distro"/*) wroot=/${root#//*/*/} ;;
  *) echo "hw-lldb-check.sh needs the repository in WSL ($distro)" >&2; exit 2 ;;
esac
dir=build/hw; [ "$board" = rx72m ] || dir=$dir-rx74n
elf=${1:-$root/$dir/lldb/lldb-demo.elf}
[ -f "$elf" ] || { echo "no $elf; run HW_BOARD=$board scripts/hw-build.sh lldb" >&2; exit 2; }
welf=/${elf#//*/*/}
wsl() { MSYS_NO_PATHCONV=1 wsl.exe -d "$distro" -- "$@"; }

# The `int #64` of the user-mode loop (user.rs): _lldb_user_loop + 11.
loop=$(wsl "$wroot/build/llvm/bin/llvm-nm" "$welf" | awk '$3 == "_lldb_user_loop" {print $1}')
[ -n "$loop" ] || { echo "no _lldb_user_loop in $elf" >&2; exit 2; }
int=$(printf '0x%x' $((0x$loop + 11)))
# Next to TRAPS (user.rs), for a second watchpoint, which the bridge refuses.
traps=$(wsl "$wroot/build/llvm/bin/llvm-nm" "$welf" | awk '$3 == "_TRAPS" {print $1}')
[ -n "$traps" ] || { echo "no TRAPS in $elf" >&2; exit 2; }
wp2=$(printf '%x' $((0x$traps + 4)))

port=${LLDB_PORT:-61235}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# wsl.exe loses the arguments after a `bash -c` script, so what runs in WSL
# is a file.
run=$dir/lldb/hw-lldb-check
# The formatters and the ^C (which needs LLDB's asynchronous mode, set from
# Python) need an LLDB with Python (build-lldb.sh LLDB_PYTHON=1).
cat > "$root/$run.probe" << EOF
"$wroot/build/lldb/bin/lldb" -b -o 'script print("python" + "-ok")'
EOF
python=no
wsl bash "$wroot/$run.probe" 2>&1 | grep -q '^python-ok' && python=yes
cat > "$tmp/cmds" << EOF
settings set use-color false
settings set interpreter.stop-command-source-on-error false
settings set plugin.process.gdb-remote.packet-timeout 30
process connect --plugin gdb-remote listen://127.0.0.1:$port
breakpoint set -n scale
continue
bt
frame variable
breakpoint set -n seen
continue
up
EOF
[ $python = yes ] && cat >> "$tmp/cmds" << EOF
command script import $wroot/rust/src/etc/lldb_lookup.py
frame variable some rect name slice
EOF
cat >> "$tmp/cmds" << EOF
breakpoint delete -f
breakpoint set -n trap
continue
bt
register read usp
frame select 2
register read psw r0
breakpoint delete -f
breakpoint set -a $int
continue
si
bt
breakpoint delete -f
breakpoint set -n trap
continue
breakpoint delete -f
watchpoint set variable TRAPS
process plugin packet send qWatchpointSupportInfo:
process plugin packet send Z2,$wp2,1
continue
continue
watchpoint delete -f
EOF
# Without Python `process continue` would not come back: the program loops.
[ $python = yes ] && cat >> "$tmp/cmds" << EOF
script lldb.debugger.SetAsync(True)
process continue
script import time; time.sleep(2)
process interrupt
script import time; time.sleep(2)
script lldb.debugger.SetAsync(False)
process status
EOF
# Session 2 (#86): after `kill` the bridge loads the ELF again and resets;
# the ELF, touched before connecting (as a rebuild would), is loaded again
# then; and the breakpoint hits again. Session 3: after `detach` the program
# runs on (into the user-mode loop), and connecting stops it. The markers
# split the output by session (`platform shell` runs on this side).
cat >> "$tmp/cmds" << EOF
kill
platform shell sleep 15
platform shell touch $welf
platform shell echo @@session2
process connect --plugin gdb-remote listen://127.0.0.1:$port
bt
breakpoint set -n scale
continue
bt
breakpoint delete -f
watchpoint set variable finished
continue
watchpoint delete -f
process detach
platform shell sleep 2
platform shell echo @@session3
process connect --plugin gdb-remote listen://127.0.0.1:$port
bt
kill
EOF
# LLDB waits for the bridge; the bridge (hw-lldb.sh) connects to it.
cp "$tmp/cmds" "$root/$run.cmds"
cat > "$root/$run.run" << EOF
args=()
while IFS= read -r c; do args+=(-o "\$c"); done < "$wroot/$run.cmds"
timeout -k 10 ${LLDB_TIMEOUT:-300} "$wroot/build/lldb/bin/lldb" -b -x "$welf" "\${args[@]}"
EOF
wsl bash "$wroot/$run.run" > "$tmp/out" 2>&1 &
lldb=$!
sleep 3
LLDB_PORT=$port bash "$root/scripts/hw-lldb.sh" --sessions 3 "$elf" > "$tmp/bridge" 2>&1 &
bridge=$!
wait $lldb
rc=$?
log=$root/$run.log
cat "$tmp/out" > "$log"
# The bridge ends after the third session. If LLDB stopped early, it would
# wait for the next one: stop the server, which ends it.
stuck=no
for i in $(seq 30); do kill -0 $bridge 2> /dev/null || break; sleep 1; done
if kill -0 $bridge 2> /dev/null; then
  stuck=yes
  taskkill //F //IM e2-server-gdb.exe > /dev/null 2>&1
  for i in $(seq 15); do kill -0 $bridge 2> /dev/null || break; sleep 1; done
  kill $bridge 2> /dev/null
  wait $bridge
fi
awk -v d="$tmp" 'BEGIN {f = d "/s1"} /^@@session[0-9]$/ {f = d "/s" substr($0, 10)} {print > f}' "$tmp/out"
touch "$tmp/s1" "$tmp/s2" "$tmp/s3"

status=0
# timeout(1): 124, or 137 after -k's KILL.
case $rc in
  124|137) echo "FAIL  LLDB timed out"; status=1 ;;
esac
[ $stuck = yes ] && { echo "FAIL  the bridge did not end (stopped it)"; status=1; }
# expect NAME PATTERN [SESSION]   (default: the first session)
expect() {
  if grep -q -E -- "$2" "$tmp/s${3:-1}"; then echo "ok    $1"
  else echo "FAIL  $1: no /$2/"; status=1; fi
}
expect "connected"                   'frame #0: 0x[0-9a-f]+ lldb-demo.elf`_rx_reset'
expect "backtrace: scale"            'frame #0: 0x[0-9a-f]+ lldb-demo.elf`lldb_demo::scale\(p=\(x = 5, y = -2\), k=3\) at main\.rs:27'
expect "backtrace: middle"           'frame #1: 0x[0-9a-f]+ lldb-demo.elf`lldb_demo::middle\(n=5\) at main\.rs:34'
expect "backtrace: outer"            'frame #2: 0x[0-9a-f]+ lldb-demo.elf`lldb_demo::outer\(a=4, b=1\) at main\.rs:40'
expect "backtrace: main"             'frame #3: 0x[0-9a-f]+ lldb-demo.elf`main at main\.rs:46'
expect "backtrace: the reset vector" 'frame #5: 0x[0-9a-f]+ lldb-demo.elf`_rx_reset'
expect "struct argument"             '\(lldb_demo::Point\) p = \(x = 5, y = -2\)'
if [ $python = yes ]; then
  expect "formatter: Option"         '\(core::option::Option<u32>\) some = Some\(7\)'
  expect "formatter: enum"           '\(lldb_demo::types::Shape\) rect = Rect\{w:4, h:9\}'
  expect "formatter: &str"           '\(&str\) name = "rx-rust"'
  expect "formatter: slice"          '\(&\[u16\]\) slice = size=2'
else
  echo "skip  formatters and ^C: LLDB has no Python"
fi
expect "interrupt: handler"          'frame #0: 0x[0-9a-f]+ lldb-demo.elf`lldb_demo::user::trap at user\.rs'
expect "interrupt: the entry"        'frame #1: 0x[0-9a-f]+ lldb-demo.elf`_rx_isr_common \+ [0-9]+'
expect "interrupt: interrupted code" 'frame #2: 0x[0-9a-f]+ lldb-demo.elf`_lldb_user_loop \+ [0-9]+'
expect "interrupt: PSW.U = 1"        'U:1|U = 1'
usp=$(grep -o -E '^ +usp = 0x[0-9a-f]+' "$tmp/out" | head -1 | awk '{print $3}')
expect "interrupt: SP = USP"         "^ +r0 = ${usp:-none} "
expect "step into int #64"           'stop reason = instruction step into'
expect "step into int #64: handler"  'frame #0: 0x[0-9a-f]+ lldb-demo.elf`_rx_vector_64$'
expect "step into int #64: caller"   'frame #1: 0x[0-9a-f]+ lldb-demo.elf`_lldb_user_loop \+ 14'
[ $python = yes ] &&
  expect "^C"                        'stop reason = signal SIGINT'
# Watchpoints (#80): two hits on TRAPS (written by the trap handler), and
# one on `finished`, set at a breakpoint after a reset (where the
# AP-RX72M-0A once never fired).
n=$(grep -c -E 'stop reason = watchpoint 1$' "$tmp/s1")
if [ "$n" -ge 2 ]; then echo "ok    watchpoint: hit twice"
else echo "FAIL  watchpoint: $n hits on TRAPS, not 2"; status=1; fi
# AtomicU32: {v:{value:{0:2}}} with the formatters, else over lines with
# `(__0 = 2)`.
expect "watchpoint: the second hit"  '^new value: \{v:\{value:\{0:2\}\}\}$|^ +\(__0 = 2\)$' 1
# One watchpoint at a time: the server's stop does not say which one hit.
expect "watchpoint: one slot"        '^response: num:1;$' 1
expect "watchpoint: second refused"  '^response: E01$' 1
expect "watchpoint: in the handler"  'frame #[0-9]: 0x[0-9a-f]+ lldb-demo.elf`lldb_demo::user::trap' 1
expect "watchpoint: finished"        'new value: 1$' 2
expect "watchpoint: at the write"    'stop reason = watchpoint 2' 2
n=$(grep -c '^loading .* again$' "$tmp/bridge")
if [ "$n" = 2 ]; then echo "ok    again: loaded after kill and after the touch"
else echo "FAIL  again: the bridge loaded the ELF $n times, not 2"; status=1; fi
expect "again: the reset vector"     'frame #0: 0x[0-9a-f]+ lldb-demo.elf`_rx_reset' 2
# (The formatters, if loaded in session 1, are still loaded.)
expect "again: breakpoint"           'frame #0: 0x[0-9a-f]+ lldb-demo.elf`lldb_demo::scale\(p=(\(x = 5, y = -2\)|\{x:5, y:-2\}), k=3\) at main\.rs:27' 2
expect "again: detached"             'Process [0-9]+ detached' 2
expect "after detach: stopped"       'stop reason = signal SIGINT' 3
# Somewhere in the loop: the loop, the vector's entry, rx-rt's interrupt
# entry or the handler. (A stop inside _rx_isr_common does not unwind yet:
# LLDB's frame-0 plan there comes from instruction emulation, which knows
# neither the interrupt's entry state nor dpushm / dpopm.)
expect "after detach: in the loop"   'frame #0: 0x[0-9a-f]+ lldb-demo.elf`(_lldb_user_loop|_rx_vector_64|_rx_isr_common|lldb_demo::user::trap|core::sync::atomic)' 3
if grep -q -E 'Assertion|Stack dump|PLEASE submit' "$tmp/out"; then
  echo "FAIL  LLDB crashed"; status=1
else
  echo "ok    LLDB did not crash"
fi
if [ $status != 0 ]; then
  echo "   (bridge:)"; cat "$tmp/bridge"
  echo "   (LLDB output in ${log#$root/}:)"; cat "$log"
fi
exit $status
