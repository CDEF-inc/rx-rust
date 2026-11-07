#!/bin/bash
# Builds the tests for the RX72M (AP-RX72M-0A board: RXv3 with the FPU and
# the DFPU) into build/hw, for scripts/hw-run.sh to run on the board. With
# HW_BOARD=rx74n they are for the RX74N (EK-RX74N: RXv3e), in build/hw-rx74n:
# the groups that run in RAM (insns, codegen, rust), with rxv3e-insns and
# without cmt, whose registers are elsewhere there.
#
#   hw-build.sh [insns] [rom] [codegen] [rust] [rt] [examples]
#                                                   default: all six
#
#   insns    test/qemu instruction tests, in RAM       -> build/hw/insns
#   rom      rxv2-insns, in the code flash (it moves   -> build/hw/rom
#            EXTB, which the RAM start-up uses)
#   codegen  test/codegen (llc -mcpu=rxv3, -O0 / -O2), in RAM -> build/hw/codegen
#   rust     test/rust/core-smoke (target-cpu=rxv3,  -> build/hw/rust
#            release / dev), in RAM
#   rt       rx-rt applications, test/rust/isr-save,    -> build/hw/rt
#            isr-bank, hw-irq, hw-pac, hw-hal, hw-uart, alloc-smoke, float-smoke (also with
#            +flush-denormals: float-smoke-fd) (target-cpu=rxv3, release / dev), in the code
#            flash with test/hw/rx72m-memory.x, booted from the reset vector
#   examples examples/* (blinky, serial-echo; release), in the code  -> build/hw/examples
#            flash. They run until stopped; hw-run.sh runs one for
#            HW_TIMEOUT seconds and leaves it running (not part of its
#            default list)
#
# Each NAME.elf gets a NAME.sym with the symbols the runner needs. The
# start-up code, harness and linker scripts are in test/hw. RX_LD picks the
# linker (default build/llvm's ld.lld, and rustc's rust-lld; e.g.
# RX_LD=gnu/install/bin/rx-elf-ld), also for the Rust applications
# (-C linker).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
MC=$root/build/llvm/bin/llvm-mc
T=$root/test/hw
board=${HW_BOARD:-rx72m}
case $board in
  rx72m)
    out=$root/build/hw cpu=rxv3 v3e=0 ram_ld=rx72m-ram.ld
    all=(insns rom codegen rust rt examples)
    insns=(cmt) extra_defs=() ;;
  rx74n)
    out=$root/build/hw-rx74n cpu=rxv3e v3e=1 ram_ld=rx74n-ram.ld
    all=(insns codegen rust)
    insns=(rxv3e-insns) extra_defs=(--defsym ROOT_ONLY=1) ;;
  *)
    echo "unknown HW_BOARD $board (rx72m, rx74n)" >&2
    exit 2 ;;
esac
groups=("$@"); [ ${#groups[@]} -gt 0 ] || groups=("${all[@]}")
GCCFLAGS="-mcpu=rx600 -m64bit-doubles"
libgcc=$("$G/rx-elf-gcc" $GCCFLAGS -print-libgcc-file-name)
LD=${RX_LD:-$root/build/llvm/bin/ld.lld}
# GNU ld's warnings about the RWX segment and the stack note (lld has
# neither the warnings nor the options).
case $LD in
  *lld*) nowarn= ;;
  *) nowarn="--no-warn-rwx-segments --no-warn-execstack" ;;
esac
linker_flag=${RX_LD:+-C linker=$(realpath "$RX_LD")}
host=x86_64-unknown-linux-gnu
s1=$root/rust/build/$host/stage1

# cargo rebuilds only when `rustc -vV` changes, and the stage1 rustc says
# 1.101.0-dev whatever it was built from (#88). So the Rust target
# directories are keyed on the compiler itself (librustc_driver, which has
# the LLVM backend, and rust-lld), and those of older compilers are removed.
rustc_id=$(stat -c '%n %s %Y' "$s1"/lib/librustc_driver-*.so \
             "$s1/lib/rustlib/$host/bin/rust-lld" 2>/dev/null | md5sum | cut -c1-8)
