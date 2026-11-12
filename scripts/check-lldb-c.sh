#!/bin/bash
# Debugs C code built by GCC (-g) together with Rust, with LLDB on QEMU:
# test/rust/core-smoke's main.c (GCC) and its Rust staticlib (dev), linked
# like run-rust-test.sh does. Checks the backtraces across the languages
# (C -> Rust -> C), GCC's arguments and locals (structs among them), and
# `finish` on C functions (C's ABI) and on a Rust one called from C.
#
#   check-lldb-c.sh        OPT=0|2 (GCC's -O, default 0); DEBUG=0 builds the
#                          C code without -g; LLDB_TIMEOUT seconds for the
#                          LLDB session (120)
#
# DEBUG=0 leaves the C functions without CFI (and without DWARF): LLDB has
# to unwind them by emulating their prologues (EmulateInstructionRX). It
# checks the backtrace from Rust through C's main to _start, and from inside
# a C function past its prologue.
#
# Two known gaps (docs/NOTES-lldb.md):
# - -O0: a breakpoint on a function written on one line stops before its
#   prologue, where GCC's locations of the arguments are not valid yet
#   (c_double's x);
# - -O2: GCC describes a struct argument passed on the stack as
#   DW_OP_breg7 (its argument pointer, r7, which is eliminated in the code),
#   so LLDB reads garbage (c_struct_mix's a).
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
host=x86_64-unknown-linux-gnu
opt=${OPT:-0}
debug=${DEBUG:-1}
G=$root/gnu/install/bin
LLDB=$root/build/lldb/bin/lldb
[ -x "$LLDB" ] || { echo "no $LLDB; run scripts/build-lldb.sh" >&2; exit 2; }
if [ -z "${QEMU:-}" ]; then
  QEMU=$root/../qemu/build/qemu-system-rx
  [ -x "$QEMU" ] || QEMU=qemu-system-rx
fi
crate=$root/test/rust/core-smoke
target_dir=$root/build/lldb-c
(cd "$crate" && RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
  RUSTFLAGS="-C target-cpu=rxv1" "$root/rust/build/$host/stage0/bin/cargo" build -q \
  --profile dev --target rx-none-elf -Zbuild-std=core \
  -Zbuild-std-features=compiler-builtins-mem --target-dir "$target_dir") || exit 1

tmp=$(mktemp -d)
trap 'kill $qemu 2>/dev/null; rm -rf "$tmp"' EXIT
GCCFLAGS="-mcpu=rx600 -m64bit-doubles"
libgcc=$("$G/rx-elf-gcc" $GCCFLAGS -print-libgcc-file-name)
name=core-smoke-O$opt$([ "$debug" = 1 ] || echo -nodebug)
elf=$target_dir/$name.elf
"$G/rx-elf-as" -o "$tmp/start.o" "$root/scripts/rx-start.s"
"$G/rx-elf-gcc" $GCCFLAGS $([ "$debug" = 1 ] && echo -g) -O"$opt" -c "$crate/main.c" \
  -o "$tmp/main.o" || exit 1
"$root/build/llvm/bin/ld.lld" -T "$root/scripts/rx-qemu.ld" --gc-sections -o "$elf" \
  "$tmp/start.o" "$tmp/main.o" "$target_dir/rx-none-elf/debug/librx_core_smoke.a" \
  "$libgcc" || exit 1

# run-rust-test.sh's way: the ELF is loaded into the SDRAM and runs from
# its entry (_start); the ROM is empty.
head -c 16 /dev/zero > "$tmp/bios.bin"
port=$((20000 + $$ % 20000))
"$QEMU" -M gdbsim-r5f562n8 -bios "$tmp/bios.bin" -display none -serial none \
  -monitor none -device loader,file="$elf",cpu-num=0 -S -gdb tcp:127.0.0.1:$port &
qemu=$!
for _ in $(seq 50); do
  ss -ltn "sport = :$port" | grep -q LISTEN && break
  sleep 0.2
done

# main.c runs its checks for k = 0, 1, 300, ...: stop in the k = 300 round.
if [ "$debug" = 1 ]; then
  cmds=(
    "breakpoint set -n rs_s3 -c 'y == 300'" "continue" "bt" "frame variable" "finish"
    "breakpoint set -n rs_call_c -c 'k == 300'" "continue" "bt"
    "breakpoint set -n c_struct_mix" "continue" "bt" "frame variable" "finish"
    "up" "frame variable k"
    "breakpoint set -n c_double" "continue" "frame variable" "finish"
  )
else
  # Without DWARF the C functions have only their symbols (_ prefixed). A
  # breakpoint on one stops at its entry; two steps take it past
  # `push.l r7; sub #4, r0`.
  cmds=(
    "breakpoint set -n rs_s3 -c 'y == 300'" "continue" "bt" "breakpoint delete -f"
    "breakpoint set -n rs_call_c -c 'k == 300'" "continue"
    "breakpoint set -n _c_struct_mix" "continue" "si" "si" "bt" "finish" "bt"
  )
