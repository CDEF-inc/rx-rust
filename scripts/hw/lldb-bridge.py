"""A bridge between LLDB and Renesas e2-server-gdb (E2 Lite).

Usage: lldb-bridge.py SERVER_PORT [connect:]PORT [ELF]   (scripts/hw-lldb.sh)

With connect:PORT the bridge connects to an LLDB that waits on localhost:PORT
(`process connect --plugin gdb-remote listen://127.0.0.1:PORT`), retrying
for two minutes: LLDB in WSL cannot reach Windows' localhost, but Windows
reaches WSL's. With a plain PORT the bridge listens on localhost:PORT for
LLDB (`gdb-remote PORT`, an LLDB on Windows).

LLDB speaks the ordinary all-stop GDB remote protocol. e2-server-gdb does
not run the CPU until e2 studio's launch sequence has been sent, works in
non-stop mode only, returns `p` values big-endian (and takes `P`
little-endian), crashes on `g` and on the DFPU registers, and exits when
its client disconnects (scripts/hw/rsp.py, docs/NOTES-hw.md). The bridge
connects to the server once, downloads ELF (if given), resets the CPU and
replays the launch sequence, so that the CPU is stopped at the reset
vector (or at the ELF's _start, for a program in RAM). It then waits for
one LLDB connection and serves it:

- the target description (target.xml, or qRegisterInfo for an LLDB without
  XML support): the core registers, the accumulators and EXTB (not the
  DFPU registers, which the server cannot read), with the server's
  register numbers; the server's memory map;
- `p` / `g` from single `p` reads, turned little-endian; `P` / `G`;
- `c`, `s`, `vCont` and ^C as non-stop requests, answered with the stop
  reply when the server's %Stop notification comes;
- memory, breakpoints (`m`, `M`, `Z0`, `Z1`, `z`) and monitor commands
  (`qRcmd`) passed through. A breakpoint in the code flash becomes one of
  the debugger's hardware breakpoints (the server does that).
- no watchpoints: the server accepts them, but a watchpoint only fires
  once, and only when it was set before the CPU first ran after a reset
  (LLDB sets and clears its watchpoints around every stop), so LLDB is told
  that they are not supported.

When LLDB detaches (`D`) the program keeps running; when it kills (`k`) or
disconnects, the bridge stops the program and exits, and so does the
server.
"""
import os
import socket
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rsp import Rsp, PC, checksum, segments  # noqa: E402
import run  # noqa: E402

TRIPLE = "rx-none-elf"
# BRIDGE_TRACE=1 logs LLDB's packets, the replies and the server's stops.
TRACE = os.environ.get("BRIDGE_TRACE") == "1"

# (name, server register number, bits); the order is that of `g`.
REGS = [("r%d" % i, i, 32) for i in range(16)] + [
    ("usp", 0x10, 32), ("isp", 0x11, 32), ("psw", 0x12, 32), ("pc", 0x13, 32),
    ("intb", 0x14, 32), ("bpsw", 0x15, 32), ("bpc", 0x16, 32),
    ("fintv", 0x17, 32), ("fpsw", 0x18, 32),
    # The accumulators are 72 bits; the server sends them as 128.
    ("acc0", 0x19, 128), ("acc1", 0x1a, 128), ("extb", 0x1b, 32),
]
BITS = {n: b for _, n, b in REGS}

TYPES = {"r0": "data_ptr", "pc": "code_ptr", "bpc": "code_ptr",
         "psw": "psw_flags", "bpsw": "psw_flags", "fpsw": "fpsw_flags"}
