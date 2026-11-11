# rx-rust

A Rust toolchain for Renesas RX microcontrollers: an LLVM backend for RX and
an `rx-none-elf` target for rustc, compatible with the GCC (`rx-elf`) ABI.

Status: experimental. Rust programs (`core`, `alloc`, `-Zbuild-std`) build,
link with lld (the default) or GNU ld and run on QEMU (RXv1, RXv2, RXv3 and
RXv3e cores of CDEF-inc/qemu) and on three boards. The instruction, codegen
and Rust tests pass on all of them (see [`docs/NOTES-hw.md`](docs/NOTES-hw.md)):

| Board | MCU | Core |
|---|---|---|
| Alpha Project AP-RX72M-0A | RX72M (R5F572MNDDFC) | RXv3, single- and double-precision FPU |
| Renesas EK-RX74N | RX74N (R5K574NCB4AE) | RXv3e |
| Renesas EK-RX74M | RX74M (R5K574MCB4AE), the RX74N plus EtherCAT | RXv3e |

Each MCU family has a PAC and an embedded-hal 1.0 HAL with clocks, GPIO, a
delay, a UART, SPI, PWM and an ADC; the RX74N HAL also has I2C.

| Component | Where | State |
|---|---|---|
| LLVM RX backend (MC layer, CodeGen) | [CDEF-inc/llvm-project](https://github.com/CDEF-inc/llvm-project/tree/rx/23.1) `rx/23.1` | RXv1 / RXv2 / RXv3 / RXv3e instructions, FPU, DSP, DFPU; object files match GNU as (and Renesas CC-RX where GNU as is wrong) |
| lld RX target | same, `lld/ELF/Arch/RX.cpp` | links like GNU ld (same relocations, e_flags and layout); `rx-none-elf`'s default linker (`rust-lld`), GNU ld with `-C linker=rx-elf-ld` (see [`docs/NOTES-lld.md`](docs/NOTES-lld.md)) |
| clang for RX | same, `clang/lib/{Basic,CodeGen}/Targets/RX.cpp`, `clang/lib/Driver/ToolChains/RX.cpp` | compiles C for `--target=rx-none-elf` with GCC's (GNURX's) ABI, types and macros; links with ld.lld or GNU ld against an rx-elf GCC's libgcc and newlib; its objects call GCC's both ways on QEMU (see [`docs/NOTES-clang.md`](docs/NOTES-clang.md)) |
| LLDB RX support | same, `lldb/source/Plugins/ABI/RX` (and a clang `RXTargetInfo` for its type system) | breakpoints, backtraces (from the compiler's CFI), variables, stepping on QEMU's gdbstub and, through a bridge to e2-server-gdb, on the RX72M; see [`docs/NOTES-lldb.md`](docs/NOTES-lldb.md) |
| rustc `rx-none-elf` target, callconv, `asm!` | [CDEF-inc/rust](https://github.com/CDEF-inc/rust/tree/rx-none-elf) `rx-none-elf` | `core` and `alloc` build; tier 3 |
| Startup code, vector tables, linker script | [`crates/rx-rt`](crates/rx-rt) | boots from the reset vector and takes interrupts (peripheral, software, nested, register banks) on QEMU and the boards; `critical-section` implementation |
| RX72M HAL (embedded-hal 1.0) | [`crates/rx72m-hal`](crates/rx72m-hal) | clocks (PLL 240 MHz), GPIO and pin functions, `DelayNs` (CMT2), SCI2 UART (`embedded-io`), SPI (RSPI2), ADC (S12AD: self-diagnosis, internal reference, temperature sensor), PWM (MTU6); checked on the AP-RX72M-0A (see [`docs/NOTES-hal.md`](docs/NOTES-hal.md)) |
| RX74N HAL (embedded-hal 1.0) | [`crates/rx74n-hal`](crates/rx74n-hal) | clocks (PLL 480 or 600 MHz), GPIO and pin functions, `DelayNs` (CMT2), RSCI12 UART (`embedded-io`), SPI (RSPIA0), ADC (ADC16H), PWM (GPTW8), I2C (RI3C0 in I2C mode; read from the EK-RX74M's EEPROM); checked on the EK-RX74N and the EK-RX74M |
| Peripheral access crates | [`crates/rx72m-pac`](crates/rx72m-pac), [`crates/rx74n-pac`](crates/rx74n-pac), [`crates/sfrx-pac`](crates/sfrx-pac) | generated at build time with svd2rust from e² studio's `RX72M.sfrx` / `RX74N.sfrx` (not redistributed); `sfrx-pac` is the shared generator (see [`docs/NOTES-hal.md`](docs/NOTES-hal.md)) |
| QEMU with RXv2 / RXv3 / RXv3e CPU models | [CDEF-inc/qemu](https://github.com/CDEF-inc/qemu/tree/rx/v11.1) `rx/v11.1` | RXv2 / RXv3 / RXv3e instructions, DFPU, Protect Zone (CPU side); instruction tests checked against the RX72M and the RX74N |
| Build and test scripts, tests | [`scripts`](scripts), [`test`](test) | QEMU and the boards |

What works:

- i8/i16/i32/i64 integer code, `f32`/`f64` (soft-float by default, GCC
  `-m64bit-doubles` compatible; with `-C target-feature=+flush-denormals`
  the FPU, and on rxv3 the DFPU for `f64`: see
  [Floating point](#floating-point)), structs by value and `sret`, calls in
  both directions between Rust/LLVM and GCC-compiled C;
- `alloc` with a `#[global_allocator]` of the application's
  (`test/rust/alloc-smoke`);
- atomics up to 32 bits, including compare-and-swap and `fetch_*`
  (`movli`/`movco` on RXv2+, interrupts masked on RXv1), inline assembly,
  interrupts (whose entries save FPSW, the DFPU registers the compiler uses
  and, with `save-acc`, the accumulators, depending on the target features;
  on RXv3 also entries that save in a register bank, `interrupt!(n, handler,
  bank = b)`);
- memory access with the RX addressing modes: post-increment and
  pre-decrement (`[Rn+]`, `[-Rn]`), register-indexed (`[Ri, Rb]`),
  immediate and memory-to-memory `mov`, and the string instructions
  (`smovf`, `sstr`) for `memcpy` / `memset`;
- the assembler for the RXv1 and system-control instructions, the
  single-precision FPU, the DSP (accumulator) instructions, the RXv2 and RXv3
  additions and the RXv3 double-precision FPU;
- linking with lld (the default) or GNU ld, and a prebuilt toolchain without
  GNU binutils (lld and the LLVM binary tools);
- debugging with LLDB, on QEMU and on the RX72M: breakpoints, backtraces,
  variables, stepping (`scripts/check-lldb.sh`); e² studio's GDB also reads
  the debug info;
- on the RXv3e (RX74N / RX74M), the Protect Zone instructions and the secure
  / non-secure address aliases (the programs run in the root world, at the
  secure addresses).

Not yet:
`std`, unwinding (panics abort), more peripheral drivers (I2C on the RX72M, DMA,
interrupt-driven and async drivers; #54), the EtherCAT and Δ-Σ peripherals
of the RX74M, and parts of LLDB (watchpoints and the DFPU registers on the
board, calling functions in expressions, Rust type names: #79-#87).

## Layout

```
rust/           submodule: CDEF-inc/rust (rx-none-elf), which has
                src/llvm-project -> CDEF-inc/llvm-project (rx/23.1)
crates/rx-rt/      runtime: reset handler, vector tables, link.x
crates/sfrx-pac/   PAC generator (sfrx -> SVD -> svd2rust) for the PACs' build.rs
crates/rx72m-pac/  RX72M PAC, generated at build time from e² studio's RX72M.sfrx
crates/rx72m-hal/  RX72M HAL: clocks, GPIO, delay, UART, SPI, ADC, PWM
crates/rx74n-pac/  RX74N PAC (also for the RX74M), from RX74N.sfrx
crates/rx74n-hal/  RX74N HAL: clocks, GPIO, delay, UART, SPI, ADC, PWM, I2C
examples/       board examples: blinky, serial-echo (AP-RX72M-0A), rx74n-serial-echo
scripts/        build and test scripts
test/mc/        assembler tests, compared with GNU as
test/codegen/   LLVM IR + C tests run on QEMU (Rust/LLVM <-> GCC calls)
test/qemu/      instruction tests for the QEMU CPU models (also run on the boards)
test/hw/        start-up code, harness and linker scripts for the boards
test/rust/      Rust tests: coretests (library/coretests on QEMU), core-smoke, variadic-smoke and
                cabi-smoke (with C drivers), qemu-demo, alloc-smoke,
                float-smoke, isr-save, isr-bank, lldb-demo, hw-irq (also on the board);
                board only: hw-pac, hw-hal, hw-uart, hw-spi, hw-adc, hw-pwm (RX72M) and
                hw-pac-rx74n, hw-hal-rx74n, hw-clock-rx74n, hw-spi-rx74n, hw-adc-rx74n,
                hw-pwm-rx74n, hw-i2c-rx74n (RX74N / RX74M)
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
scripts/build-llvm.sh     # configure LLVM (X86 + RX) and lld in build/llvm, build llc/llvm-mc/ld.lld
scripts/build-llvm-libs.sh
scripts/build-rustc.sh    # stage1 rustc using build/llvm
scripts/build-lldb.sh     # optional: clang + LLDB in build/lldb
```

### Prebuilt toolchain

For machines that do not build it (CI), `scripts/dist-toolchain.sh` packs
the built toolchain into `build/dist/rx-toolchain-x86_64-linux.tar.xz`:
the stage1 rustc with the library sources for `-Zbuild-std`, cargo, lld
(rustc's `rust-lld`, and `ld.lld`), the LLVM binary tools (`llvm-objcopy`,
`llvm-objdump`, `llvm-nm`, `llvm-size`, `llvm-readelf`, ...; also under the
`rx-elf-*` names) and `qemu-system-rx`, with the commits they were built
from in `VERSIONS`. It has no GNU binutils: Rust programs do not need them.
The tests in this repository still use GNU as, ld and GCC, to compare with
them and to call GCC-compiled C. The
[releases](https://github.com/CDEF-inc/rx-rust/releases) carry it: the CI
packs it on Ubuntu 22.04 (the `rx-toolchain-x86_64-linux` artifact of a run
on `main`), so it runs on x86-64 Linux with glibc 2.35 or later (Ubuntu
22.04, Debian 12 and later; `VERSIONS` says which). `qemu-system-rx`
brings its shared libraries (glib, pixman, ...) in `lib/`; build scripts
need a host C compiler. [`scripts/check-dist.sh`](scripts/check-dist.sh)
tests an archive on a machine (the CI does on Ubuntu 22.04 and Debian 12).

```sh
tar -xJf rx-toolchain-x86_64-linux.tar.xz
T=$PWD/rx-toolchain-x86_64-linux
RUSTC=$T/bin/rustc RUSTC_BOOTSTRAP=1 PATH=$T/bin:$PATH \
  $T/bin/cargo build --release   # a crate set up like test/rust/qemu-demo
```

For Windows, [`scripts/build-windows.sh`](scripts/build-windows.sh) builds
the same tools with Visual Studio's C++ tools (host
`x86_64-pc-windows-msvc`; `qemu-system-rx` with MSYS2's MinGW, and its DLLs
beside it) and `scripts/dist-toolchain-windows.sh` packs them into
`build/dist/rx-toolchain-x86_64-windows.zip`. It is the
`rx-toolchain-x86_64-windows` artifact of the Windows workflow. The programs
need nothing installed, but build scripts need the MSVC linker (Visual Studio
Build Tools), as with any MSVC Rust. In PowerShell:

```powershell
tar -xf rx-toolchain-x86_64-windows.zip
$T = "$PWD\rx-toolchain-x86_64-windows"
$env:RUSTC = "$T\bin\rustc.exe"; $env:RUSTC_BOOTSTRAP = 1; $env:PATH = "$T\bin;$env:PATH"
cargo build --release   # a crate set up like test/rust/qemu-demo
```

The tests use `../qemu/build/qemu-system-rx` (CDEF-inc/qemu, built as in
[`docs/NOTES-qemu.md`](docs/NOTES-qemu.md)) when it exists, else
`qemu-system-rx` from `PATH`.

## Testing

`scripts/run-all-tests.sh` runs everything below: the assembler and QEMU
instruction tests once, and the codegen and Rust tests on each QEMU CPU
model (`rx62n`, `rxv2`, `rxv3`, `rxv3e`; or the ones given as arguments).
The single tests take `RX_CPU=<model>` to build for and run on that core
(see [`scripts/cpu.sh`](scripts/cpu.sh)); without it they build RXv1 code
and run on `rx62n`.

The tests run in parallel, one per CPU by default (`-j N`; about 80 seconds
on 28 threads, against 33 minutes one at a time); tests that share a cargo
target directory, or check what another one built, run in order.
`--shard K/N` runs one of N parts, for CI. QEMU runs with `-icount` there
(`QEMU_FLAGS`), so that the timer tests do not depend on the machine's load.

The CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs them on
every pull request and on `main`, in four shards on GitHub's ubuntu-22.04
runners. It builds the toolchain with [`scripts/ci.sh`](scripts/ci.sh) in
four cached parts (LLVM + rustc, LLDB, GNU, QEMU at
[`scripts/qemu.commit`](scripts/qemu.commit)), each rebuilt only when its
commits or build scripts change: a pull request that does not move the
`rust` submodule goes straight to the tests. Board tests stay manual.

```sh
scripts/run-all-tests.sh                     # everything, on every CPU model
scripts/run-all-tests.sh -j 8 --shard 1/4    # the first quarter, 8 at a time
RX_CPU=rxv3 scripts/run-rust-rom.sh test/rust/isr-save
scripts/mc-diff.sh lines test/mc/basic.s     # also scc, system, fpu, dsp, v2, dfpu, v3, v3e, mov, memex
scripts/mc-diff.sh file test/mc/branch.s     # also symbol, string, sizeless
scripts/mc-dis.sh test/mc/basic.s            # disassembler round trip; any test/mc file
scripts/run-codegen-test.sh test/codegen/i32.ll test/codegen/main.c
scripts/run-codegen-test.sh test/codegen/m3.ll test/codegen/m3_main.c
scripts/run-codegen-test.sh test/codegen/struct.ll test/codegen/struct_main.c
scripts/run-codegen-test.sh test/codegen/atomic.ll test/codegen/atomic_main.c
ATTRS=+fpu scripts/run-codegen-test.sh test/codegen/fpu.ll test/codegen/fpu_main.c
scripts/check-dfpu.sh                        # DFPU code: instruction sequences
RX_CPU=rxv3 scripts/run-codegen-test.sh test/codegen/dfpu.ll test/codegen/dfpu_main.c
scripts/run-coretests.sh                     # library/coretests (the tests of core) on QEMU
scripts/run-rust-test.sh                     # test/rust/core-smoke
scripts/run-rust-rom.sh                      # test/rust/qemu-demo, boots a ROM image
scripts/run-rust-rom.sh test/rust/isr-save   # FPSW / ACC / A1 / DFPU saved by interrupt entries
RX_CPU=rxv3 scripts/run-rust-rom.sh test/rust/isr-bank   # register bank interrupt entries (save / rstr)
# CMT interrupts; hw-irq is built for rxv3 (the RX72M), so give the QEMU core
RX_CPU=rxv3 RX_MEMORY_X=$PWD/test/rust/qemu-demo/memory.x scripts/run-rust-rom.sh test/rust/hw-irq
scripts/run-qemu-test.sh fcmp-flags rx62n rxv2 rxv3 rxv3e
scripts/run-qemu-test.sh cpu-features rx62n rxv1 rxv2 rxv3 rxv3e
scripts/run-qemu-test.sh fpu-status rx62n rxv2 rxv3 rxv3e
scripts/run-qemu-test.sh rxv2-fpu rxv2 rxv3 rxv3e
scripts/run-qemu-test.sh rxv2-insns rxv2 rxv3 rxv3e
scripts/run-qemu-test.sh dsp-v1 rx62n rxv1         # generated by test/qemu/gen-dsp.py
scripts/run-qemu-test.sh dsp-v2 rxv2 rxv3 rxv3e
scripts/run-qemu-test.sh rxv3-insns rxv3 rxv3e
scripts/run-qemu-test.sh dfpu-insns rxv3 rxv3e
scripts/run-qemu-test.sh rxv3e-insns rxv3e
scripts/run-qemu-test.sh memex-insns rx62n rxv1 rxv2 rxv3 rxv3e
scripts/run-qemu-test.sh cmt rx62n rxv3             # compare match timer (a peripheral)
RX_CPU=rxv2 scripts/run-codegen-test.sh test/codegen/atomic.ll test/codegen/atomic_main.c
scripts/check-lld.sh                         # lld and GNU ld link the codegen tests alike
scripts/check-lldb.sh                        # LLDB on test/rust/lldb-demo, through QEMU's gdbstub
PROFILE=release RX_CPU=rxv3 scripts/check-lldb.sh   # optimized, RXv3 code
OPT=2 scripts/check-lldb-c.sh                # LLDB on core-smoke: GCC's C (-g) with Rust
# the tests link with lld (rustc's rust-lld, build/llvm's ld.lld); with GNU ld:
RX_LD=$PWD/gnu/install/bin/rx-elf-ld scripts/run-codegen-test.sh test/codegen/i32.ll test/codegen/main.c
RX_CPU=rx62n RUST_EXTRA_FLAGS="-C linker=$PWD/gnu/install/bin/rx-elf-ld" scripts/run-rust-rom.sh
```

### On the board

The board tests run through an E2 Lite and the GDB server of Renesas e²
studio, on Windows. They cover:

- the instruction tests (the parts the core has);
- the codegen tests (`llc -mcpu=rxv3` or `rxv3e`, -O0 and -O2);
- core-smoke;
- the rx-rt, PAC and HAL tests (the `rt` group), booted from the code flash
  or MRAM.

`HW_BOARD` picks the board: `rx72m` (the default), `rx74n` or `rx74m`. The
RX74M runs the RX74N builds (`build/hw-rx74n`).

| Board | Set-up |
|---|---|
| AP-RX72M-0A | JSW3 = SGL, JSW2 = SCI |
| EK-RX74N / EK-RX74M | the E2 Lite on J9, SW6 pin 1 ON and pin 2 OFF, power from J27's USB; e² studio needs the RX FSP packs for the RX74N / RX74M |

[`docs/NOTES-hw.md`](docs/NOTES-hw.md) has the setup, the server's quirks
and the last results.

```sh
scripts/hw-build.sh                    # in WSL: build the tests into build/hw
scripts/hw-run.sh                      # in Git Bash on Windows: run them on the board
HW_BOARD=rx74n scripts/hw-build.sh     # the RX74N tests, into build/hw-rx74n
HW_BOARD=rx74m scripts/hw-run.sh       # run them on an EK-RX74M
```

They link with lld; `RX_LD=$PWD/gnu/install/bin/rx-elf-ld scripts/hw-build.sh`
links them with GNU ld instead.

### Debugging with LLDB

LLDB (`scripts/build-lldb.sh`) debugs RX programs on QEMU's gdbstub
(`qemu-system-rx ... -S -gdb tcp:127.0.0.1:1234`, then `gdb-remote 1234`)
and on the board, where `scripts/hw-lldb.sh` starts e2-server-gdb and a
bridge that hides the server's quirks (non-stop mode only, its launch
sequence, big-endian `p`) from LLDB:

```sh
lldb ELF -o 'process connect --plugin gdb-remote listen://127.0.0.1:61235'  # in WSL
scripts/hw-lldb.sh ELF                                                     # in Git Bash
```

Watchpoints do not work on the board, and LLDB does not show the DFPU
registers there (see [`docs/NOTES-lldb.md`](docs/NOTES-lldb.md)).

### Example: blinky

[`examples/blinky`](examples/blinky) blinks the board's monitor LEDs LD1 /
LD2 (PJ0 / PJ1) at 2 Hz. The blinking is a function written against the
embedded-hal 1.0 traits only (`StatefulOutputPin`, `OutputPin`,
`DelayNs`); `main` hands it rx72m-hal's pins and CMT2 delay after setting
the PLL to 240 MHz. It is written to the code flash, so it keeps running
after the runner detaches, and after a reset or a power cycle.

```sh
scripts/hw-build.sh examples                            # in WSL
HW_TIMEOUT=10 scripts/hw-run.sh build/hw/examples/blinky.elf  # in Git Bash
```

### Example: serial-echo

[`examples/serial-echo`](examples/serial-echo) echoes what arrives on the
board's serial connector CN8 (SCI2, 115200 bps 8N1), through the
`embedded-io` `Read` / `Write` traits that rx72m-hal's `Serial` implements;
a receive error comes back as a line such as `! framing error`. Connect a
3.3 V USB serial adapter to CN8 (its TXD to pin 1, its RXD to pin 2, and
GND) and, while the runner keeps the program running, check it from
Windows: a line, 4096 bytes back to back and a break.

```sh
HW_TIMEOUT=60 scripts/hw-run.sh build/hw/examples/serial-echo.elf  # in Git Bash
powershell -File scripts/hw/serial-echo.ps1 -Port COM8         # meanwhile
```

So far it has only been checked with the debugger attached: after the
runner detaches, nothing comes out of the serial port
([`docs/NOTES-hal.md`](docs/NOTES-hal.md)).

[`examples/rx74n-serial-echo`](examples/rx74n-serial-echo) does the same on
an EK-RX74N or EK-RX74M, on the USB serial port J27 (RSCI12, CPUCLK0
600 MHz). It needs no adapter:

```sh
HW_BOARD=rx74n scripts/hw-build.sh examples                                   # in WSL
HW_BOARD=rx74n HW_TIMEOUT=60 scripts/hw-run.sh build/hw-rx74n/examples/rx74n-serial-echo.elf  # in Git Bash
powershell -File scripts/hw/serial-echo.ps1 -Port COM9                        # meanwhile
```

## Writing a program

See [`test/rust/qemu-demo`](test/rust/qemu-demo): a `#![no_main]` binary that
depends on `rx-rt`, provides `memory.x`, and builds with
`-Zbuild-std=core` and `-C link-arg=-Tlink.x`. For a board, the examples
add a HAL (`rx72m-hal` or `rx74n-hal`, which re-exports its PAC as `pac`)
and build for `target-cpu=rxv3` or `rxv3e`; the PACs need e² studio's
`.sfrx` files (see [`docs/NOTES-hal.md`](docs/NOTES-hal.md)). The target and
its ABI are described in
[`src/doc/rustc/src/platform-support/rx-none-elf.md`](https://github.com/CDEF-inc/rust/blob/rx-none-elf/src/doc/rustc/src/platform-support/rx-none-elf.md).

### Floating point

By default `f32` and `f64` use soft-float routines (compiler-builtins), even
on cores with an FPU. To use the FPU (rxv2, rxv3) and, for `f64`, the DFPU
(rxv3, rxv3e), add `+flush-denormals` to the target features:

```toml
# .cargo/config.toml
[target.rx-none-elf]
rustflags = ["-C", "link-arg=-Tlink.x", "-C", "target-cpu=rxv3",
             "-C", "target-feature=+flush-denormals"]
```

- **Speed:** arithmetic, compares and `as` conversions then compile to FPU / DFPU instructions, about 28 times faster on an RX72M (`test/rust/float-smoke`'s benchmark).
- **Results:** IEEE for NaNs, infinities, signed zeros and rounding.
- **What changes:** denormal inputs and results become ±0 (FPSW.DN / DPSW.DDN, which rx-rt sets at reset).
- **Interrupts:** rx-rt's interrupt entries save FPSW and, with the DFPU, DR0-DR3, DPSW and DCMR.

`docs/NOTES-codegen.md` ("IEEE の FPU / DFPU") has the details.

### Code size

`-C target-feature=+relax` turns on linker relaxation: lld shortens calls
(`bsr.a` → `bsr.w`) and symbol addresses (`mov.l #sym` to a 16- or 8-bit
immediate) once the addresses are known. That takes about 3 % off `.text`.
The debug info and the CFI follow the shortened code. See
`docs/NOTES-lld.md` ("リンカ緩和").

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