fi
args=()
for c in "${cmds[@]}" kill; do args+=(-o "$c"); done
lldb_timeout=${LLDB_TIMEOUT:-120}
timeout -k 10 "$lldb_timeout" "$LLDB" -b -x -o "settings set use-color false" \
  -o "target create $elf" -o "gdb-remote 127.0.0.1:$port" \
  "${args[@]}" > "$tmp/out" 2>&1
lldb_status=$?
log=$target_dir/check-lldb-c-${name#core-smoke-}.log
cat "$tmp/out" > "$log"

status=0
if [ $lldb_status = 124 ] || [ $lldb_status = 137 ]; then
  echo "FAIL  LLDB did not finish in $lldb_timeout s"; status=1
else
  echo "ok    LLDB finished"
fi
expect() {
  if grep -q -E -- "$2" "$tmp/out"; then echo "ok    $1"
  else echo "FAIL  $1: no /$2/"; status=1; fi
}
f="frame #[0-9]+: 0x[0-9a-f]+ $name\\.elf\`"
if [ "$debug" != 1 ]; then
  # From Rust (CFI) into C's main (none) and on to _start.
  expect "no CFI: Rust's C caller"     "#1: 0x[0-9a-f]+ $name\\.elf\`_main \\+ [0-9]+\$"
  expect "no CFI: past the C caller"   "#2: 0x[0-9a-f]+ $name\\.elf\`_start \\+ 10\$"
  # From a C function past its prologue: Rust, C, _start.
  expect "no CFI: inside a C function" "#0: 0x[0-9a-f]+ $name\\.elf\`_c_struct_mix \\+ 4\$"
  expect "no CFI: its Rust caller"     "#1: 0x[0-9a-f]+ $name\\.elf\`rs_call_c\\(k=300\\) at lib\\.rs:87"
  expect "no CFI: then C's main"       "#2: 0x[0-9a-f]+ $name\\.elf\`_main \\+ [0-9]+\$"
  expect "no CFI: then _start"         "#3: 0x[0-9a-f]+ $name\\.elf\`_start \\+ 10\$"
  if grep -q 'frame #4' "$tmp/out"; then
    echo "FAIL  no CFI: the backtrace goes on after _start"; status=1
  else
    echo "ok    no CFI: the backtrace ends at _start"
  fi
  expect "no CFI: finish"              "stop reason = step out"
  expect "no CFI: finish returns"      "\\* frame #0: 0x[0-9a-f]+ $name\\.elf\`rs_call_c\\(k=300\\) at lib\\.rs:87"
fi
if [ "$debug" = 1 ]; then
# Rust called from C: its arguments, and `finish` in the C caller.
expect "Rust from C: backtrace"       "#0: 0x[0-9a-f]+ [^ ]*\`rs_s3\\(y=300, s=.*\\) at lib\\.rs:64"
expect "Rust from C: the C caller"    "${f}main at main\\.c:74"
expect "Rust from C: argument"        '\(int\) y = 300'
expect "Rust from C: struct argument" '\(rx_core_smoke::S3\) s = \(c = ",\\t\\U00000001"\)'
expect "Rust from C: finish"          'Return value: \(int\) \$[0-9]+ = 600'
# C called from Rust called from C.
expect "backtrace: c_struct_mix"      "${f}c_struct_mix\\(b=\\(a = 300, b = 3\\), c=300, a=.*\\) at main\\.c:(32|33)"
expect "backtrace: rs_call_c"         "${f}rs_call_c\\(k=300\\) at lib\\.rs:87"
expect "backtrace: main"              "#2: 0x[0-9a-f]+ [^ ]*\`main at main\\.c:85"
expect "backtrace: _start"            "#3: 0x[0-9a-f]+ [^ ]*\`_start \\+ 10"
if grep -q 'frame #4' "$tmp/out"; then
  echo "FAIL  backtrace: goes on after _start"; status=1
else
  echo "ok    backtrace: ends at _start"
fi
expect "C struct argument in registers" '\(s8\) b = \(a = 300, b = 3\)'
if [ "$opt" = 0 ]; then
  expect "C struct argument on the stack" '\(s3\) a = \(c = ",\\a\\U00000001"\)'
elif grep -q -F '(s3) a = (c = ",\a\U00000001")' "$tmp/out"; then
  echo "note  c_struct_mix's a is right at -O2 now: update this script and NOTES-lldb.md"
fi
expect "finish: C int"                'Return value: \(int\) \$[0-9]+ = 301111'
expect "up: C local"                  '\(int\) k = 300'
[ "$opt" = 0 ] || expect "C double argument" '\(double\) x = 300'
expect "finish: C double"             'Return value: \(double\) \$[0-9]+ = 150'
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
