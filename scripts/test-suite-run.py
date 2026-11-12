#!/usr/bin/env python3
"""Runs an llvm-test-suite program on QEMU (gdbsim-r5f562n8) as if it ran
on the host: the arguments and stdin go into a block that QEMU's loader puts
at __args (test/test-suite/syscalls.c), what the program writes to SCI0
comes out on stdout, and the exit status after the RX-EXIT marker becomes
this script's. The test-suite's TEST_SUITE_RUN_UNDER
(scripts/run-test-suite.sh).

    test-suite-run.py [--cpu MODEL] ELF [ARGS...] [< INPUT]

--cpu is QEMU's -cpu model (default QEMU_CPU, else QEMU's rx62n). lit does
not pass the environment on to the tests, so run-test-suite.sh gives it in
TEST_SUITE_RUN_UNDER. QEMU is the emulator (default
../qemu/build/qemu-system-rx next to this repository), QEMU_FLAGS more
options (default
"-icount shift=3,sleep=off": the program's clock counts the instructions it
ran, 8 ns each), TEST_TIMEOUT the seconds before it is stopped (default
1800, exit status 124).
"""
import os
import struct
import subprocess
import sys
import tempfile
import time

MARKER = b"\x04\x04RX-EXIT "
ARGS_ADDR = 0x01FB0000  # ___args in rx-qemu-libc.ld
ARGS_SIZE = 0x10000
ARGS_MAGIC = 0x56475241


def args_block(argv, stdin):
    """struct args: magic, argc, in_len, argv[argc + 1], stdin, strings."""
    head = 12 + 4 * (len(argv) + 1)
    strings = head + len(stdin)
    ptrs, blob = [], b""
    for a in argv:
        ptrs.append(ARGS_ADDR + strings + len(blob))
        blob += a.encode() + b"\0"
    data = struct.pack("<3I", ARGS_MAGIC, len(argv), len(stdin))
    data += struct.pack("<%dI" % (len(argv) + 1), *ptrs, 0) + stdin + blob
    if len(data) > ARGS_SIZE:
        sys.exit("test-suite-run: arguments and input over %d bytes" % ARGS_SIZE)
    return data


def main():
    args = sys.argv[1:]
    cpu = os.environ.get("QEMU_CPU")
    if args[:1] == ["--cpu"]:
        cpu, args = args[1], args[2:]
    elf = args[0]
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    qemu = os.environ.get("QEMU") or os.path.join(
        root, "..", "qemu", "build", "qemu-system-rx")
    timeout = float(os.environ.get("TEST_TIMEOUT", "1800"))
    stdin = b"" if sys.stdin.isatty() else sys.stdin.buffer.read()
    argv = [os.path.basename(elf)] + args[1:]
    with tempfile.TemporaryDirectory() as tmp:
        bios = os.path.join(tmp, "bios.bin")
        with open(bios, "wb") as f:
            f.write(bytes(16))
        block = os.path.join(tmp, "args.bin")
        with open(block, "wb") as f:
            f.write(args_block(argv, stdin))
        cmd = [qemu, "-M", "gdbsim-r5f562n8", "-bios", bios,
               "-display", "none", "-monitor", "none", "-nodefaults",
               "-serial", "stdio",
               "-device", "loader,file=%s,cpu-num=0" % elf,
               "-device", "loader,file=%s,addr=0x%x,force-raw=on" % (block, ARGS_ADDR)]
        if cpu:
            cmd[1:1] = ["-cpu", cpu]
        # The guest's time (CMT, the program's clock()) follows the
        # instructions it ran, 8 ns each, whatever else the host runs.
        cmd += os.environ.get("QEMU_FLAGS", "-icount shift=3,sleep=off").split()
        # QEMU's own messages (such as -icount's warning that no timer runs
        # yet) are not the program's output: kept aside, and shown only if
        # the program does not exit.
        qemu_err = open(os.path.join(tmp, "qemu.err"), "w+b")
        p = subprocess.Popen(cmd, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=qemu_err)
        os.set_blocking(p.stdout.fileno(), False)
        out = bytearray()
        status = None
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            chunk = p.stdout.read()
            if chunk:
                out += chunk
                i = out.find(MARKER)
                if i >= 0 and out.endswith(b"\n", i):
                    status = int(out[i + len(MARKER):].split(b"\n")[0])
                    del out[i:]
                    break
            elif p.poll() is not None:
                break
            else:
                time.sleep(0.005)
        p.kill()
        p.wait()
        qemu_err.seek(0)
        qemu_msgs = qemu_err.read()
        qemu_err.close()
    sys.stdout.buffer.write(out)
    sys.stdout.flush()
    if status is None:
        sys.stderr.buffer.write(qemu_msgs)
        sys.stderr.write("test-suite-run: %s did not exit (%s)\n" % (
            elf, "timeout" if time.monotonic() >= end else "QEMU stopped"))
        sys.exit(124)
    sys.exit(status & 0xff)


if __name__ == "__main__":
    main()
