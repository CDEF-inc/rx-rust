#!/bin/bash
# Debugs test/rust/lldb-demo on QEMU's RX62N with LLDB (build/lldb, see
# scripts/build-lldb.sh) through QEMU's gdbstub, and checks what LLDB shows:
# a breakpoint by Rust function name, the backtrace through the Rust frames
# (from the compiler's .debug_frame), arguments and locals (a struct among
# them), expressions, `finish` with the return value (also of the
# aggregates in lldb-demo's rets.rs), `thread return`, registers,
# disassembly, stepping, a watchpoint and a global variable. Expressions
# that call a function cannot be run on RX (no JIT for it in the target):
# LLDB has to say so, not crash (#79). The release build (opt-level "s")
# gets a shorter session: what it checks is what optimization leaves, such
# as an inlined frame and arguments that are no longer available.
#
#   check-lldb.sh          PROFILE=dev|release (default dev); RX_CPU picks
#                          the core (scripts/cpu.sh; default RXv1 code on
#                          QEMU's rx62n); RUST_EXTRA_FLAGS adds rustc flags
#                          (e.g. -C target-feature=+relax); both build in a
#                          target directory of their own; LLDB_TIMEOUT
#                          seconds for the LLDB session (120)
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
. "$root/scripts/cpu.sh"
crate=$root/test/rust/lldb-demo
flags="${RUST_CPU_FLAGS:-}${RUST_EXTRA_FLAGS:+ $RUST_EXTRA_FLAGS}"
target_dir=$root/build/lldb-demo${flags:+-$(echo "$flags" | md5sum | cut -c1-8)}
# RUSTFLAGS replaces the crate's rustflags (rxv1), so give them too.
export RUSTFLAGS=${flags:+-C link-arg=-Tlink.x ${RUST_CPU_FLAGS:--C target-cpu=rxv1} ${RUST_EXTRA_FLAGS:-}}
[ -n "$RUSTFLAGS" ] || unset RUSTFLAGS
(cd "$crate" && RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
  "$root/rust/build/$host/stage0/bin/cargo" build -q --profile "$profile" \
  --target-dir "$target_dir") || exit 1
elf=$target_dir/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")/lldb-demo

tmp=$(mktemp -d)
trap 'kill $qemu 2>/dev/null; rm -rf "$tmp"' EXIT
"$L/llvm-objcopy" -O binary "$elf" "$tmp/rom.bin"
port=$((20000 + $$ % 20000))
"$QEMU" -M gdbsim-r5f562n8 ${QEMU_CPU:+-cpu "$QEMU_CPU"} -bios "$tmp/rom.bin" -display none -serial null \
  -monitor none -S -gdb tcp:127.0.0.1:$port &
qemu=$!
# Wait for the gdbstub (up to 10 s: the machine may be busy with other tests).
for _ in $(seq 50); do
  ss -ltn "sport = :$port" | grep -q LISTEN && break
  sleep 0.2
done

# The session takes seconds. A stop that never comes would make LLDB wait
# forever (once in CI, rx-rust#126, until the job's 60 minutes ran out and
# no log was kept), so it gets a time limit and the output so far is
# checked and shown.
lldb_timeout=${LLDB_TIMEOUT:-120}
# An LLDB with Python (build-lldb.sh LLDB_PYTHON=1) also shows types.rs's
# locals through rustc's formatters (src/etc, what rust-lldb loads).
formatters=()
if "$LLDB" -b -x -o 'script print("python" + "-ok")' 2>&1 | grep -q '^python-ok'; then
  python=1
  formatters=("command script import $root/rust/src/etc/lldb_lookup.py"
              "frame variable some none ok err circle rect name slice point_ref tuple")
fi
if [ "$profile" = release ]; then
  # opt-level "s": the session stops only where the code is still there
  # (some rets.rs functions are folded into their caller).
  cmds=(
    "breakpoint set -n scale" "continue" "bt" "frame variable"
    "finish" "bt" "frame variable"
    "watchpoint set variable finished" "continue"
    "target variable result finished"
    "breakpoint set -n ret_s2" "breakpoint set -n ret_u64" "breakpoint set -n ret_f64"
    "continue" "finish" "continue" "finish" "continue" "finish"
  )
