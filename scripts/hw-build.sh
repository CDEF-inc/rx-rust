#!/bin/bash
# Builds the tests for the RX72M (AP-RX72M-0A board: RXv3 with the FPU and
# the DFPU) into build/hw, for scripts/hw-run.sh to run on the board. With
# HW_BOARD=rx74n they are for the RX74N (EK-RX74N: RXv3e), in build/hw-rx74n:
# insns with rxv3e-insns and without cmt, whose registers are elsewhere
# there, rom, codegen, rust, the rt tests isr-save, isr-bank, alloc-smoke,
# float-smoke, hw-pac, hw-hal, hw-clock (480 and 600 MHz), hw-spi, hw-pwm,
# hw-adc and hw-i2c, and serial-echo.
# HW_BOARD=rx74m builds the same: the RX74M (EK-RX74M) has every RX74N
# register at the same address and runs these tests as they are.
# The board tests (test/rust/hw-* but hw-irq, and serial-echo) pick the
# board with a feature (board_feature below: ap-rx72m or ek-rx74n; see
# test/rust/hw-board).
#
#   hw-build.sh [insns] [rom] [codegen] [rust] [rt] [examples] [lldb] [coretests]
#                                                   default: all seven
#
#   insns    test/qemu instruction tests, in RAM       -> build/hw/insns
#   rom      rxv2-insns, in the code flash (it moves   -> build/hw/rom
#            EXTB, which the RAM start-up uses)
#   codegen  test/codegen (llc -mcpu=rxv3, -O0 / -O2), in RAM -> build/hw/codegen
#   rust     test/rust/core-smoke (target-cpu=rxv3,  -> build/hw/rust
#            release / dev), in RAM
#   rt       rx-rt applications, test/rust/isr-save,    -> build/hw/rt
#            isr-bank, hw-irq, hw-pac, hw-hal, hw-uart, hw-spi, hw-adc, hw-pwm
#            (the board tests with the board's feature),
#            alloc-smoke, float-smoke (also with +flush-denormals: float-smoke-fd)
#            (target-cpu=rxv3, release / dev), in the code flash with
#            test/hw/rx72m-memory.x, booted from the reset vector
#   examples examples/* (blinky, serial-echo with the board's feature;  -> build/hw/examples
#            release, target-cpu and memory.x as for rt), in the code
#            flash. They run until stopped; hw-run.sh runs one for
#            HW_TIMEOUT seconds and leaves it running (not part of its
#            default list)
#   lldb     test/rust/lldb-demo (dev), in the code flash -> build/hw/lldb
#            (MRAM), for scripts/hw-lldb-check.sh (not hw-run.sh's)
#   coretests  library/coretests (RX74N only; not in the default list) -> build/hw-rx74n/coretests
#            in two parts that fit in the 1 MB code MRAM: coretests-num.elf
#            and coretests-rest.elf (test/rust/coretests, rx74n feature).
#            Run them with HW_TIMEOUT=900 hw-run.sh build/hw-rx74n/coretests/*.elf
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
[ "$board" = rx74m ] && board=rx74n
case $board in
  rx72m)
    out=$root/build/hw cpu=rxv3 v3e=0 ram_ld=rx72m-ram.ld rom_ld=rx72m-rom.ld
    memory_x=rx72m-memory.x
    board_tests=(hw-pac hw-hal hw-uart hw-spi hw-adc hw-pwm)
    peripherals=(hw-irq "${board_tests[@]}"
                 "hw-irq:-relax:-C target-feature=+relax")
    all=(insns rom codegen rust rt examples lldb)
    insns=(cmt) extra_defs=() board_feature=ap-rx72m
    examples=(blinky serial-echo) ;;
  rx74n)
    out=$root/build/hw-rx74n cpu=rxv3e v3e=1 ram_ld=rx74n-ram.ld rom_ld=rx74n-rom.ld
    memory_x=rx74n-memory.x
    board_tests=(hw-pac hw-hal hw-clock hw-spi hw-pwm hw-adc hw-i2c)
    peripherals=("${board_tests[@]}" "hw-clock:-600::clock600")
    all=(insns rom codegen rust rt examples lldb)
    insns=(rxv3e-insns) extra_defs=(--defsym ROOT_ONLY=1)
    # Also on the EK-RX74M, like the other tests (one RX74N PAC build).
    board_feature=ek-rx74n
    examples=(serial-echo) ;;
  *)
    echo "unknown HW_BOARD $board (rx72m, rx74n, rx74m)" >&2
    exit 2 ;;
esac
# The crates that take the board's feature: the board tests and serial-echo.
board_crates=("${board_tests[@]}" serial-echo)
# features CRATE [MORE]: the cargo features for CRATE (comma-separated)
features() {
  local f=${2:-}
  [[ " ${board_crates[*]} " = *" $1 "* ]] && f=$board_feature${f:+,$f}
  echo "$f"
}
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

