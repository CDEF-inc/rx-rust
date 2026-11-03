#!/bin/bash
# Debugs test/rust/lldb-demo on QEMU's RX62N with LLDB (build/lldb, see
# scripts/build-lldb.sh) through QEMU's gdbstub, and checks what LLDB shows:
# a breakpoint by Rust function name, the backtrace through the Rust frames
# (from the compiler's .debug_frame), arguments and locals (a struct among
# them), `finish` with the return value, registers, disassembly, stepping,
# a watchpoint and a global variable.
#
#   check-lldb.sh          PROFILE=dev|release (default dev)
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
host=x86_64-unknown-linux-gnu
profile=${PROFILE:-dev}
LLDB=$root/build/lldb/bin/lldb
L=$root/build/llvm/bin
[ -x "$LLDB" ] || { echo "no $LLDB; run scripts/build-lldb.sh" >&2; exit 2; }
if [ -z "${QEMU:-}" ]; then
  QEMU=$root/../qemu/build/qemu-system-rx
  [ -x "$QEMU" ] || QEMU=qemu-system-rx
fi
crate=$root/test/rust/lldb-demo
target_dir=$root/build/lldb-demo
(cd "$crate" && RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
  "$root/rust/build/$host/stage0/bin/cargo" build -q --profile "$profile" \
  --target-dir "$target_dir") || exit 1
elf=$target_dir/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")/lldb-demo

tmp=$(mktemp -d)
trap 'kill $qemu 2>/dev/null; rm -rf "$tmp"' EXIT
"$L/llvm-objcopy" -O binary "$elf" "$tmp/rom.bin"
port=$((20000 + $$ % 20000))
"$QEMU" -M gdbsim-r5f562n8 -bios "$tmp/rom.bin" -display none -serial null \
  -monitor none -S -gdb tcp:127.0.0.1:$port &
qemu=$!
sleep 1

"$LLDB" -b -x -o "settings set use-color false" \
  -o "target create $elf" -o "gdb-remote 127.0.0.1:$port" \
  -o "breakpoint set -n scale" -o "continue" \
  -o "bt" -o "frame variable" -o "register read pc r0 psw" \
  -o "disassemble -c 3 --pc" -o "finish" -o "frame variable" \
  -o "next" -o "frame variable" -o "up" -o "frame variable" \
  -o "watchpoint set variable finished" -o "continue" \
  -o "target variable result finished" -o "kill" > "$tmp/out" 2>&1
cat "$tmp/out" > "$root/build/check-lldb.log"

status=0
expect() {
  if grep -q -E -- "$2" "$tmp/out"; then echo "ok    $1"
  else echo "FAIL  $1: no /$2/"; status=1; fi
}
# LLDB has no Rust plugin: it shows the types with their C names.
expect "stopped at the breakpoint"   'stop reason = breakpoint 1\.1'
expect "backtrace: scale"            'frame #0: 0x[0-9a-f]+ lldb-demo`lldb_demo::scale\(p=\(x = 5, y = -2\), k=3\) at main\.rs:27'
expect "backtrace: middle"           'frame #1: 0x[0-9a-f]+ lldb-demo`lldb_demo::middle\(n=5\) at main\.rs:34'
expect "backtrace: outer"            'frame #2: 0x[0-9a-f]+ lldb-demo`lldb_demo::outer\(a=4, b=1\) at main\.rs:40'
expect "backtrace: main"             'frame #3: 0x[0-9a-f]+ lldb-demo`main at main\.rs:46'
expect "backtrace: rx-rt"            'frame #4: 0x[0-9a-f]+ lldb-demo`rx_rt::start'
expect "backtrace: the reset vector" 'frame #5: 0x[0-9a-f]+ lldb-demo`_rx_reset'
if grep -q 'frame #6' "$tmp/out"; then
  echo "FAIL  backtrace: goes on after _rx_reset"; status=1
else
  echo "ok    backtrace: ends at _rx_reset"
fi
expect "struct argument"             '\(lldb_demo::Point\) p = \(x = 5, y = -2\)'
expect "registers"                   'pc = 0xfff8[0-9a-f]+  lldb-demo`lldb_demo::scale'
expect "disassembly"                 '^-> +0xfff8[0-9a-f]+ <\+[0-9]+>: '
expect "finish: return value"        'Return value: \(int\) \$0 = 9'
expect "next: the local"             '\(int\) s = 9'
expect "up: outer's arguments"       '\(unsigned int\) a = 4'
expect "watchpoint"                  'stop reason = watchpoint 1'
expect "global variable"             '\(unsigned int\) result = 28'
[ $status = 0 ] || echo "   (LLDB output in build/check-lldb.log)"
exit $status