else
  cmds=(
    "breakpoint set -n scale" "continue"
    "bt" "frame variable" "expr k + 1" "expr p.x * 2"
    "register read pc r0 psw"
    "disassemble -c 3 --pc" "finish" "frame variable"
    "next" "frame variable" "up" "frame variable"
    "watchpoint set variable finished" "continue"
    "target variable result finished"
    "breakpoint set -n ret_s2" "breakpoint set -n ret_s3"
    "breakpoint set -n ret_pair16" "breakpoint set -n ret_reordered"
    "breakpoint set -n ret_pair_u8_u16" "breakpoint set -n ret_s8"
    "breakpoint set -n ret_s12" "breakpoint set -n ret_s20"
    "breakpoint set -n ret_option" "breakpoint set -n ret_u64"
    "breakpoint set -n ret_f64"
    "continue" "finish" "continue" "finish" "continue" "finish"
    "continue" "finish" "continue" "finish"
    "continue" "finish" "continue" "finish"
    "continue" "thread return 7" "register read r1"
    "continue" "finish" "continue" "finish" "continue" "finish"
    "breakpoint set -n seen" "continue" "up" "frame variable"
    'frame variable some.$variants$.$variant$1.value.__0'
    "frame variable -Z 2 slice.data_ptr" "frame variable *point_ref"
    "frame variable *add_k._ref__k"
    "${formatters[@]}"
    "breakpoint set -n trap" "continue" "bt" "register read usp"
    "frame select 2" "register read psw r0" "frame select 0"
    "expr lldb_demo::outer(1, 2)"
  )
fi
args=()
for c in "${cmds[@]}" kill; do args+=(-o "$c"); done
timeout -k 10 "$lldb_timeout" "$LLDB" -b -x -o "settings set use-color false" \
  -o "target create $elf" -o "gdb-remote 127.0.0.1:$port" \
  "${args[@]}" > "$tmp/out" 2>&1
lldb_status=$?
log=$target_dir/check-lldb$([ "$profile" = dev ] || echo "-$profile").log
cat "$tmp/out" > "$log"

status=0
# 124: timeout stopped it (137: killed after the grace period).
if [ $lldb_status = 124 ] || [ $lldb_status = 137 ]; then
  echo "FAIL  LLDB did not finish in $lldb_timeout s"; status=1
else
  echo "ok    LLDB finished"
fi
expect() {
  if grep -q -E -- "$2" "$tmp/out"; then echo "ok    $1"
  else echo "FAIL  $1: no /$2/"; status=1; fi
}
no() {
  if grep -q -E -- "$2" "$tmp/out"; then echo "FAIL  $1: /$2/"; status=1
  else echo "ok    $1"; fi
}
# LLDB has no Rust plugin: it shows the types with their C names.
expect "stopped at the breakpoint"   'stop reason = breakpoint 1\.1'
expect "backtrace: scale"            'frame #0: 0x[0-9a-f]+ lldb-demo`lldb_demo::scale\(p=\(x = 5, y = -2\), k=3\) at main\.rs:27'
expect "backtrace: middle"           'frame #1: 0x[0-9a-f]+ lldb-demo`lldb_demo::middle\(n=5\) at main\.rs:34'
if [ "$profile" = release ]; then
  # a and b are dead once a + b is computed; the DWARF says so.
  expect "backtrace: outer"          'frame #2: 0x[0-9a-f]+ lldb-demo`lldb_demo::outer\(a=<unavailable>, b=<unavailable>\) at main\.rs:40'
else
  expect "backtrace: outer"          'frame #2: 0x[0-9a-f]+ lldb-demo`lldb_demo::outer\(a=4, b=1\) at main\.rs:40'