FLAGS = """
  <flags id="psw_flags" size="4">
    <field name="C" start="0" end="0"/>
    <field name="Z" start="1" end="1"/>
    <field name="S" start="2" end="2"/>
    <field name="O" start="3" end="3"/>
    <field name="I" start="16" end="16"/>
    <field name="U" start="17" end="17"/>
    <field name="PM" start="20" end="20"/>
    <field name="IPL" start="24" end="27"/>
  </flags>
  <flags id="fpsw_flags" size="4">
    <field name="RM" start="0" end="1"/>
    <field name="CV" start="2" end="2"/>
    <field name="CO" start="3" end="3"/>
    <field name="CZ" start="4" end="4"/>
    <field name="CU" start="5" end="5"/>
    <field name="CX" start="6" end="6"/>
    <field name="CE" start="7" end="7"/>
    <field name="DN" start="8" end="8"/>
    <field name="EV" start="10" end="10"/>
    <field name="EO" start="11" end="11"/>
    <field name="EZ" start="12" end="12"/>
    <field name="EU" start="13" end="13"/>
    <field name="EX" start="14" end="14"/>
    <field name="FV" start="26" end="26"/>
    <field name="FO" start="27" end="27"/>
    <field name="FZ" start="28" end="28"/>
    <field name="FU" start="29" end="29"/>
    <field name="FX" start="30" end="30"/>
    <field name="FS" start="31" end="31"/>
  </flags>"""


def target_xml():
    regs = "\n".join(
        '  <reg name="%s" bitsize="%d" regnum="%d" type="%s" group="%s"/>'
        % (name, bits, n, TYPES.get(name, "uint%d" % bits),
           "general" if n < 0x10 or name == "pc" else "system")
        for name, n, bits in REGS)
    return ('<?xml version="1.0"?>\n<!DOCTYPE target SYSTEM "gdb-target.dtd">\n'
            '<target version="1.0">\n<architecture>rx</architecture>\n'
            '<feature name="org.gnu.gdb.rx.core">%s\n%s\n</feature>\n</target>\n'
            % (FLAGS, regs))


# For LLDB built without libxml2, which cannot read target.xml: the same
# registers through qRegisterInfo (the index is the server's number).
GENERIC = {"r0": "sp", "r6": "fp", "pc": "pc", "psw": "flags",
           "r1": "arg1", "r2": "arg2", "r3": "arg3", "r4": "arg4"}
DWARF = dict([("r%d" % i, i) for i in range(16)] + [("psw", 16), ("pc", 17)])


def register_info(index):
    if index >= len(REGS):
        return "E45"
    name, n, bits = REGS[index]
    offset = sum(b for _, _, b in REGS[:index]) // 8
    info = ("name:%s;bitsize:%d;offset:%d;encoding:uint;format:hex;"
            "set:%s;" % (name, bits, offset, "General Purpose Registers"
                         if n < 0x10 or name == "pc" else "System Registers"))
    if name in DWARF:
        info += "ehframe:%d;dwarf:%d;" % (DWARF[name], DWARF[name])
    if name in GENERIC:
        info += "generic:%s;" % GENERIC[name]
    return info


def xfer_slice(data, args):
    off, length = (int(x, 16) for x in args.split(","))
    chunk = data[off:off + length]
    return ("l" if off + length >= len(data) else "m") + chunk


