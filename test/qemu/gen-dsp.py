#!/usr/bin/env python3
"""Generates the DSP instruction tests dsp-v1.s and dsp-v2.s.

The expected values come from a model of the accumulator instructions
written from the "Operation" of each instruction in the RXv3 manual
(R01US0316EJ0100, section 3): 72-bit accumulators A0 / A1 on RXv2 and
later, one 64-bit accumulator on RXv1. RACW / RACL clear the low 32 bits of
the rounded value (the manual's Operation masks the unrounded tmp, which
would make the rounding affect only the saturation; QEMU's RXv1 racw and
the instruction names round).

    gen-dsp.py OUTDIR
"""
import sys


def sext(v, bits):
    v &= (1 << bits) - 1
    return v - (1 << bits) if v >> (bits - 1) else v


def s16(v):
    return sext(v, 16)


def s32(v):
    return sext(v, 32)


class Model:
    def __init__(self, bits):
        self.bits = bits            # 72 on RXv2+, 64 on RXv1
        self.acc = [0, 0]

    def store(self, a, v):
        self.acc[a] = sext(v, self.bits)

    PRODUCT = {
        "hi": lambda rs, rs2: (s16(rs >> 16) * s16(rs2 >> 16)) << 16,
        "lo": lambda rs, rs2: (s16(rs) * s16(rs2)) << 16,
        "lh": lambda rs, rs2: (s16(rs) * s16(rs2 >> 16)) << 16,
        "e": lambda rs, rs2: s32(rs) * s32(rs2),
    }

    def dsp(self, kind, sel, a, rs, rs2):
        p = self.PRODUCT[sel](rs, rs2)
        if kind == "mul":
            self.store(a, p)
        elif kind == "mac":
            self.store(a, self.acc[a] + p)
        else:
            self.store(a, self.acc[a] - p)

    def mvfac(self, a, shift, pos):
        tmp = sext(self.acc[a] << shift, 72)
        return (tmp >> pos) & 0xffffffff

    def mvtac(self, a, pos, val):
        # mvtacgu sign-extends into the guard bits; mvtachi leaves them
        # alone (measured on the RX72M; the manual's Operation ORs in the
        # sign-extended source, its Function text does not)
        field = 0xffffffff << pos
        src = s32(val) if pos == 64 else val
        self.store(a, (self.acc[a] & ~field) | (src << pos))

    def rac(self, a, shift, rnd, long):
        if long:
            hi, lo = 0x7FFFFFFF00000000, -(1 << 63)
        else:
            hi, lo = 0x00007FFF00000000, -0x800000000000
        tmp = sext(self.acc[a] << shift, self.bits)
        if rnd:
            tmp += 0x80000000
        if tmp > hi:
            tmp = hi
        elif tmp < lo:
            tmp = lo
        else:
            tmp &= ~0xffffffff
        self.store(a, tmp)


MUL_OPS = [  # name, kind, sel, rxv1
    ("mulhi", "mul", "hi", True), ("mullo", "mul", "lo", True),
    ("mullh", "mul", "lh", False), ("emula", "mul", "e", False),
    ("machi", "mac", "hi", True), ("maclo", "mac", "lo", True),
    ("maclh", "mac", "lh", False), ("emaca", "mac", "e", False),
    ("msbhi", "msb", "hi", False), ("msblo", "msb", "lo", False),
    ("msblh", "msb", "lh", False), ("emsba", "msb", "e", False),
]
PAIRS = [
    (0x00010002, 0x00030004), (0x7fff7fff, 0x7fff7fff),
    (0x80008000, 0x80008000), (0x80007fff, 0x7fff8000),
    (0xffffffff, 0x12345678), (0x7fffffff, 0x80000000),
    (0x80000000, 0x80000000), (0xfedc8765, 0x0f0e0d0c),
]
ACCS72 = [
    0, 1, -1, (1 << 63) - 1, -(1 << 63), (1 << 71) - 1, -(1 << 71),
    0x12_3456789a_bcdef012, -0x7f_00000000_00010000,
]
ACCS64 = [0, 1, -1, (1 << 63) - 1, -(1 << 63), 0x3456789a_bcdef012,
          -0x7fff0000_00010000]