fi
expect "backtrace: main"             'frame #3: 0x[0-9a-f]+ lldb-demo`main at main\.rs:46'
expect "backtrace: rx-rt"            'frame #4: 0x[0-9a-f]+ lldb-demo`rx_rt::start'
expect "backtrace: the reset vector" 'frame #5: 0x[0-9a-f]+ lldb-demo`_rx_reset'
no "backtrace: ends at _rx_reset"    'frame #6'
expect "struct argument"             '\(lldb_demo::Point\) p = \(x = 5, y = -2\)'
expect "finish: return value"        'Return value: \(int\) \$[0-9]+ = 9'
expect "watchpoint"                  'stop reason = watchpoint 1'
expect "global variable"             '\(unsigned int\) result = 28'
expect "return: 2-byte struct"       "\\(lldb_demo::rets::S2\\) \\\$[0-9]+ = \\(a = 'A', b = 'B'\\)"
expect "return: u64"                 '\(unsigned long long\) \$[0-9]+ = 8589934597'
expect "return: f64"                 '\(double\) \$[0-9]+ = 3\.5'
if [ "$profile" = release ]; then
  # `finish` returns into middle, where wrapping_add is inlined: LLDB
  # shows it as a frame of its own, with its arguments.
  expect "inlined frame"             'frame #0: 0x[0-9a-f]+ lldb-demo`<u32>::wrapping_add\(self=9, rhs=5\) at uint_macros\.rs:[0-9]+:[0-9]+ \[inlined\]'
  expect "inlined frame: caller"     'frame #1: 0x[0-9a-f]+ lldb-demo`lldb_demo::middle\(n=5\) at main\.rs:35'
  expect "inlined frame: variables"  '\(unsigned int\) self = 9'
fi
if [ "$profile" != release ]; then
# One space or two before the symbol, depending on the LLDB version.
expect "registers"                   'pc = 0xfff8[0-9a-f]+ +lldb-demo`lldb_demo::scale'
expect "disassembly"                 '^-> +0xfff8[0-9a-f]+ <\+[0-9]+>: '
expect "next: the local"             '\(int\) s = 9'
expect "up: outer's arguments"       '\(unsigned int\) a = 4'
# Rust's ABI: pairs of scalars in R1 / R2, small aggregates in R1:R2.
expect "return: 3-byte struct"       "\\(lldb_demo::rets::S3\\) \\\$[0-9]+ = \\(a = 'a', b = 'b', c = 'c'\\)"
expect "return: (u16, u16)"          '\(\(u16, u16\)\) \$[0-9]+ = \(__0 = 16, __1 = 4660\)'
expect "return: reordered fields"    "\\(lldb_demo::rets::Reordered\\) \\\$[0-9]+ = \\(a = 'r', b = 305419896\\)"
expect "return: (u8, u16)"           "\\(\\(u8, u16\\)\\) \\\$[0-9]+ = \\(__0 = 'p', __1 = 17185\\)"
expect "return: 8-byte struct"       '\(lldb_demo::rets::S8\) \$[0-9]+ = \(a = 8, b = -1\)'
expect "return: Option<u32>"         'value = \(__0 = 42\)'
no "return: none for a 12-byte struct (in memory)" 'Return value: \(lldb_demo::rets::S12\)'
expect "thread return"               'r1 = 0x00000007'
expect "expression"                  '\(int\) \$[0-9]+ = 4'
expect "expression on a struct field" '\(int\) \$[0-9]+ = 10'
expect "a call: a clean error"       "error: Can't evaluate the expression"
# types.rs (#83): what the C type system makes of Rust's types. Enums
# with data show every variant and the discriminant; `frame variable`
# reaches into them (`expr` does not: clang asserts, on any target).
expect "C-like enum"                 '\(lldb_demo::types::Color\) color = Green'
expect "Option<u32>"                 '\(core::option::Option<u32>\) some = \{'
expect "Option<u32>: the Some value" '\(unsigned int\) some\.\$variants\$\.\$variant\$1\.value\.__0 = 7'
expect "Result<u32, i8>"             '\(core::result::Result<u32, i8>\) err = \{'
expect "enum with data: a variant"   'value = \(w = .\\x04., h = .\\t.\)'
expect "&str"                        '\(&str\) name = \(data_ptr = "rx-rust", length = 7\)'
expect "slice"                       '\(&\[u16\]\) slice = \{'
expect "slice: elements"             '\(unsigned short \*\) slice\.data_ptr = 0x[0-9a-f]+ \{'
expect "slice: an element"           '\[1\] = 30'
expect "u8 array"                    '\(unsigned char\[4\]\) bytes = "RX72"'
expect "reference"                   '\(lldb_demo::Point \*\) point_ref = 0x[0-9a-f]+'
expect "reference: the value"        '\(lldb_demo::Point\) \*point_ref = \(x = 11, y = -12\)'
expect "generic struct"              '\(lldb_demo::types::Wrapper<short>\) wrapped = \(inner = -5, tag = .\\x02.\)'
expect "char"                        '\(char32_t\) ch = U\+03BB'
expect "bool"                        '\(bool\) flag = true'
expect "tuple"                       '\(\(u8, i32\)\) tuple = \(__0 = .\\x01., __1 = -1\)'
expect "closure"                     '\(lldb_demo::types::run::\{closure_env#0\}\) add_k = \{'
expect "closure: a capture"          '\(unsigned int\) \*add_k\._ref__k = 3'
if [ -n "${python:-}" ]; then
  expect "formatter: Some"           '\(core::option::Option<u32>\) some = Some\(7\)'
  expect "formatter: None"           '\(core::option::Option<u32>\) none = None'
  expect "formatter: Ok"             '\(core::result::Result<u32, i8>\) ok = Ok\(100\)'
  expect "formatter: Err"            '\(core::result::Result<u32, i8>\) err = Err\(-3\)'
  expect "formatter: tuple variant"  '\(lldb_demo::types::Shape\) circle = Circle\(300\)'
  expect "formatter: struct variant" '\(lldb_demo::types::Shape\) rect = Rect\{w:4, h:9\}'
  expect "formatter: &str"           '\(&str\) name = "rx-rust"'
  expect "formatter: slice"          '\(&\[u16\]\) slice = size=2'
  expect "formatter: reference"      '\(lldb_demo::Point \*\) point_ref = 0x[0-9a-f]+ \{x:11, y:-12\}'
  expect "formatter: tuple"          '\(\(u8, i32\)\) tuple = \(1, -1\)'
