#!/bin/bash
# Builds the tests for the RX72M (AP-RX72M-0A board: RXv3 with the FPU and
# the DFPU) into build/hw, for scripts/hw-run.sh to run on the board.
#
#   hw-build.sh [insns] [rom] [codegen] [rust] [rt]     default: all five
#
#   insns    test/qemu instruction tests, in RAM       -> build/hw/insns
#   rom      rxv2-insns, in the code flash (it moves   -> build/hw/rom
#            EXTB, which the RAM start-up uses)
#   codegen  test/codegen (llc -mcpu=rxv3, -O0 / -O2), in RAM -> build/hw/codegen
#   rust     test/rust/core-smoke (target-cpu=rxv3,  -> build/hw/rust
#            release / dev), in RAM
#   rt       rx-rt applications, test/rust/isr-save and -> build/hw/rt
#            test/rust/hw-irq (target-cpu=rxv3, release / dev), in the code
#            flash with test/hw/rx72m-memory.x, booted from the reset vector
#
# Each NAME.elf gets a NAME.sym with the symbols the runner needs. The
# start-up code, harness and linker scripts are in test/hw.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
G=$root/gnu/install/bin
MC=$root/build/llvm/bin/llvm-mc
T=$root/test/hw
out=$root/build/hw
groups=("$@"); [ ${#groups[@]} -gt 0 ] || groups=(insns rom codegen rust rt)
GCCFLAGS="-mcpu=rx600 -m64bit-doubles"
libgcc=$("$G/rx-elf-gcc" $GCCFLAGS -print-libgcc-file-name)
# The RX72M has the RXv2 and RXv3 instructions, the FPU and the DFPU.
defs=(--defsym V2=1 --defsym V3=1 --defsym V3E=0 --defsym FPU=1 --defsym DFPU=1)

# sym ELF: write the runner's symbols next to the ELF
sym() {
  "$G/rx-elf-nm" "$1" | grep -E ' _(start|rx_reset|done|checks|failures|first_failure|got_value|want_value|exit_code|finished|faillog)$' > "${1%.elf}.sym"
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
    "$G/rx-elf-ld" --no-warn-rwx-segments -T "$T/$ld" \
      "$d/start.o" "$d/harness.o" "$d/$name.o" -o "$d/$name.elf"
    sym "$d/$name.elf"
    echo "built $d/$name.elf"
  done
}

for g in "${groups[@]}"; do
  case $g in
    insns)
      insn_tests "$out/insns" start-ram.s rx72m-ram.ld cpu-features fcmp-flags \
        fpu-status rxv2-fpu dsp-v2 dfpu-insns rxv3-insns mem-insns cmt memex-insns ;;
    rom)
      insn_tests "$out/rom" start-rom.s rx72m-rom.ld rxv2-insns ;;
    codegen)
      d=$out/codegen; mkdir -p "$d"
      "$MC" -triple=rx -mcpu=rxv3e -filetype=obj "$T/start-ram.s" -o "$d/start.o"
      C=$root/test/codegen
      for t in "i32 main" "m3 m3_main" "struct struct_main" "atomic atomic_main" \
               "fpu fpu_main" "dfpu dfpu_main" "mem mem_main"; do
        set -- $t
        "$G/rx-elf-gcc" $GCCFLAGS -O2 -c "$C/$2.c" -o "$d/$2.o"
        for o in 0 2; do
          "$root/build/llvm/bin/llc" -mtriple=rx-none-elf -mcpu=rxv3 \
            -mattr=+64bit-doubles -O$o -filetype=obj "$C/$1.ll" -o "$d/$1-O$o.o"
          "$G/rx-elf-ld" --no-warn-rwx-segments --no-warn-execstack -T "$T/rx72m-ram.ld" \
            -o "$d/$1-O$o.elf" "$d/start.o" "$d/$2.o" "$d/$1-O$o.o" "$libgcc"
          sym "$d/$1-O$o.elf"
          echo "built $d/$1-O$o.elf"
        done
      done ;;
    rust)
      d=$out/rust; mkdir -p "$d"
      host=x86_64-unknown-linux-gnu
      crate=$root/test/rust/core-smoke
      target_dir=$root/build/hw/rust-target
      "$MC" -triple=rx -mcpu=rxv3e -filetype=obj "$T/start-ram.s" -o "$d/start.o"
      "$G/rx-elf-gcc" $GCCFLAGS -O2 -c "$crate/main.c" -o "$d/main.o"
      for profile in release dev; do
        (cd "$crate" &&
         RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
         RUSTFLAGS="-C target-cpu=rxv3" PATH=$G:$PATH \
         "$root/rust/build/$host/stage0/bin/cargo" build -q --profile "$profile" \
           --target rx-none-elf -Zbuild-std=core \
           -Zbuild-std-features=compiler-builtins-mem --target-dir "$target_dir")
        lib=$(ls "$target_dir/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")"/lib*.a | head -1)
        "$G/rx-elf-ld" --no-warn-rwx-segments --no-warn-execstack --gc-sections -T "$T/rx72m-ram.ld" \
          -o "$d/core-smoke-$profile.elf" "$d/start.o" "$d/main.o" "$lib" "$libgcc"
        sym "$d/core-smoke-$profile.elf"
        echo "built $d/core-smoke-$profile.elf"
      done ;;
    rt)
      d=$out/rt; mkdir -p "$d"
      host=x86_64-unknown-linux-gnu
      for crate in isr-save hw-irq; do
        for profile in release dev; do
          # RUSTFLAGS replaces the crate's rustflags (isr-save's are for QEMU's
          # rx62n), so it gives the linker script too.
          (cd "$root/test/rust/$crate" &&
           RUSTC=$root/rust/build/$host/stage1/bin/rustc RUSTC_BOOTSTRAP=1 \
           RUSTFLAGS="-C link-arg=-Tlink.x -C target-cpu=rxv3" \
           RX_MEMORY_X=$T/rx72m-memory.x PATH=$G:$PATH \
           "$root/rust/build/$host/stage0/bin/cargo" build -q --profile "$profile" \
             --target-dir "$root/build/hw/rt-target")
          cp "$root/build/hw/rt-target/rx-none-elf/$([ "$profile" = dev ] && echo debug || echo "$profile")/$crate" \
            "$d/$crate-$profile.elf"
          sym "$d/$crate-$profile.elf"
          echo "built $d/$crate-$profile.elf"
        done
      done ;;
    *)
      echo "unknown group $g (insns, rom, codegen, rust, rt)" >&2
      exit 2 ;;
  esac
done
