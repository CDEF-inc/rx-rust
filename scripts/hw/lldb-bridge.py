"""A bridge between LLDB and Renesas e2-server-gdb (E2 Lite).

Usage: lldb-bridge.py [--sessions N] SERVER_PORT [connect:]PORT [ELF]
                                                       (scripts/hw-lldb.sh)

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
an LLDB connection and serves it:

- the target description (target.xml, or qRegisterInfo for an LLDB without
  XML support): the core registers, the accumulators, EXTB and, on the
  RX74N / RX74M, the DFPU's (dr0-dr15 as doubles, dpsw, dcmr, decnt, depc;
  #81), numbered in order without the server's gap before dr0 (the
  bridge turns the numbers into the server's); the server's memory map.
  The AP-RX72M-0A's server crashes when dr0 is read, so there the DFPU's
  registers are left out;
- `p` / `g` from single `p` reads, turned little-endian (a DR register
  comes as two big-endian words, low first); `P` / `G`;
- `c`, `s`, `vCont` and ^C as non-stop requests, answered with the stop
  reply when the server's %Stop notification comes;
- memory, breakpoints (`m`, `M`, `Z0`, `Z1`, `z`) and monitor commands
  (`qRcmd`) passed through. A breakpoint in the code flash becomes one of
  the debugger's hardware breakpoints (the server does that).
- watchpoints (`Z2` write, `Z3` read, `Z4` access) passed through (#80).
  The server's stop does not say which one hit, so the bridge allows one
  at a time (qWatchpointSupportInfo says 1; a second `Z2`-`Z4` gets E01)
  and adds its `watch:` / `rwatch:` / `awatch:` field. (With several,
  telling them apart by the watched memory fails for reads and for
  writes of the same value.) The CPU stops after the access, which
  qHostInfo tells LLDB. (On the AP-RX72M-0A a watchpoint was once seen to
  fire only before the first run; on the EK-RX74M they fire every time.)

When LLDB kills (`k`) or disconnects, the bridge stops the program,
removes LLDB's breakpoints and waits for the next LLDB connection: in
LLDB, `kill` and `process connect` again (#86; after a rebuild, in a new
target, since LLDB keeps a target's symbols). When that comes, it
downloads ELF again, read from the file then (so an ELF rebuilt in
between is the one loaded), and resets the CPU. When LLDB detaches (`D`)
the program keeps running, and the next LLDB connection stops it where it
is; ^C ending the bridge meanwhile leaves it running. The server stays connected all along: it exits when
the bridge disconnects. --sessions N ends the bridge after N sessions
(default: serve until interrupted).
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
INTB = 0x14
# The DFPU's registers (#81), with the server's numbers from its
# target.xml: dr0-dr15 (64 bits), then dpsw, dcmr, decnt, depc.
DFPU = ["dr%d" % i for i in range(16)] + ["dpsw", "dcmr", "decnt", "depc"]

TYPES = {"r0": "data_ptr", "pc": "code_ptr", "bpc": "code_ptr",
         "psw": "psw_flags", "bpsw": "psw_flags", "fpsw": "fpsw_flags"}
TYPES.update(("dr%d" % i, "ieee_double") for i in range(16))
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


def group(name, n):
    if n < 0x10 or name == "pc":
        return "general"
    return "dfpu" if name in DFPU else "system"


def target_xml(regs):
    # regnum: the place in REGS (the bridge's numbers for `p` / `P`).
    xml = "\n".join(
        '  <reg name="%s" bitsize="%d" regnum="%d" type="%s" group="%s"/>'
        % (name, bits, i, TYPES.get(name, "uint%d" % bits), group(name, n))
        for i, (name, n, bits) in enumerate(regs))
    return ('<?xml version="1.0"?>\n<!DOCTYPE target SYSTEM "gdb-target.dtd">\n'
            '<target version="1.0">\n<architecture>rx</architecture>\n'
            '<feature name="org.gnu.gdb.rx.core">%s\n%s\n</feature>\n</target>\n'
            % (FLAGS, xml))


def server_dfpu(xml):
    """The DFPU registers in the server's target.xml, as (name, number,
    bits): only when dr0 has a number of its own (RX74N / RX74M, where the
    security extension's registers come first). The AP-RX72M-0A's server
    numbers them from 0x1c on, and reading dr0 there crashes it (#81)."""
    import re
    regs, n = {}, 0
    for m in re.finditer(r"<reg ([^>]*)>", xml):
        a = dict(re.findall(r'(\w+)="([^"]*)"', m.group(1)))
        if a.get("name") == "dr0" and "regnum" not in a:
            return []
        if "regnum" in a:
            n = int(a["regnum"])
        regs[a.get("name")] = (n, int(a.get("bitsize", "32")))
        n += 1
    if not all(name in regs for name in DFPU):
        return []
    return [(name,) + regs[name] for name in DFPU]


# For LLDB built without libxml2, which cannot read target.xml: the same
# registers through qRegisterInfo (the index is the place in the list, as
# in target.xml's regnum).
GENERIC = {"r0": "sp", "r6": "fp", "pc": "pc", "psw": "flags",
           "r1": "arg1", "r2": "arg2", "r3": "arg3", "r4": "arg4"}
DWARF = dict([("r%d" % i, i) for i in range(16)] + [("psw", 16), ("pc", 17)]
             + [("dr%d" % i, 32 + i) for i in range(16)])
SETS = {"general": "General Purpose Registers", "system": "System Registers",
        "dfpu": "DFPU Registers"}


def register_info(regs, index):
    if index >= len(regs):
        return "E45"
    name, n, bits = regs[index]
    offset = sum(b for _, _, b in regs[:index]) // 8
    encoding = ("encoding:ieee754;format:float;" if name.startswith("dr")
                else "encoding:uint;format:hex;")
    info = ("name:%s;bitsize:%d;offset:%d;%sset:%s;"
            % (name, bits, offset, encoding, SETS[group(name, n)]))
    if name in DWARF:
        info += "ehframe:%d;dwarf:%d;" % (DWARF[name], DWARF[name])
    if name in GENERIC:
        info += "generic:%s;" % GENERIC[name]
    return info


def xfer_slice(data, args):
    off, length = (int(x, 16) for x in args.split(","))
    chunk = data[off:off + length]
    return ("l" if off + length >= len(data) else "m") + chunk


class SessionEnd(Exception):
    """LLDB killed, detached (DETACHED) or went away."""
    def __init__(self, detached=False):
        super().__init__()
        self.detached = detached


class ServerGone(Exception):
    """e2-server-gdb closed the connection."""


class Bridge:
    def __init__(self, server, elf):
        self.r = server
        self.elf = elf
        self.stop = "T05thread:1;"
        self.memory_map = None
        self.regs = REGS + server_dfpu(self.xfer_server("features", "target.xml"))
        self.bits = {n: b for _, n, b in self.regs}
        # LLDB numbers the registers by their place in self.regs, through
        # target.xml and qRegisterInfo alike; the server's numbers have a
        # gap before dr0 (its security registers, which LLDB is not shown).
        self.server_num = [n for _, n, _ in self.regs]
        assert self.server_num.index(PC) == PC  # the stop reply's PC
        self.xml = target_xml(self.regs)
        self.write = None
        self.cond = threading.Condition()
        self.running = False
        self.loaded = self.elf_stamp()    # of the ELF last loaded (connect())
        self.detached = False     # the last session detached
        # LLDB's breakpoints, address -> "TYPE,ADDR,KIND" of their Z packet,
        # which step() must leave alone and which are removed when LLDB goes.
        self.breakpoints = {}
        # LLDB's watchpoints, (TYPE, address) -> length.
        self.watchpoints = {}
        self.slots = None
        self.session = 0
        self.new_session()

    def new_session(self):
        self.session += 1
        self.ack = True
        self.inbuf = b""
        self.eof = False

    # --- the server

    def getreg(self, n):
        """Register N as little-endian hex (the server sends big-endian)."""
        rep, _ = self.r.cmd("p%x" % n)
        if len(rep) != self.bits[n] // 4:
            raise RuntimeError("p%x: %s" % (n, rep))
        if self.bits[n] == 64:
            # A DR register: the low word, then the high word, each
            # big-endian (seen against dpushm.d on the RX74M). `P` takes
            # the value little-endian, as for the others.
            return (bytes.fromhex(rep[:8])[::-1] + bytes.fromhex(rep[8:])[::-1]).hex()
        return bytes.fromhex(rep)[::-1].hex()

    def resume(self, step):
        self.r.cmd("vCont;s:p1.1" if step else "vCont;c")
        self.running = True
        interrupted = False
        while True:
            note = self.r.wait_note(0.1)
            if note:
                if TRACE:
                    log("server: %s" % note)
                self.running = False
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
        if (not step and not interrupted and sig == "05" and not extra
                and int(bytes.fromhex(pc)[::-1].hex(), 16) not in self.breakpoints):
            extra = self.watch_hit()
        self.stop = "T%sthread:1;%02x:%s;%s" % (sig, PC, pc, extra.lstrip(";"))
        if not self.stop.endswith(";"):
            self.stop += ";"
        return self.stop

    def watch_hit(self):
        """The stop field for the watchpoint that stopped the CPU (#80): the
        server's stop says nothing, and comes after the access (as LLDB
        expects on RX). There is at most one (see the `Z2`-`Z4` handling)."""
        if not self.watchpoints:
            return ""
        (kind, addr), = self.watchpoints
        return ";%s:%x" % ({"2": "watch", "3": "rwatch", "4": "awatch"}[kind], addr)

    def step(self):
        """A single step. The server leaves a garbage PC after stepping an
        instruction that enters an exception (#98), so for `int #n` and
        `brk` run to the handler instead: temporary hardware breakpoints on
        the handler (from the vector table at INTB) and, in case the
        exception does not come, on the next instruction."""
        pc = self.r.getreg(PC)
        code = self.r.read(pc, 3)
        if code[:2] == b"\x75\x60":    # int #imm
            vector, size = code[2], 3
        elif code[:1] == b"\x00":      # brk: trap 0
            vector, size = 0, 1
        else:
            return self.resume(step=True)
        handler = self.r.word(self.r.getreg(INTB) + 4 * vector)
        placed = []
        try:
            for a in {handler, pc + size} - self.breakpoints.keys():
                rep = self.r.cmd("Z1,%x,1" % a)[0]
                if rep != "OK":
                    # No free hardware breakpoint, say: running would not
                    # stop at the handler, or not at all. Stay, and say so.
                    msg = ("bridge: cannot step %s at 0x%x: no hardware "
                           "breakpoint at 0x%x (%s); delete a breakpoint\n"
                           % ("brk" if size == 1 else "int #%d" % vector,
                              pc, a, rep or "not supported"))
                    log(msg.rstrip())
                    self.send("O" + msg.encode("latin-1").hex())
                    break
                placed.append(a)
            else:
                self.resume(step=False)
        finally:
            for a in placed:
                self.r.cmd("z1,%x,1" % a)
        # A step, not a breakpoint of LLDB's.
        self.stop = "T05thread:1;%02x:%s;" % (PC, self.getreg(PC))
        return self.stop

    def lldb_interrupted(self):
        """Whether LLDB sent ^C while the target runs (a ^C before any
        packet; the rest stays for packets())."""
        with self.cond:
            if self.eof:
                raise SessionEnd()
            i = self.inbuf.find(b"\x03")
            j = self.inbuf.find(b"$")
            if i < 0 or 0 <= j < i:
                return False
            self.inbuf = self.inbuf[:i] + self.inbuf[i + 1:]
            return True

    # --- LLDB

    def start_reader(self, read):
        """Read LLDB's bytes into inbuf on a thread, so that a ^C is seen
        while the target runs. The thread ends with its session."""
        session = self.session

        def loop():
            while True:
                try:
                    data = read()
                except OSError:
                    data = b""
                with self.cond:
                    if self.session != session:
                        return
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
            # The server stops after the instruction that hit a watchpoint.
            return ("triple:%s;endian:little;ptrsize:4;"
                    "watchpoint_exceptions_received:after;"
                    % TRIPLE.encode().hex())
        if p.startswith("qWatchpointSupportInfo"):
            return "num:%d;" % min(1, self.watch_slots())
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
            return register_info(self.regs, int(p[len("qRegisterInfo"):], 16))
        if p.startswith("p"):
            i = int(p[1:].split(";")[0], 16)
            if i >= len(self.server_num):
                return "E01"
            return self.getreg(self.server_num[i])
        if p.startswith("P"):
            i, v = p[1:].split(";")[0].split("=")
            i = int(i, 16)
            if i >= len(self.server_num):
                return "E01"
            return r.cmd("P%x=%s" % (self.server_num[i], v))[0]
        if p == "g" or p.startswith("g;"):
            return "".join(self.getreg(n) for _, n, _ in self.regs)
        if p.startswith("G"):
            data, off = p[1:].split(";")[0], 0
            for _, n, bits in self.regs:
                k = bits // 4
                rep = r.cmd("P%x=%s" % (n, data[off:off + k]))[0]
                if rep != "OK":
                    return rep
                off += k
            return "OK"
        if p[0] in "Zz" and p[1:2] in ("2", "3", "4"):
            # Write, read and access watchpoints (#80), one at a time: the
            # server's stop does not say which one hit.
            _, addr, length = p[1:].split(";")[0].split(",")
            key = (p[1], int(addr, 16))
            if p[0] == "Z" and self.watchpoints and key not in self.watchpoints:
                return "E01"
            rep = r.cmd(p)[0]
            if rep == "OK":
                if p[0] == "Z":
                    self.watchpoints[key] = int(length, 16)
                else:
                    self.watchpoints.pop(key, None)
            return rep
        if p[0] in "Zz" and p[1:2] in ("0", "1"):
            rep = r.cmd(p)[0]
            if rep == "OK":
                addr = int(p.split(",")[1], 16)
                if p[0] == "Z":
                    self.breakpoints[addr] = p[1:]
                else:
                    self.breakpoints.pop(addr, None)
            return rep
        if p[0] in "mM":
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
            return self.step()
        if p.startswith("D"):
            # Run on; the server stays connected for the next session.
            self.clear_points()
            r.cmd("vCont;c")
            self.running = True
            self.send("OK")
            raise SessionEnd(detached=True)
        if p in ("k", "vKill;1", "vKill"):
            # LLDB takes an exit status for `k`, OK for `vKill`.
            self.send("X09" if p == "k" else "OK")
            raise SessionEnd()
        return ""

    def halt(self):
        """Stop the program if it runs (LLDB went away, or detached)."""
        if self.running:
            self.r.cmd("vCont;t:p1.1")
            self.r.wait_note(5)
            self.running = False

    def clear_points(self):
        """Remove LLDB's breakpoints and watchpoints (LLDB went away)."""
        for z in self.breakpoints.values():
            self.r.cmd("z" + z)
        for (kind, addr), length in self.watchpoints.items():
            self.r.cmd("z%s,%x,%x" % (kind, addr, length))
        self.breakpoints = {}
        self.watchpoints = {}

    def watch_slots(self):
        """The server's count of free hardware breakpoints (8 on the RX74M),
        which watchpoints share."""
        if self.slots is None:
            self.slots = 4
            try:
                rep = self.r.mon("get_no_hw_bkpts_available")[0]
                self.slots = int(bytes.fromhex(rep).decode().strip())
            except (ValueError, UnicodeDecodeError):
                pass
        return self.slots

    def end_session(self, last=False):
        """After a session LLDB killed or left: stop, remove LLDB's
        breakpoints and watchpoints, download the ELF again and reset, for
        the next session. If the ELF changes before that connects (a
        rebuild), it is loaded again then (reload_if_changed)."""
        self.halt()
        self.clear_points()
        if not last:
            self.reload()

    def elf_stamp(self):
        try:
            st = os.stat(self.elf)
            return (st.st_mtime_ns, st.st_size)
        except (OSError, TypeError):
            return None

    def reload(self):
        """Download the ELF again (read from the file) and reset."""
        log("loading %s again" % (self.elf or "nothing"))
        # Before reading it: a change while loading is seen at the next
        # connection.
        self.loaded = self.elf_stamp()
        load(self.r, self.elf)
        self.stop = "T05thread:1;%02x:%s;" % (PC, self.getreg(PC))

    def reload_if_changed(self):
        """At a connection: the ELF rebuilt since it was loaded is loaded
        now (this takes some seconds of LLDB's handshake: scripts/hw/hw.lldb
        gives LLDB a longer packet timeout)."""
        if self.elf_stamp() != self.loaded:
            self.reload()

    def xfer_server(self, obj, annex=""):
        data, off = "", 0
        while True:
            rep = self.r.cmd("qXfer:%s:read:%s:%x,400" % (obj, annex, off))[0]
            if not rep or rep[0] not in "ml":
                return data
            data += rep[1:]
            off += len(rep) - 1
            if rep[0] == "l":
                return data

    def accept(self, where, listener):
        if where.startswith("connect:"):
            port = int(where[len("connect:"):])
            log("connecting to LLDB on localhost:%d" % port)
            # The first LLDB may come up later than the bridge (two minutes);
            # a later one whenever the user runs `process connect` again.
            end = time.time() + 120 if self.session == 1 else None
            while True:
                try:
                    return socket.create_connection(("localhost", port), timeout=5)
                except OSError:
                    if end is not None and time.time() > end:
                        raise
                    self.check_server()
                    time.sleep(1)
        log("waiting for LLDB on localhost:%s" % where)
        listener.settimeout(1)
        while True:
            try:
                return listener.accept()[0]
            except socket.timeout:
                self.check_server()

    def check_server(self):
        """End the bridge if the server went away while it waits for LLDB
        (hw-lldb-check.sh stops the server to end a bridge left waiting)."""
        s = self.r.s
        s.setblocking(False)
        try:
            gone = s.recv(1, socket.MSG_PEEK) == b""
        except BlockingIOError:
            gone = False
        except OSError:
            gone = True
        finally:
            s.settimeout(60)
        if gone:
            log("the server went away")
            raise ServerGone()

    def serve(self, where, sessions):
        """Serve LLDB sessions, SESSIONS of them (0: for ever)."""
        listener = None
        if not where.startswith("connect:"):
            listener = socket.socket()
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(("localhost", int(where)))
            listener.listen(1)
        n = 0
        while True:
            c = self.accept(where, listener)
            n += 1
            c.settimeout(None)
            c.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            if not self.running:
                self.reload_if_changed()
            else:
                # The last session detached: stop where the program is.
                self.halt()
                self.stop = "T02thread:1;%02x:%s;" % (PC, self.getreg(PC))
            self.detached = False
            self.start_reader(lambda: c.recv(65536))
            self.write = c.sendall
            log("LLDB connected")
            detached = False
            try:
                for p in self.packets():
                    if TRACE:
                        log("-> %r" % p[:200])
                    reply = self.handle(p)
                    if reply is not None:
                        self.send(reply)
            except SessionEnd as e:
                detached = e.detached
            finally:
                with self.cond:
                    self.new_session()
                try:
                    c.close()
                except OSError:
                    pass
            # Kept on the bridge for main(): ^C while waiting for the next
            # session leaves a detached program running.
            self.detached = detached
            if detached:
                log("LLDB detached; the program keeps running")
            else:
                log("LLDB disconnected")
                self.end_session(last=bool(sessions) and n >= sessions)
            if sessions and n >= sessions:
                return


def connect(port, elf):
    r = Rsp(port)
    for p in ("qSupported:swbreak+;hwbreak+", "QNonStop:1", "?", "vCont;t:p1.1",
              "Hgp1.1"):
        r.cmd(p)
    r.wait_note(2)
    r.mon("configuration_complete")
    load(r, elf, launch=True)
    return r


def load(r, elf, launch=False):
    """Download ELF (if given) and reset; the CPU is stopped. The launch
    sequence is sent once, after the first reset (as scripts/hw/run.py)."""
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
    if launch:
        run.launch(r)
    # A program in RAM starts at its entry; one in the code flash boots from
    # the reset vector, where the reset left the PC.
    if entry is not None and entry < 0xffc00000:
        r.setreg(PC, entry)


def elf_entry(path):
    import struct
    with open(path, "rb") as f:
        return struct.unpack_from("<I", f.read(28), 24)[0]


def log(text):
    print(text, flush=True)


def main():
    args = sys.argv[1:]
    sessions = 0
    if args[:1] == ["--sessions"]:
        sessions, args = int(args[1]), args[2:]
    server_port, where = int(args[0]), args[1]
    elf = args[2] if len(args) > 2 else None
    r = connect(server_port, elf)
    b = Bridge(r, elf)
    try:
        b.serve(where, sessions)
    except KeyboardInterrupt:     # ^C in hw-lldb.sh's terminal
        pass
    except ServerGone:
        log("bridge done")
        return
    try:
        # Leave a detached program running (unless a session took it over
        # since); stop any other.
        r.cmd("D;1" if b.detached else "vKill;1")
    except Exception:
        pass
    log("bridge done")


if __name__ == "__main__":
    main()
