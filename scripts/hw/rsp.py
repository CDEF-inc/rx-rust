"""A minimal GDB remote serial protocol client for Renesas e2-server-gdb.

The server's quirks (seen with e2 studio 2026-10 and an E2 Lite):
- `g` fails on RXv3 cores with the DFPU, so registers are read one by one
  with `p`, which returns big-endian hex; `P` takes little-endian hex.
- In non-stop mode (QNonStop:1) stop events come as `%Stop` notifications
  that must be acknowledged with `vStopped`.
- The server exits when the client disconnects.
"""
import socket
import struct
import time

PC, PSW = 0x13, 0x12


def checksum(b):
    return b"%02x" % (sum(b) & 0xFF)


class Rsp:
    def __init__(self, port=61234):
        self.s = socket.create_connection(("localhost", port), timeout=60)
        self.buf = b""
        self.notes = []

    def _getc(self):
        if not self.buf:
            self.buf = self.s.recv(65536)
            if not self.buf:
                raise EOFError("server closed the connection")
        c, self.buf = self.buf[:1], self.buf[1:]
        return c

    def _body(self):
        data = b""
        while True:
            ch = self._getc()
            if ch == b"#":
                break
            data += ch
        self._getc()
        self._getc()
        return data.decode("latin-1")

    def recv(self):
        """Return the next reply packet; keep notifications in self.notes."""
        while True:
            c = self._getc()
            if c == b"$":
                data = self._body()
                self.s.sendall(b"+")
                return data
            if c == b"%":
                self.notes.append(self._body())

    def send(self, pkt):
        b = pkt.encode("latin-1")
        frame = b"$" + b + b"#" + checksum(b)
        self.s.sendall(frame)
        while True:
            c = self._getc()
            if c == b"+":
                return
            if c == b"-":
                self.s.sendall(frame)
            if c == b"%":
                self.notes.append(self._body())

    def cmd(self, pkt):
        """Send a packet; return (reply, console output)."""
        self.send(pkt)
        out = ""
        while True:
            r = self.recv()
            if r.startswith("O") and r != "OK":
                out += bytes.fromhex(r[1:]).decode("latin-1", "replace")
                continue
            return r, out

    def mon(self, text):
        return self.cmd("qRcmd," + text.encode().hex())

    def wait_note(self, timeout):
        """Wait for a %Stop notification, acknowledge it and return it, or
        return None after TIMEOUT seconds."""
        end = time.time() + timeout
        while not self.notes and time.time() < end:
            self.s.settimeout(max(0.05, end - time.time()))
            try:
                c = self._getc()
            except socket.timeout:
                break
            finally:
                self.s.settimeout(60)
            if c == b"%":
                self.notes.append(self._body())
        if self.notes:
            n = self.notes.pop(0)
            self.cmd("vStopped")
            return n
        return None

    def read(self, addr, n):
        out = b""
        for off in range(0, n, 512):
            k = min(512, n - off)
            r, _ = self.cmd("m%x,%x" % (addr + off, k))
            if r.startswith("E") or len(r) != 2 * k:
                raise RuntimeError("read %x: %s" % (addr + off, r))
            out += bytes.fromhex(r)
        return out

    def write(self, addr, data):
        for off in range(0, len(data), 512):
            chunk = data[off:off + 512]
            r, _ = self.cmd("M%x,%x:%s" % (addr + off, len(chunk), chunk.hex()))
            if r != "OK":
                raise RuntimeError("write %x: %s" % (addr + off, r))

    def word(self, addr):
        return struct.unpack("<I", self.read(addr, 4))[0]

    def getreg(self, n):
        r, _ = self.cmd("p%x" % n)
        return int(r, 16)

    def setreg(self, n, v):
        r, _ = self.cmd("P%x=%s" % (n, struct.pack("<I", v).hex()))
        if r != "OK":
            raise RuntimeError("set reg %x: %s" % (n, r))


def segments(path):
    """Yield (load address, bytes) of the PT_LOAD segments of an ELF file."""
    d = open(path, "rb").read()
    assert d[:4] == b"\x7fELF" and d[4] == 1 and d[5] == 1
    phoff, = struct.unpack_from("<I", d, 28)
    phentsize, phnum = struct.unpack_from("<HH", d, 42)
    for i in range(phnum):
        (typ, off, vaddr, paddr, filesz, memsz, flags,
         align) = struct.unpack_from("<8I", d, phoff + i * phentsize)
        if typ == 1 and memsz:
            yield paddr, d[off:off + filesz] + b"\0" * (memsz - filesz)


def symbols(path):
    """Read NAME.sym (rx-elf-nm output) next to NAME.elf."""
    syms = {}
    for line in open(path[:-4] + ".sym"):
        a, _, name = line.split()
        syms[name.lstrip("_")] = int(a, 16)
    return syms