# target_dir BASE: BASE@<compiler>, removing BASE@<other compilers> (and
# BASE itself, from before the key)
target_dir() {
  local old
  rm -rf "$1"
  for old in "$1"@*; do
    [ -e "$old" ] && [ "$old" != "$1@$rustc_id" ] && rm -rf "$old"
  done
  echo "$1@$rustc_id"
}
# Both boards have the RXv2 and RXv3 instructions, the FPU and the DFPU.
defs=(--defsym V2=1 --defsym V3=1 --defsym V3E=$v3e --defsym FPU=1 --defsym DFPU=1
      "${extra_defs[@]}")

# sym ELF: write the runner's symbols next to the ELF
sym() {
  "$G/rx-elf-nm" "$1" | grep -E ' _(start|rx_reset|done|checks|failures|first_failure|got_value|want_value|exit_code|finished|faillog|loopback)$' > "${1%.elf}.sym"
}

# insn_tests DIR START LDSCRIPT NAME...: instruction tests from test/qemu
insn_tests() {
  local d=$1 start=$2 ld=$3; shift 3
  mkdir -p "$d"
  "$MC" -triple=rx -mcpu=rxv3e -filetype=obj "$T/$start" -o "$d/start.o"
  "$MC" -triple=rx -mcpu=rxv3e -filetype=obj -I "$root/test/qemu" "${defs[@]}" \
    "$T/harness.s" -o "$d/harness.o"
  for name in "$@"; do
    "$MC" -triple=rx -mcpu=rxv3e -filetype=obj -g -I "$root/test/qemu" "${defs[@]}" \
      "$root/test/qemu/$name.s" -o "$d/$name.o"
    "$LD" $nowarn -T "$T/$ld" \
      "$d/start.o" "$d/harness.o" "$d/$name.o" -o "$d/$name.elf"
    sym "$d/$name.elf"
    echo "built $d/$name.elf"
  done
}

