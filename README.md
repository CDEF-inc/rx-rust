# rx-rust

A Rust toolchain for Renesas RX microcontrollers: an LLVM backend for RX and
an `rx-none-elf` target for rustc, compatible with the GCC (`rx-elf`) ABI.

Status: experimental. Rust programs (`core`, `-Zbuild-std`) build, link with
GNU ld and run on QEMU's RX62N board; real hardware has not been tried yet.

| Component | Where | State |
|---|---|---|
| LLVM RX backend (MC layer, CodeGen) | [CDEF-inc/llvm-project](https://github.com/CDEF-inc/llvm-project/tree/rx/23.1) `rx/23.1` | RXv1 instructions; object files match GNU as |
| rustc `rx-none-elf` target, callconv, `asm!` | [CDEF-inc/rust](https://github.com/CDEF-inc/rust/tree/rx-none-elf) `rx-none-elf` | `core` builds; tier 3 |
| Startup code, vector tables, linker script | [`crates/rx-rt`](crates/rx-rt) | boots from the reset vector on QEMU |
| Build and test scripts, tests | [`scripts`](scripts), [`test`](test) | |

What works:

- i8/i16/i32/i64 integer code, `f32`/`f64` (soft-float, GCC `-m64bit-doubles`
  compatible), structs by value and `sret`, calls in both directions between
  Rust/LLVM and GCC-compiled C;
- 32-bit atomic loads and stores (no compare-and-swap), inline assembly,
  interrupts;
- the assembler for the RXv1 and system-control instructions, the
  single-precision FPU, the DSP (accumulator) instructions and the RXv2
  additions.

Not yet: RXv2/RXv3 instructions in code generation, the FPU/DFPU in code
generation, the DFPU and RXv3 instructions in the assembler, `alloc`/`std`,
debug info checks, real hardware.

## Layout

```
rust/           submodule: CDEF-inc/rust (rx-none-elf), which has
                src/llvm-project -> CDEF-inc/llvm-project (rx/23.1)
crates/rx-rt/   runtime: reset handler, vector tables, link.x
scripts/        build and test scripts
test/mc/        assembler tests, compared with GNU as
test/codegen/   LLVM IR + C tests run on QEMU (Rust/LLVM <-> GCC calls)
test/rust/      Rust tests: core-smoke (with a C driver), qemu-demo (Rust only)
docs/           design notes and plan (in Japanese)
```

## Building

Tested on Ubuntu (WSL2) on x86-64. Needs the usual LLVM / rustc build tools
(cmake, ninja, a C++ compiler, python3, ccache) and `qemu-system-rx` for the
tests. Sources and a full build take about 15 GB of disk. The scripts keep
the build parallelism low, for machines with about 8 GB of memory.

```sh
git clone https://github.com/CDEF-inc/rx-rust
cd rx-rust
scripts/setup.sh          # submodules, rust/bootstrap.toml
scripts/build-gnu.sh      # binutils 2.43 + GCC 14.2 for rx-elf into gnu/install
scripts/build-llvm.sh     # configure LLVM (X86 + RX) in build/llvm, build llc/llvm-mc
scripts/build-llvm-libs.sh
scripts/build-rustc.sh    # stage1 rustc using build/llvm
```

## Testing

```sh
scripts/mc-diff.sh lines test/mc/basic.s     # also scc.s, system.s, fpu.s, dsp.s, v2.s
scripts/mc-diff.sh file test/mc/branch.s     # also symbol.s
scripts/run-codegen-test.sh test/codegen/i32.ll test/codegen/main.c
scripts/run-codegen-test.sh test/codegen/m3.ll test/codegen/m3_main.c
scripts/run-codegen-test.sh test/codegen/struct.ll test/codegen/struct_main.c
scripts/run-rust-test.sh                     # test/rust/core-smoke
scripts/run-rust-rom.sh                      # test/rust/qemu-demo, boots a ROM image
```

## Writing a program

See [`test/rust/qemu-demo`](test/rust/qemu-demo): a `#![no_main]` binary that
depends on `rx-rt`, provides `memory.x`, and builds with
`-Zbuild-std=core` and `-C link-arg=-Tlink.x`. The target and its ABI are
described in
[`src/doc/rustc/src/platform-support/rx-none-elf.md`](https://github.com/CDEF-inc/rust/blob/rx-none-elf/src/doc/rustc/src/platform-support/rx-none-elf.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. The LLVM fork is under the LLVM license (Apache-2.0 WITH
LLVM-exception) and the rust fork under the Rust project's licenses.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