class Bridge:
    def __init__(self, server, elf):
        self.r = server
        self.ack = True
        self.stop = "T05thread:1;"
        self.memory_map = None
        self.xml = target_xml()
        self.write = None
        self.inbuf = b""
        self.eof = False
        self.cond = threading.Condition()

    # --- the server

    def getreg(self, n):
        """Register N as little-endian hex (the server sends big-endian)."""
        rep, _ = self.r.cmd("p%x" % n)
        if len(rep) != BITS[n] // 4:
            raise RuntimeError("p%x: %s" % (n, rep))
        return bytes.fromhex(rep)[::-1].hex()

    def resume(self, step):
        self.r.cmd("vCont;s:p1.1" if step else "vCont;c")
        interrupted = False
        while True:
            note = self.r.wait_note(0.1)
            if note:
                if TRACE:
                    log("server: %s" % note)
                break
            if not interrupted and self.lldb_interrupted():
                self.r.cmd("vCont;t:p1.1")
                interrupted = True
        # e.g. "Stop:T05thread:p1.1;core:0;swbreak:;"
        body = note[len("Stop:"):] if note.startswith("Stop:") else note
        sig = body[1:3] if body[:1] == "T" else "05"
        if interrupted or sig == "00":
            sig = "02"
        extra = "".join(";" + f for f in body.split(";")[1:]
                        if f.startswith(("swbreak", "hwbreak", "watch", "rwatch",
                                         "awatch")))
        pc = self.getreg(PC)
        self.stop = "T%sthread:1;%02x:%s;%s" % (sig, PC, pc, extra.lstrip(";"))
        if not self.stop.endswith(";"):
            self.stop += ";"
        return self.stop

    def lldb_interrupted(self):
        """Whether LLDB sent ^C while the target runs (a ^C before any
        packet; the rest stays for packets())."""
        with self.cond:
            if self.eof:
                raise KeyboardInterrupt
            i = self.inbuf.find(b"\x03")
            j = self.inbuf.find(b"$")
            if i < 0 or 0 <= j < i:
                return False
            self.inbuf = self.inbuf[:i] + self.inbuf[i + 1:]
            return True

    # --- LLDB

    def start_reader(self, read):
        """Read LLDB's bytes into inbuf on a thread, so that a ^C is seen
        while the target runs."""
        def loop():
            while True:
                data = read()
                with self.cond:
                    if not data:
                        self.eof = True
                    else:
                        self.inbuf += data
                    self.cond.notify()
                if not data:
                    return
        threading.Thread(target=loop, daemon=True).start()

    def send(self, data):
        if TRACE:
            log("<- %s" % data[:200])
        b = data.encode("latin-1")
        self.write(b"$" + b + b"#" + checksum(b))

    def packets(self):
        """Yield LLDB's packets, and "\\x03" for an interrupt; acks are
        dropped (the connection is reliable)."""
        while True:
            with self.cond:
                while True:
                    c = self.inbuf[:1]
                    if c == b"\x03":
                        self.inbuf = self.inbuf[1:]
                        packet = "\x03"
                        break
                    if c and c != b"$":
                        self.inbuf = self.inbuf[1:]
                        continue
                    j = self.inbuf.find(b"#")
                    if c and j >= 0 and len(self.inbuf) >= j + 3:
                        packet = self.inbuf[1:j].decode("latin-1")
                        self.inbuf = self.inbuf[j + 3:]
                        if self.ack:
                            self.write(b"+")
                        break
                    if self.eof:
                        return
                    self.cond.wait()
            yield packet

    def handle(self, p):
        r = self.r
        if p == "\x03":
            r.cmd("vCont;t:p1.1")
            note = r.wait_note(5)
            pc = self.getreg(PC)
            self.stop = "T02thread:1;%02x:%s;" % (PC, pc)
            return self.stop if note is not None else None
        if p == "QStartNoAckMode":
            self.send("OK")
            self.ack = False
            return None
        if p.startswith("qSupported"):
            return ("PacketSize=1000;qXfer:features:read+;"
                    "qXfer:memory-map:read+;QStartNoAckMode+;swbreak+;"
                    "hwbreak+;vContSupported+")
        if p == "qHostInfo":
            return "triple:%s;endian:little;ptrsize:4;" % TRIPLE.encode().hex()
        if p == "qProcessInfo":
            return "pid:1;triple:%s;endian:little;ptrsize:4;" % (
                TRIPLE.encode().hex())
        if p == "qC":
            return "QC1"
        if p == "qfThreadInfo":
            return "m1"
        if p == "qsThreadInfo":
            return "l"
        if p == "qAttached":
            return "1"
        if p in ("?",) or p.startswith("qThreadStopInfo"):
            return self.stop
        if p.startswith("H"):
            return "OK"
        if p == "vCont?":
            return "vCont;c;C;s;S;t"
        if p.startswith("qXfer:features:read:target.xml:"):
            return xfer_slice(self.xml, p.rsplit(":", 1)[1])
        if p.startswith("qXfer:memory-map:read::"):
            if self.memory_map is None:
                self.memory_map = self.xfer_server("memory-map")
            return xfer_slice(self.memory_map, p.rsplit(":", 1)[1])
        if p.startswith("qSymbol"):
            return "OK"
        if p.startswith("qRegisterInfo"):
            return register_info(int(p[len("qRegisterInfo"):], 16))
        if p.startswith("p"):
            n = int(p[1:].split(";")[0], 16)
            return self.getreg(n) if n in BITS else "E01"
        if p.startswith("P"):
            n, v = p[1:].split(";")[0].split("=")
            if int(n, 16) not in BITS:
                return "E01"
            return r.cmd("P%s=%s" % (n, v))[0]
        if p == "g" or p.startswith("g;"):
            return "".join(self.getreg(n) for _, n, _ in REGS)
        if p.startswith("G"):
            data, off = p[1:].split(";")[0], 0
            for _, n, bits in REGS:
                k = bits // 4
                rep = r.cmd("P%x=%s" % (n, data[off:off + k]))[0]
                if rep != "OK":
                    return rep
                off += k
            return "OK"
        if p[0] in "Zz" and p[1:2] in ("2", "3", "4"):
            return ""
        if p[0] in "mMZz":
            return r.cmd(p)[0]
        if p.startswith("qRcmd,"):
            rep, out = r.cmd(p)
            if out:
                self.send("O" + out.encode("latin-1").hex())
            return rep
        if p[0] in "cC" or p.startswith("vCont;c") or p.startswith("vCont;C"):
            addr = p[1:].split(";")[0] if p[0] == "c" else ""
            if addr:
                r.setreg(PC, int(addr, 16))
            return self.resume(step=False)
        if p[0] in "sS" or p.startswith("vCont;s") or p.startswith("vCont;S"):
            return self.resume(step=True)
        if p.startswith("D"):
            r.cmd("D;1")
            self.send("OK")
            raise SystemExit(0)
        if p in ("k", "vKill;1", "vKill"):
            # LLDB takes an exit status for `k`, OK for `vKill`.
            self.send("X09" if p == "k" else "OK")
            raise KeyboardInterrupt
        return ""

    def xfer_server(self, obj):
        data, off = "", 0
        while True:
            rep = self.r.cmd("qXfer:%s:read::%x,400" % (obj, off))[0]
            if not rep or rep[0] not in "ml":
                return data
            data += rep[1:]
            off += len(rep) - 1
            if rep[0] == "l":
                return data

    def serve(self, where):
        if where.startswith("connect:"):
            port = int(where[len("connect:"):])
            log("connecting to LLDB on localhost:%d" % port)
            end = time.time() + 120
            while True:
                try:
                    c = socket.create_connection(("localhost", port), timeout=5)
                    break
                except OSError:
                    if time.time() > end:
                        raise
                    time.sleep(1)
        else:
            s = socket.socket()
            s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            s.bind(("localhost", int(where)))
            s.listen(1)
            log("waiting for LLDB on localhost:%s" % where)
            c, _ = s.accept()
        c.settimeout(None)
        c.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.start_reader(lambda: c.recv(65536))
        self.write = c.sendall
        log("LLDB connected")
        for p in self.packets():
            if TRACE:
                log("-> %r" % p[:200])
            reply = self.handle(p)
            if reply is not None:
                self.send(reply)


