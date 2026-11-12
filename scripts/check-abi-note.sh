#!/bin/bash
# Checks the calling-convention note and lld's check of it
# (CDEF-inc/llvm-project#46): every object of this LLVM says whether it
# depends on CC-RX's calling convention (the default), on GNURX's
# (-target-abi=gnu / -mabi=gnu / "llvm-abiname": "gnu"), or on neither, and
# ld.lld refuses to link objects that depend on different ones, or a CC-RX
# one with an object of an older LLVM (no note, a clang or rustc .comment).
# Objects without the note otherwise (GCC's) are not checked.
#
# test/abi-note: after_stack.ll defines, and caller.ll calls, a function
# whose int follows a structure that is always on the stack (R1 for CC-RX,
# R3 for GNURX); neutral.ll has only arguments both place alike; lib.rs is
# after_stack in Rust (no_core, so that it needs no RX std).
#
#   check-abi-note.sh
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
host=x86_64-unknown-linux-gnu
B=$root/build/llvm/bin
G=$root/gnu/install/bin
RUSTC=$root/rust/build/$host/stage1/bin/rustc
T=$root/test/abi-note
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"

LLC="$B/llc -mtriple=rx-none-elf -O2 -filetype=obj"
$LLC "$T/after_stack.ll" -o after_ccrx.o
$LLC -target-abi=gnu "$T/after_stack.ll" -o after_gnu.o
$LLC "$T/caller.ll" -o caller_ccrx.o
$LLC -target-abi=gnu "$T/caller.ll" -o caller_gnu.o
$LLC -target-abi=gnu "$T/neutral.ll" -o neutral_gnu.o
RUSTC_BOOTSTRAP=1 "$RUSTC" --target rx-none-elf --crate-type lib --emit obj \
  -C opt-level=2 -A internal_features "$T/lib.rs" -o rust.o
# An object of an older rustc: no note, and its .comment.
"$B/llvm-objcopy" --remove-section=.note.rx.abi rust.o old_rust.o
"$G/rx-elf-gcc" -m64bit-doubles -O2 -c -x c - -o gcc.o <<'EOF'
struct s6 { char c[6]; };
int after_stack(struct s6 s, int x) { return s.c[0] + x; }
EOF

status=0
# note OBJ KIND: the object's note says KIND (1 CC-RX, 2 GNURX, 3 neither)
note() {
  local got
  got=$("$B/llvm-readelf" -n "$1" | awk '/description data:/ {print $3; exit}')
  if [ "$got" = "0$2" ]; then echo "ok    $1: ABI note $2"
  else echo "FAIL  $1: ABI note '$got', want $2"; status=1; fi
}
# link NAME ok|error|warning [PATTERN] OBJ...: ld.lld links them, or
# refuses / warns with PATTERN.
link() {
  local name=$1 want=$2 pat= out rc=0
  shift 2
  [ "$want" = ok ] || { pat=$1; shift; }
  out=$("$B/ld.lld" -e 0 "$@" -o "$tmp/out.elf" 2>&1) || rc=$?
  case $want in
    ok) [ $rc = 0 ] && [ -z "$out" ] ;;
    error) [ $rc != 0 ] && echo "$out" | grep -q -- "error: .*$pat" ;;
    warning) [ $rc = 0 ] && echo "$out" | grep -q -- "warning: .*$pat" ;;
  esac && echo "ok    $name" || { echo "FAIL  $name ($want)"; echo "$out" | sed 's/^/      /'; status=1; }
}

note after_ccrx.o 1
note after_gnu.o 2
note caller_gnu.o 2
note neutral_gnu.o 3
note rust.o 1
link "same convention links" ok caller_ccrx.o after_ccrx.o
link "CC-RX + GNURX is refused" error "uses CC-RX's calling convention" caller_ccrx.o after_gnu.o
link "neutral GNURX object + CC-RX links" ok neutral_gnu.o after_ccrx.o
link "Rust + CC-RX caller links" ok caller_ccrx.o rust.o
link "Rust + GNURX caller is refused" error "uses CC-RX's calling convention" caller_gnu.o rust.o
link "old rustc object + CC-RX is refused" error "was built by an older LLVM" caller_ccrx.o old_rust.o
link "old rustc object + GNURX links" ok caller_gnu.o old_rust.o
link "-z rx-abi-check=warning" warning "was built by an older LLVM" -z rx-abi-check=warning caller_ccrx.o old_rust.o
link "GCC object is not checked" ok caller_ccrx.o gcc.o
exit $status