class Emitter:
    def __init__(self, v2):
        self.v2 = v2
        self.m = Model(72 if v2 else 64)
        self.lines = []

    def emit(self, s):
        self.lines.append("\t" + s)

    def comment(self, s):
        self.lines.append("; " + s)

    def check(self, reg, want):
        self.emit("mov.l\t%s, r1" % reg)
        self.emit("mov.l\t#0x%08x, r2" % (want & 0xffffffff))
        self.emit("bsr.a\t_check")

    def set_acc(self, a, v):
        """Load an accumulator: hi, then the guard (RXv2), then lo."""
        v = sext(v, self.m.bits)
        self.emit("mov.l\t#0x%08x, r6" % ((v >> 32) & 0xffffffff))
        self.emit("mvtachi\tr6, a%d" % a)
        self.m.mvtac(a, 32, (v >> 32) & 0xffffffff)
        if self.v2:
            self.emit("mov.l\t#0x%08x, r6" % ((v >> 64) & 0xffffffff))
            self.emit("mvtacgu\tr6, a%d" % a)
            self.m.mvtac(a, 64, (v >> 64) & 0xffffffff)
        self.emit("mov.l\t#0x%08x, r6" % (v & 0xffffffff))
        self.emit("mvtaclo\tr6, a%d" % a)
        self.m.mvtac(a, 0, v & 0xffffffff)
        assert self.m.acc[a] == v

    def check_acc(self, a):
        """Check an accumulator: guard, hi and lo on RXv2; hi and mi on
        RXv1 (which has no mvfaclo / mvfacgu)."""
        parts = [("mvfacgu", 64), ("mvfachi", 32), ("mvfaclo", 0)] \
            if self.v2 else [("mvfachi", 32), ("mvfacmi", 16)]
        for insn, pos in parts:
            self.emit("%s\t#0, a%d, r9" % (insn, a))
            self.check("r9", self.m.mvfac(a, 0, pos))

    def text(self):
        return "\n".join(self.lines) + "\n"


def generate(v2):
    e = Emitter(v2)
    accs = ACCS72 if v2 else ACCS64
    accsel = [0, 1] if v2 else [0]
    n = 0

    e.comment("mul* / mac* / msb* and em*a")
    for op, kind, sel, v1 in MUL_OPS:
        if not v2 and not v1:
            continue
        for i, (rs, rs2) in enumerate(PAIRS):
            a = accsel[i % len(accsel)]
            init = accs[(i + n) % len(accs)]
            n += 1
            e.comment("%s 0x%08x, 0x%08x, a%d (acc %#x)" % (op, rs, rs2, a, init))
            if v2:
                e.set_acc(1 - a, 0x55_66667777_88889999)
            e.set_acc(a, init)
            e.emit("mov.l\t#0x%08x, r4" % rs)
            e.emit("mov.l\t#0x%08x, r5" % rs2)
            e.emit("%s\tr4, r5, a%d" % (op, a))
            e.m.dsp(kind, sel, a, rs, rs2)
            e.check_acc(a)
            if v2:
                e.check_acc(1 - a)      # the other one is unchanged

    e.comment("mvfac* with shifts")
    parts = [("mvfachi", 32), ("mvfacmi", 16)]
    if v2:
        parts += [("mvfaclo", 0), ("mvfacgu", 64)]
    for i, init in enumerate(accs):
        a = accsel[i % len(accsel)]
        e.set_acc(a, init)
        for insn, pos in parts:
            for shift in ([0, 1, 2] if v2 else [0]):
                e.emit("%s\t#%d, a%d, r9" % (insn, shift, a))
                e.check("r9", e.m.mvfac(a, shift, pos))

    e.comment("mvtac* (mvtacgu sign-extends, mvtachi keeps the guard bits)")
    tacs = [("mvtachi", 32), ("mvtaclo", 0)]
    if v2:
        tacs.append(("mvtacgu", 64))
    for i, init in enumerate(accs):
        a = accsel[i % len(accsel)]
        for insn, pos in tacs:
            for val in (0x12345678, 0x87654321, 0x000000ff, 0xffffff00):
                e.set_acc(a, init)
                e.emit("mov.l\t#0x%08x, r6" % val)
                e.emit("%s\tr6, a%d" % (insn, a))
                e.m.mvtac(a, pos, val)
                e.check_acc(a)

    e.comment("racw / racl / rdacw / rdacl")
    racs = [("racw", True, False)]
    if v2:
        racs += [("racl", True, True), ("rdacw", False, False),
                 ("rdacl", False, True)]
    edges = [
        0x00003fff_ffffffff, 0x00003fff_7fffffff, 0x00003fff_80000000,
        0x00007fff_7fffffff, 0x00007fff_80000000, -0x00004000_00000000,
        -0x00004000_00000001, -0x00008000_00000000, 0x3fffffff_7fffffff,
        0x3fffffff_80000000, -0x40000000_00000000, 0x12345678_9abcdef0,
    ] + accs
    for insn, rnd, long in racs:
        for i, init in enumerate(edges):
            a = accsel[i % len(accsel)]
            for shift in (1, 2):
                e.set_acc(a, init)
                e.emit("%s\t#%d, a%d" % (insn, shift, a))
                e.m.rac(a, shift, rnd, long)
                e.check_acc(a)

    head = """; Generated by gen-dsp.py from a model of the RXv3 manual's DSP
; instructions; do not edit. %s
	.include "harness.inc"

	.text
	.global	_main
_main:
""" % ("RXv2 and later: A0 / A1, 72 bits." if v2 else
       "RXv1: A0 only, 64 bits.")
    tail = """	mov.l	#0, r1
	rts
"""
    return head + e.text() + tail


def main():
    out = sys.argv[1]
    for name, v2 in (("dsp-v1.s", False), ("dsp-v2.s", True)):
        with open("%s/%s" % (out, name), "w") as f:
            f.write(generate(v2))


if __name__ == "__main__":
    main()