for g in "${groups[@]}"; do
  case $g in
    insns)
      insn_tests "$out/insns" start-ram.s "$ram_ld" cpu-features fcmp-flags \
        fpu-status rxv2-fpu dsp-v2 dfpu-insns rxv3-insns mem-insns memex-insns         "${insns[@]}" ;;
    rom)
      insn_tests "$out/rom" start-rom.s rx72m-rom.ld rxv2-insns ;;
    codegen)
      d=$out/codegen; mkdir -p "$d"
      "$MC" -triple=rx -mcpu=rxv3e -filetype=obj "$T/start-ram.s" -o "$d/start.o"
      C=$root/test/codegen
      for t in "i32 main" "m3 m3_main" "struct struct_main" "atomic atomic_main" \
               "fpu fpu_main" "dfpu dfpu_main" "mem mem_main" \
               "mulh mulh_main" "frame frame_main" "jt jt_main" \
               "fp fp_main" "va va_main" "fpu_ieee fpu_ieee_main" \
               "fpu_ieee fpu_ieee_main -fd +flush-denormals"; do
        # TEST DRIVER [SUFFIX ATTRS]
        set -- $t
        n=$1${3:-}
        "$G/rx-elf-gcc" $GCCFLAGS -O2 -c "$C/$2.c" -o "$d/$2.o"
        for o in 0 2; do
          "$root/build/llvm/bin/llc" -mtriple=rx-none-elf -mcpu=$cpu \
            -mattr=+64bit-doubles${4:+,$4} -O$o -filetype=obj "$C/$1.ll" -o "$d/$n-O$o.o"
          "$LD" $nowarn -T "$T/$ram_ld" \
            -o "$d/$n-O$o.elf" "$d/start.o" "$d/$2.o" "$d/$n-O$o.o" "$libgcc"
          sym "$d/$n-O$o.elf"
          echo "built $d/$n-O$o.elf"
        done
      done ;;
    rust)
      d=$out/rust; mkdir -p "$d"
      crate=$root/test/rust/core-smoke
      tdir=$(target_dir "$out/rust-target")
      "$MC" -triple=rx -mcpu=rxv3e -filetype=obj "$T/start-ram.s" -o "$d/start.o"
      "$G/rx-elf-gcc" $GCCFLAGS -O2 -c "$crate/main.c" -o "$d/main.o"
      for profile in release dev; do
        (cd "$crate" &&
         RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
         RUSTFLAGS="-C target-cpu=$cpu" PATH=$G:$PATH \
         "$root/rust/build/$host/stage0/bin/cargo" build -q --profile "$profile" \
           --target rx-none-elf -Zbuild-std=core \
           -Zbuild-std-features=compiler-builtins-mem --target-dir "$tdir")
        lib=$(ls "$tdir/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")"/lib*.a | head -1)
        "$LD" $nowarn --gc-sections -T "$T/$ram_ld" \
          -o "$d/core-smoke-$profile.elf" "$d/start.o" "$d/main.o" "$lib" "$libgcc"
        sym "$d/core-smoke-$profile.elf"
        echo "built $d/core-smoke-$profile.elf"
      done ;;
    rt)
      d=$out/rt; mkdir -p "$d"
      # NAME or NAME:SUFFIX:RUSTFLAGS (float-smoke also with the FPU / DFPU;
      # some also with linker relaxation)
      for spec in isr-save isr-bank hw-irq hw-pac hw-hal hw-uart alloc-smoke float-smoke \
                  "float-smoke:-fd:-C target-feature=+flush-denormals" \
                  "isr-save:-relax:-C target-feature=+relax" \
                  "isr-bank:-relax:-C target-feature=+relax" \
                  "hw-irq:-relax:-C target-feature=+relax" \
                  "alloc-smoke:-relax:-C target-feature=+relax" \
                  "float-smoke:-fd-relax:-C target-feature=+flush-denormals,+relax"; do
        IFS=: read -r crate suffix extra <<< "$spec"
        tdir=$(target_dir "$root/build/hw/rt-target$suffix")
        for profile in release dev; do
          # RUSTFLAGS replaces the crate's rustflags (isr-save's are for QEMU's
          # rx62n), so it gives the linker script too.
          (cd "$root/test/rust/$crate" &&
           RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
           RUSTFLAGS="-C link-arg=-Tlink.x -C target-cpu=rxv3 $extra $linker_flag" \
           RX_MEMORY_X=$T/rx72m-memory.x PATH=$G:$PATH \
           "$root/rust/build/$host/stage0/bin/cargo" build -q --profile "$profile" \
             --target-dir "$tdir")
          cp "$tdir/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")/$crate" \
            "$d/$crate$suffix-$profile.elf"
          sym "$d/$crate$suffix-$profile.elf"
          echo "built $d/$crate$suffix-$profile.elf"
        done
      done ;;
    examples)
      d=$out/examples; mkdir -p "$d"
      tdir=$(target_dir "$root/build/examples")
      for crate in blinky serial-echo; do
        # The crate's .cargo/config.toml has the target, its rustflags and
        # build-std.
        (cd "$root/examples/$crate" &&
         RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
         PATH=$G:$PATH \
         "$root/rust/build/$host/stage0/bin/cargo" build -q --release \
           --target-dir "$tdir")
        cp "$tdir/rx-none-elf/release/$crate" "$d/$crate.elf"
        sym "$d/$crate.elf"
        echo "built $d/$crate.elf"
      done ;;
    *)
      echo "unknown group $g (insns, rom, codegen, rust, rt, examples)" >&2
      exit 2 ;;
  esac
done
