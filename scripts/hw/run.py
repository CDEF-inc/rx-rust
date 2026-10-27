"""Run test ELFs on the RX72M through e2-server-gdb and print the result
variables (checks, failures, ...) of each. Started by scripts/hw-run.sh.

Usage: run.py PORT SECONDS ELF...   (NAME.sym next to each NAME.elf)

The server only runs the CPU after the monitor command sequence e2 studio
sends when it launches a debug session (non-stop mode), so this replays
it. Each ELF is downloaded, the CPU reset and run from _start (a program
in RAM) or the reset vector (one in the code flash) until the hardware
breakpoint at _done, or SECONDS. Failures logged by test/hw/harness.s go
to NAME.fail. The exit status is 0 iff every test finished without
failures.
"""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rsp import Rsp, PC, segments, symbols  # noqa: E402


def launch(r):
    """e2 studio's launch sequence; `vCont;c` does nothing before it."""
    r.mon("enable_stopped_notify_on_connect")
    r.wait_note(2)
    r.mon("enable_execute_on_connect")
    r.mon("ignore_continue_during_connect")
    r.cmd("vCont;c")                    # ignored
    r.wait_note(2)
    r.mon("launch_complete")
    r.mon("all_launches_complete")


def run(r, elf, seconds):
    name = os.path.basename(elf)[:-4]
    syms = symbols(elf)
    r.mon("prg_download_start_on_connect")
    for addr, data in segments(elf):
        r.write(addr, data)
    r.mon("prg_download_end")
    for addr, data in segments(elf):
        if r.read(addr, len(data)) != data:
            raise RuntimeError("verify failed at %x" % addr)
    r.mon("reset")
    if not r.launched:
        launch(r)
        r.launched = True
    r.cmd("Z1,%x,1" % syms["done"])
    # rx-rt applications have no _start; they boot from the reset vector.
    if syms.get("start", 0xffffffff) < 0xffc00000:
        r.setreg(PC, syms["start"])
    t0 = time.time()
    r.cmd("vCont;c")
    stop = r.wait_note(seconds)
    if stop is None:
        r.cmd("vCont;t:p1.1")
        r.wait_note(5)
    el = time.time() - t0
    pc = r.getreg(PC)
    r.cmd("z1,%x,1" % syms["done"])
    # The Rust tests have no got_value / want_value.
    v = {k: r.word(syms[k]) if k in syms else 0 for k in
         ("finished", "checks", "failures", "first_failure",
          "got_value", "want_value")}
    if v["finished"] != 1:
        print("[%s/rx72m] did not finish in %.1f s (pc=%08x, checks=%d)"
              % (name, el, pc, v["checks"]))
        return False
    first = ""
    if v["failures"]:
        first = " first=%d got=%#x want=%#x" % (
            v["first_failure"], v["got_value"], v["want_value"])
    # hw-uart: whether CN8's RXD / TXD were found wired together
    if "loopback" in syms:
        first += " loopback=%d" % r.word(syms["loopback"])
    print("[%s/rx72m] checks=%d failures=%d%s (%.1f s)"
          % (name, v["checks"], v["failures"], first, el))
    if "faillog" in syms and v["failures"]:
        n = min(v["failures"], 64)
        log = r.read(syms["faillog"], 16 * n)
        with open(elf[:-4] + ".fail", "w") as f:
            f.write("# check got want address-of-the-check\n")
            for i in range(n):
                idx, got, want, at = struct.unpack_from("<4I", log, 16 * i)
                f.write("%d %08x %08x %08x\n" % (idx, got, want, at))
    return v["failures"] == 0


def main():
    port, seconds = int(sys.argv[1]), float(sys.argv[2])
    r = Rsp(port)
    r.launched = False
    for p in ("qSupported:swbreak+;hwbreak+", "QNonStop:1", "?",
              "vCont;t:p1.1", "Hgp1.1"):
        r.cmd(p)
    r.wait_note(2)
    r.mon("configuration_complete")
    ok, n = True, 0
    for elf in sys.argv[3:]:
        try:
            good = run(r, elf, seconds)
        except Exception as e:
            print("[%s] error: %r" % (elf, e))
            good = False
        ok &= good
        n += good
    try:
        r.cmd("vKill;1")
    except Exception:
        pass
    print("== %d passed, %d failed" % (n, len(sys.argv) - 3 - n))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