else
  echo "skip  formatters: LLDB has no Python (build-lldb.sh LLDB_PYTHON=1)"
fi
# user.rs (#85): from the handler through _rx_isr_common into code that
# ran in user mode. Its SP is USP (saved by the interrupt entry), not the
# CFA, and its PSW has U = 1.
expect "interrupt: handler"          'frame #0: 0x[0-9a-f]+ lldb-demo`lldb_demo::user::trap at user\.rs'
expect "interrupt: the entry"        'frame #1: 0x[0-9a-f]+ lldb-demo`_rx_isr_common \+ [0-9]+'
expect "interrupt: interrupted code" 'frame #2: 0x[0-9a-f]+ lldb-demo`_lldb_user_loop \+ [0-9]+'
# rustc's formatters, when loaded, also print the fields as `U:1`.
expect "interrupt: PSW.U = 1"        '= \(IPL = 0, PM = 0, U = 1,|PM:0, :0, U:1,'
usp=$(grep -o -E '^ +usp = 0x[0-9a-f]+' "$tmp/out" | head -1 | awk '{print $3}')
expect "interrupt: SP = USP"         "^ +r0 = ${usp:-none} "
fi
# build-lldb.sh links libxml2, so LLDB reads QEMU's target.xml (#82).
if grep -q 'does not have XML parsing enabled' "$tmp/out"; then
  echo "FAIL  target.xml: LLDB was built without libxml2"; status=1
else
  echo "ok    target.xml: read"
fi
if grep -q -E 'Assertion|Stack dump|PLEASE submit' "$tmp/out"; then
  echo "FAIL  LLDB crashed"; status=1
else
  echo "ok    LLDB did not crash"
fi
if [ $status != 0 ]; then
  echo "   (LLDB output in ${log#$root/}:)"
  cat "$log"
fi
exit $status