# cargo rebuilds only when `rustc -vV` changes, and that names only the rust
# commit (#88): a rebuild with a changed LLVM or uncommitted changes says the
# same. So the Rust target
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
  "$G/rx-elf-nm" "$1" | grep -E ' _(start|rx_reset|done|checks|failures|first_failure|got_value|want_value|exit_code|finished|faillog|loopback|eeprom|coretests_log|coretests_log_len)$' > "${1%.elf}.sym"
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
      insn_tests "$out/rom" start-rom.s "$rom_ld" rxv2-insns ;;
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
      # NAME or NAME:SUFFIX:RUSTFLAGS[:FEATURES] (float-smoke also with the
      # FPU / DFPU; some also with linker relaxation; hw-clock at 600 MHz)
      for spec in isr-save isr-bank "${peripherals[@]}" alloc-smoke float-smoke \
                  "float-smoke:-fd:-C target-feature=+flush-denormals" \
                  "isr-save:-relax:-C target-feature=+relax" \
                  "isr-bank:-relax:-C target-feature=+relax" \
                  "alloc-smoke:-relax:-C target-feature=+relax" \
                  "float-smoke:-fd-relax:-C target-feature=+flush-denormals,+relax"; do
        IFS=: read -r crate suffix extra more <<< "$spec"
        feats=$(features "$crate" "$more")
        # Other RUSTFLAGS rebuild everything (the PACs too): a target
        # directory of their own. Features only rebuild the crate.
        tdir=$(target_dir "$out/rt-target${extra:+$suffix}")
        for profile in release dev; do
          # RUSTFLAGS replaces the crate's rustflags (isr-save's are for QEMU's
          # rx62n), so it gives the linker script too.
          (cd "$root/test/rust/$crate" &&
           RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
           RUSTFLAGS="-C link-arg=-Tlink.x -C target-cpu=$cpu $extra $linker_flag" \
           RX_MEMORY_X=$T/$memory_x PATH=$G:$PATH \
           "$root/rust/build/$host/stage0/bin/cargo" build -q --profile "$profile" \
             ${feats:+--features "$feats"} --target-dir "$tdir")
          cp "$tdir/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")/$crate" \
            "$d/$crate$suffix-$profile.elf"
          sym "$d/$crate$suffix-$profile.elf"
          echo "built $d/$crate$suffix-$profile.elf"
        done
      done ;;
    examples)
      d=$out/examples; mkdir -p "$d"
      tdir=$(target_dir "$out/examples-target")
      for crate in "${examples[@]}"; do
        feats=$(features "$crate")
        # The crate's .cargo/config.toml has the target and build-std;
        # RUSTFLAGS replaces its rustflags, for this board's target-cpu.
        (cd "$root/examples/$crate" &&
         RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
         RUSTFLAGS="-C link-arg=-Tlink.x -C target-cpu=$cpu $linker_flag" \
         RX_MEMORY_X=$T/$memory_x PATH=$G:$PATH \
         "$root/rust/build/$host/stage0/bin/cargo" build -q --release \
           ${feats:+--features "$feats"} --target-dir "$tdir")
        cp "$tdir/rx-none-elf/release/$crate" "$d/$crate.elf"
        sym "$d/$crate.elf"
        echo "built $d/$crate.elf"
      done ;;
    lldb)
      d=$out/lldb; mkdir -p "$d"
      tdir=$(target_dir "$out/lldb-target")
      # RUSTFLAGS replaces the crate's rustflags (for QEMU's rx62n).
      (cd "$root/test/rust/lldb-demo" &&
       RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
       RUSTFLAGS="-C link-arg=-Tlink.x -C target-cpu=$cpu $linker_flag" \
       RX_MEMORY_X=$T/$memory_x PATH=$G:$PATH \
       "$root/rust/build/$host/stage0/bin/cargo" build -q --target-dir "$tdir")
      cp "$tdir/rx-none-elf/debug/lldb-demo" "$d/lldb-demo.elf"
      echo "built $d/lldb-demo.elf" ;;
    coretests)
      # library/coretests (test/rust/coretests with the rx74n feature), in
      # two parts that each fit in the code MRAM: `num` and the rest.
      [ "$board" = rx74n ] || { echo "coretests: RX74N only" >&2; exit 2; }
      d=$out/coretests; mkdir -p "$d"
      for part in num:+num rest:-num; do
        name=${part%%:*}
        src=$out/coretests-src-$name
        bash "$root/scripts/coretests-part.sh" "${part#*:}" "$src"
        tdir=$(target_dir "$out/coretests-target-$name")
        exe=$(cd "$src" &&
          RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
          RUSTFLAGS="-C link-arg=-Tcoretests-rx74n.x -C target-cpu=$cpu $linker_flag" \
          PATH=$G:$PATH \
          "$root/rust/build/$host/stage0/bin/cargo" test -q --no-run --release \
            --features rx74n --target-dir "$tdir" \
            --message-format=json-render-diagnostics 2> "$d/coretests-$name.build.log" |
          sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -1)
        [ -n "$exe" ] || { grep -E '^error' -A12 "$d/coretests-$name.build.log" | head -60
                           exit 1; }
        cp "$exe" "$d/coretests-$name.elf"
        sym "$d/coretests-$name.elf"
        echo "built $d/coretests-$name.elf"
      done ;;
    *)
      echo "unknown group $g (insns, rom, codegen, rust, rt, examples, lldb, coretests)" >&2
      exit 2 ;;
  esac
done