def connect(port, elf):
    r = Rsp(port)
    for p in ("qSupported:swbreak+;hwbreak+", "QNonStop:1", "?", "vCont;t:p1.1",
              "Hgp1.1"):
        r.cmd(p)
    r.wait_note(2)
    r.mon("configuration_complete")
    entry = None
    if elf:
        r.mon("prg_download_start_on_connect")
        for addr, data in segments(elf):
            r.write(addr, data)
        r.mon("prg_download_end")
        for addr, data in segments(elf):
            if r.read(addr, len(data)) != data:
                raise RuntimeError("verify failed at %x" % addr)
        entry = elf_entry(elf)
    r.mon("reset")
    run.launch(r)
    # A program in RAM starts at its entry; one in the code flash boots from
    # the reset vector, where the reset left the PC.
    if entry is not None and entry < 0xffc00000:
        r.setreg(PC, entry)
    return r


def elf_entry(path):
    import struct
    with open(path, "rb") as f:
        return struct.unpack_from("<I", f.read(28), 24)[0]


def log(text):
    print(text, flush=True)


def main():
    server_port, where = int(sys.argv[1]), sys.argv[2]
    elf = sys.argv[3] if len(sys.argv) > 3 else None
    r = connect(server_port, elf)
    b = Bridge(r, elf)
    try:
        b.serve(where)
    except KeyboardInterrupt:
        pass
    except SystemExit:
        log("detached; the program keeps running")
        return
    try:
        r.cmd("vKill;1")
    except Exception:
        pass
    log("LLDB disconnected")


if __name__ == "__main__":
    main()
